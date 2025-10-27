//! Automated lifecycle management for NMEA 2000 logical addresses:
//! initial claim, conflict detection, defense, and reclaim.
use crate::{
    error::{ClaimError, SendPgnError},
    infra::codec::traits::PgnData,
    protocol::{
        managment::address_claiming::claim_address,
        transport::{
            can_frame::CanFrame,
            can_id::CanId,
            fast_packet::builder::FastPacketBuilder,
            traits::{can_bus::CanBus, korri_timer::KorriTimer, pgn_sender::PgnSender},
            FAST_PACKET_INTER_FRAME_DELAY_MS,
        },
    },
};

/// NMEA2000/J1939-compliant address manager.
/// Handles address defense and automatic reclaim.
pub struct AddressManager<C: CanBus, T: KorriTimer> {
    /// CAN bus implementation used to send/receive frames.
    can_bus: C,
    /// Asynchronous timer enforcing delays between claim attempts.
    timer: T,
    /// Node NAME identifier (64 bits).
    my_name: u64,
    /// Preferred address used during the initial claim and subsequent reclaims.
    preferred_address: u8,
    /// Active address currently owned by the node.
    current_address: u8,
}

impl<C: CanBus, T: KorriTimer> AddressManager<C, T>
where
    C::Error: core::fmt::Debug,
{
    /// Perform the initial claim and instantiate the manager with the obtained address.
    ///
    /// This async constructor waits until a valid address is claimed or an unrecoverable
    /// CAN bus error occurs. It only returns once the claim succeeds or fails definitively.
    pub async fn new(
        mut can_bus: C,
        mut timer: T,
        my_name: u64,
        preferred_address: u8,
    ) -> Result<Self, ClaimError<C::Error>> {
        // Perform the initial claim
        let current_address =
            claim_address(&mut can_bus, &mut timer, my_name, preferred_address).await?;

        Ok(Self {
            can_bus,
            timer,
            my_name,
            preferred_address,
            current_address,
        })
    }

    /// Return the address currently held by the manager.
    pub fn current_address(&self) -> u8 {
        self.current_address
    }

    /// Send a frame on the CAN bus using the current address as source.
    pub async fn send(&mut self, frame: &CanFrame) -> Result<(), C::Error> {
        self.can_bus.send(frame).await
    }

    /// Send a PGN on the bus with automatic Fast Packet handling and inter-frame delays.
    ///
    /// High-level helper that covers:
    /// - **Automatic serialization** of the PGN
    /// - **Fast Packet segmentation** for messages > 8 bytes
    /// - **Inter-frame throttling** to avoid TX buffer saturation
    /// - **Automatic source address** (current manager address)
    ///
    /// Returns [`SendPgnError`] when serialization, Fast Packet construction,
    /// or CAN bus transmission fails.
    pub async fn send_pgn<P: PgnData>(
        &mut self,
        pgn_data: &P,
        pgn: u32,
        destination: Option<u8>,
    ) -> Result<(), SendPgnError<C::Error>> {
        let source_address = self.current_address;
        self.can_bus
            .send_pgn(pgn_data, pgn, source_address, destination, &mut self.timer)
            .await
    }

    /// Process an incoming frame and apply address management rules.
    ///
    /// Returns `Ok(Some(frame))` for application frames or `Ok(None)` for consumed
    /// frames (claim/defense).
    pub async fn handle_frame(&mut self, frame: &CanFrame) -> Result<Option<CanFrame>, C::Error> {
        // Check if this is a claim frame targeting our address
        if frame.id.pgn() == 60928
            && frame.id.source_address() == self.current_address
            && frame.len == 8
        {
            let their_name = u64::from_le_bytes(frame.data);

            // In J1939/NMEA2000 the lowest NAME wins
            if self.my_name > their_name {
                // We lose, reclaim a new address
                self.reclaim().await.ok();
                Ok(None)
            } else if their_name != self.my_name {
                // We win, defend our address
                self.defend().await?;
                Ok(None)
            } else {
                // Same NAME (ours), ignore
                Ok(None)
            }
        } else {
            // Regular frame, forward to the application
            Ok(Some(frame.clone()))
        }
    }

    /// Blocking receive loop that filters out address management frames.
    pub async fn recv(&mut self) -> Result<Option<CanFrame>, C::Error> {
        loop {
            let frame = self.can_bus.recv().await?;
            if let Some(app_frame) = self.handle_frame(&frame).await? {
                return Ok(Some(app_frame));
            }
            // Otherwise it was absorbed by address management, continue listening
        }
    }

    /// Re-issue a claim to defend the current address (PGN 60928).
    async fn defend(&mut self) -> Result<(), C::Error> {
        let claim_frame = CanFrame {
            id: CanId::builder(60928, self.current_address)
                .to_destination(255)
                .with_priority(6)
                .build()
                .expect("PGN 60928 with destination 255 must always produce a valid CanId"),
            data: self.my_name.to_le_bytes(),
            len: 8,
        };

        self.can_bus.send(&claim_frame).await
    }

    /// Send a pre-built payload using the current logical address.
    pub async fn send_payload(
        &mut self,
        pgn: u32,
        priority: u8,
        destination: Option<u8>,
        payload: &[u8],
    ) -> Result<(), SendPgnError<C::Error>> {
        let source_address = self.current_address;
        let builder = FastPacketBuilder::new(pgn, source_address, destination, payload);
        let mut is_first = true;

        for frame in builder.build() {
            let mut frame = frame.map_err(SendPgnError::Build)?;
            frame.id.0 = (frame.id.0 & !(0x7 << 26)) | (((priority & 0x07) as u32) << 26);

            if !is_first && payload.len() > 8 {
                self.timer.delay_ms(FAST_PACKET_INTER_FRAME_DELAY_MS).await;
            }

            self.can_bus
                .send(&frame)
                .await
                .map_err(SendPgnError::Send)?;

            is_first = false;
        }

        Ok(())
    }

    /// Attempt to acquire a new address after losing the previous one.
    async fn reclaim(&mut self) -> Result<(), ClaimError<C::Error>> {
        // Move to the NULL address temporarily
        self.current_address = 255;

        // Reclaim a new address
        let new_address = claim_address(
            &mut self.can_bus,
            &mut self.timer,
            self.my_name,
            self.preferred_address,
        )
        .await?;

        self.current_address = new_address;
        Ok(())
    }
}
