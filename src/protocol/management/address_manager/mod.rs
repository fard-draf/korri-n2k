use core::future::pending;

use futures_util::{
    future::{select, Either},
    pin_mut,
};

use crate::{
    error::{
        ClaimFault::{self},
        SendPgnError,
    },
    infra::codec::traits::PgnData,
    protocol::{
        management::{
            address_claiming::{
                engine::{AddressClaimEngine, ClaimOutput},
                AddressClaimStrategy,
            },
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

/// Address manager: the claim engine plus the bus and clock it runs on.
///
/// It implements the J1939-81 address claiming subset NMEA 2000 needs: the
/// initial claim, NAME arbitration, defence, loss and reclaim, Cannot Claim with
/// retry, and answers to ISO Requests for PGN 60928.
///
/// Two parts of J1939-81 are **not** implemented. There is no pseudo-random
/// delay before the first claim, and ISO Commanded Address (PGN 65240) is
/// treated as ordinary traffic. The latter arrives by BAM and needs the ISO
/// Transport Protocol, which this crate does not have.
pub struct AddressManager<'a, C: CanBus, T: KorriTimer> {
    can_bus: C,
    timer: T,
    engine: AddressClaimEngine<'a>,
}

impl<'a, C: CanBus, T: KorriTimer> AddressManager<'a, C, T>
where
    C::Error: core::fmt::Debug,
{
    /// Build the manager. The claim happens under the runner.
    pub fn new(
        can_bus: C,
        timer: T,
        name: IsoName,
        strategy: AddressClaimStrategy<'a>,
    ) -> Result<Self, ClaimFault> {
        let engine = AddressClaimEngine::new(name, strategy)?;

        Ok(Self {
            can_bus,
            timer,
            engine,
        })
    }

    /// Return None until claimed.
    pub fn claimed_address(&self) -> Option<u8> {
        self.engine.claimed_address()
    }

    /// Poll the engine. Reads the clock itself: the runner never handles time.
    pub fn poll(&mut self, rx: Option<&CanFrame>) -> ClaimOutput {
        self.engine.poll(self.timer.now_ms(), rx)
    }

    #[cfg(any(feature = "embassy", feature = "tokio"))]
    pub(crate) fn tx_sent(&mut self) -> ClaimOutput {
        self.engine.tx_sent(self.timer.now_ms())
    }

    /// Emit a frame decided by the engine. Deliberately not guarded by 'claimed_address()':
    /// a CannotClaim legitimately goes out with SA = 254.
    #[cfg(any(feature = "embassy", feature = "tokio"))]
    pub(crate) async fn emit_claim(&mut self, frame: &CanFrame) -> Result<(), C::Error> {
        self.can_bus.send(frame).await
    }

    /// Wait for a frame, or for the absolute deadline `wake_at_ms`, whichever
    /// comes first. `None` waits on the bus alone.
    ///
    /// Ok(None) means the deadline expired with no frame.
    pub async fn recv_until(
        &mut self,
        wake_at_ms: Option<u64>,
    ) -> Result<Option<CanFrame>, C::Error> {
        // Split the borrow: the receive future and the delay future hold
        // different fields of `self` at the same time.
        let Self { can_bus, timer, .. } = self;

        // A deadline already in the past yields a zero delay, not a negative one.
        let remaining_ms =
            wake_at_ms.map(|at| at.saturating_sub(timer.now_ms()).min(u32::MAX as u64) as u32);

        let recv = can_bus.recv();
        pin_mut!(recv);

        let deadline = async {
            match remaining_ms {
                Some(delay_ms) => timer.delay_ms(delay_ms).await,
                // No deadline: only a frame can end this wait.
                None => pending::<()>().await,
            }
        };
        pin_mut!(deadline);

        match select(recv, deadline).await {
            Either::Left((frame, _)) => frame.map(Some),
            Either::Right((_, _)) => Ok(None),
        }
    }

    /// Send a PGN on the bus with automatic Fast Packet handling and inter-frame delays.
    pub async fn send_pgn<P: PgnData>(
        &mut self,
        pgn_data: &P,
        pgn: u32,
        destination: Option<u8>,
    ) -> Result<(), SendPgnError<C::Error>> {
        let Some(source_addr) = self.claimed_address() else {
            return Err(SendPgnError::NotClaimed);
        };
        self.can_bus
            .send_pgn(pgn_data, pgn, source_addr, destination, &mut self.timer)
            .await
    }

    /// Send a pre-built payload using the current logical address.
    pub async fn send_payload(
        &mut self,
        pgn: u32,
        priority: u8,
        destination: Option<u8>,
        payload: &[u8],
    ) -> Result<(), SendPgnError<C::Error>> {
        let Some(source_addr) = self.claimed_address() else {
            return Err(SendPgnError::NotClaimed);
        };
        let builder =
            FastPacketBuilder::new(pgn, source_addr, destination, payload).with_priority(priority);
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
}
