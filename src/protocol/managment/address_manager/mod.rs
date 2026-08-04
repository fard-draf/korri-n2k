//! Automated lifecycle management for NMEA 2000 logical addresses:
//! initial claim, conflict detection, defense, and reclaim.
use crate::{
    error::{
        AddressManagerError,
        ClaimError::{self},
        ClaimFault::{self},
        SendPgnError,
    },
    infra::codec::traits::PgnData,
    protocol::{
        constants::address,
        managment::{
            address_claiming::{build_address_claim_frame, claim_address, AddressClaimStrategy},
            iso_name::IsoName,
        },
        transport::{
            can_frame::CanFrame,
            fast_packet::builder::FastPacketBuilder,
            traits::{can_bus::CanBus, korri_timer::KorriTimer, pgn_sender::PgnSender},
            FAST_PACKET_INTER_FRAME_DELAY_MS,
        },
    },
};

/// NMEA2000/J1939-compliant address manager.
/// Handles address defense and automatic reclaim.
pub struct AddressManager<'a, C: CanBus, T: KorriTimer> {
    /// CAN bus implementation used to send/receive frames.
    can_bus: C,
    /// Asynchronous timer enforcing delays between claim attempts.
    timer: T,
    /// Node NAME identifier (64 bits).
    my_name: IsoName,
    /// Address Claim strategy used.
    strategy: AddressClaimStrategy<'a>,
    /// Active address currently owned by the node.
    current_address: u8,
}

impl<'a, C: CanBus, T: KorriTimer> AddressManager<'a, C, T>
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
        my_name: IsoName,
        strategy: AddressClaimStrategy<'a>,
    ) -> Result<Self, ClaimError<C::Error>> {
        // initial claim
        let current_address = claim_address(&mut can_bus, &mut timer, my_name, strategy).await?;

        Ok(Self {
            can_bus,
            timer,
            my_name,
            strategy,
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
        if source_address == address::NULL {
            Ok(())
        } else {
            self.can_bus
                .send_pgn(pgn_data, pgn, source_address, destination, &mut self.timer)
                .await
        }
    }

    /// Process an incoming frame and apply address management rules.
    ///
    /// Returns `Ok(Some(frame))` for application frames or `Ok(None)` for consumed
    /// frames (claim/defense).
    pub async fn handle_frame(
        &mut self,
        frame: &CanFrame,
    ) -> Result<Option<CanFrame>, AddressManagerError<C::Error>> {
        // Check if this is a claim frame targeting our address
        if frame.id.pgn() == 60928
            && frame.id.source_address() == self.current_address
            && frame.len == 8
        {
            let their_name = IsoName::from_raw(u64::from_le_bytes(frame.data));

            // In J1939/NMEA2000 the lowest NAME wins
            if self.my_name > their_name {
                // We lose, reclaim a new address
                match self.reclaim().await {
                    Ok(addr) => self.current_address = addr,
                    Err(_) => {
                        return Err(AddressManagerError::Claim(
                            ClaimFault::RequestAddressClaimErr,
                        ))
                    }
                }
                Ok(None)
            } else if their_name != self.my_name {
                // We win, defend our address
                self.defend()
                    .await
                    .map_err(|_| ClaimFault::RequestAddressClaimErr)?;
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
    pub async fn recv(&mut self) -> Result<Option<CanFrame>, AddressManagerError<C::Error>> {
        loop {
            let frame = self
                .can_bus
                .recv()
                .await
                .map_err(|e| AddressManagerError::Bus(e))?;
            if let Some(app_frame) = self.handle_frame(&frame).await? {
                return Ok(Some(app_frame));
            }
        }
    }

    /// Re-issue a claim to defend the current address (PGN 60928).
    async fn defend(&mut self) -> Result<(), AddressManagerError<C::Error>> {
        let claim_frame = build_address_claim_frame(self.my_name, self.current_address)
            .map_err(AddressManagerError::Build)?;

        self.can_bus
            .send(&claim_frame)
            .await
            .map_err(AddressManagerError::Bus)
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
        let builder = FastPacketBuilder::new(pgn, source_address, destination, payload)
            .with_priority(priority);
        let mut is_first = true;

        for frame in builder.build() {
            let frame = frame.map_err(SendPgnError::Build)?;

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
    async fn reclaim(&mut self) -> Result<u8, AddressManagerError<C::Error>> {
        claim_address(
            &mut self.can_bus,
            &mut self.timer,
            self.my_name,
            self.strategy,
        )
        .await
        .map_err(|_| AddressManagerError::Claim(ClaimFault::RequestAddressClaimErr))
    }
}
