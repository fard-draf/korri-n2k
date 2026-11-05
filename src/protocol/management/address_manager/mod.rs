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
            address_claiming::{engine::AddressClaimEngine, AddressClaimStrategy},
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

#[cfg(any(feature = "embassy", feature = "tokio"))]
use crate::protocol::management::address_claiming::engine::ClaimOutput;

/// Address manager: the claim engines plus the bus and clock they run on.
///
/// It implements the J1939-81 address claiming subset NMEA 2000 needs: the
/// initial claim, NAME arbitration, defence, loss and reclaim, Cannot Claim with
/// retry, and answers to ISO Requests for PGN 60928.
///
/// `N` is the number of Controller Applications sharing this bus, one NAME and
/// one address each. `N = 1`, the default, is a plain node.
///
/// A CAN controller does not receive its own frames, so the runner loops local
/// emissions back to the engines. Single-frame application emissions always
/// are: J1939-21 requires a Controller Application to answer its own global
/// Request for Address Claim. Above one Controller Application, claims are too,
/// so two local NAMEs aiming at the same address see each other's claims.
///
/// Two parts of J1939-81 are **not** implemented. There is no pseudo-random
/// delay before the first claim, and ISO Commanded Address (PGN 65240) is
/// treated as ordinary traffic. The latter arrives by BAM and needs the ISO
/// Transport Protocol, which this crate does not have.
pub struct AddressManager<'a, C: CanBus, T: KorriTimer, const N: usize = 1> {
    can_bus: C,
    timer: T,
    engines: [AddressClaimEngine<'a>; N],
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
        Self::with_controllers(can_bus, timer, [(name, strategy)])
    }

    /// Return None until claimed.
    pub fn claimed_address(&self) -> Option<u8> {
        self.claimed_address_at(0)
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
        self.send_payload_at(0, pgn, priority, destination, payload, &mut |_| {})
            .await
            .map(|_| ())
    }
}

impl<'a, C: CanBus, T: KorriTimer, const N: usize> AddressManager<'a, C, T, N>
where
    C::Error: core::fmt::Debug,
{
    /// Build a manager for `N` Controller Applications sharing one bus.
    ///
    /// Fails if a NAME disagrees with its strategy, or if two Controller
    /// Applications share a NAME: each would take the other's claim for its own
    /// and never detect the conflict.
    pub fn with_controllers(
        can_bus: C,
        timer: T,
        controllers: [(IsoName, AddressClaimStrategy<'a>); N],
    ) -> Result<Self, ClaimFault> {
        const { assert!(N > 0, "at least one Controller Application is required") };

        for (index, (name, _)) in controllers.iter().enumerate() {
            if controllers[..index].iter().any(|(other, _)| other == name) {
                return Err(ClaimFault::DuplicateName);
            }
        }

        let mut fault = None;
        let engines = controllers.map(|(name, strategy)| {
            AddressClaimEngine::new(name, strategy)
                .map_err(|err| fault = Some(err))
                .ok()
        });
        if let Some(fault) = fault {
            return Err(fault);
        }

        Ok(Self {
            can_bus,
            timer,
            engines: engines.map(|engine| engine.expect("every engine was built")),
        })
    }

    /// Address held by Controller Application `index`, `None` until claimed.
    pub(crate) fn claimed_address_at(&self, index: usize) -> Option<u8> {
        self.engines[index].claimed_address()
    }

    /// Poll every engine at the same instant with the same `rx`. Its own
    /// claim looped back is a no-op for an engine (`OwnClaim`); its own global
    /// request is answered, as J1939-21 requires.
    #[cfg(any(feature = "embassy", feature = "tokio"))]
    pub(crate) fn poll_all(&mut self, rx: Option<&CanFrame>) -> [ClaimOutput; N] {
        let now_ms = self.timer.now_ms();
        core::array::from_fn(|index| self.engines[index].poll(now_ms, rx))
    }

    /// Emit what `poll_all` decided, engine by engine, and return the nearest
    /// deadline. Each engine gets its `tx_sent` before any other poll, so none
    /// is ever polled with a send pending, which would drop its `rx`.
    /// `on_sent` sees every frame once it reached the bus.
    #[cfg(any(feature = "embassy", feature = "tokio"))]
    pub(crate) async fn emit_all(
        &mut self,
        outputs: &[ClaimOutput; N],
        loopback: &mut [Option<CanFrame>; N],
        on_sent: &mut impl FnMut(&CanFrame),
    ) -> Result<Option<u64>, C::Error> {
        let mut wake_at_ms: Option<u64> = None;

        for (index, output) in outputs.iter().enumerate() {
            let mut output = *output;
            // Always emitted, whatever the status: a `Claimed` returned together
            // with a defence frame still owes that frame to the bus.
            if let Some(frame) = output.tx {
                self.can_bus.send(&frame).await?;
                output = self.engines[index].tx_sent(self.timer.now_ms());
                on_sent(&frame);
                // A lone engine gains nothing from its own claim.
                if N > 1 {
                    loopback[index] = Some(frame);
                }
            }
            wake_at_ms = match (wake_at_ms, output.wake_at_ms) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            };
        }

        Ok(wake_at_ms)
    }

    /// Emit a frame as is. The caller checks the source address against the
    /// one its Controller Application holds.
    #[cfg(any(feature = "embassy", feature = "tokio"))]
    pub(crate) async fn emit_raw(&mut self, frame: &CanFrame) -> Result<(), C::Error> {
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

    /// Send a payload from Controller Application `index`. Returns the frame
    /// when the payload fit in one, so the runner can loop it back. `on_sent`
    /// sees every frame once it reached the bus, Fast Packet fragments included.
    pub(crate) async fn send_payload_at(
        &mut self,
        index: usize,
        pgn: u32,
        priority: u8,
        destination: Option<u8>,
        payload: &[u8],
        on_sent: &mut impl FnMut(&CanFrame),
    ) -> Result<Option<CanFrame>, SendPgnError<C::Error>> {
        let Some(source_addr) = self.claimed_address_at(index) else {
            return Err(SendPgnError::NotClaimed);
        };
        let builder =
            FastPacketBuilder::new(pgn, source_addr, destination, payload).with_priority(priority);
        let mut sent = 0usize;
        let mut last = None;

        for frame in builder.build() {
            let frame = frame.map_err(SendPgnError::Build)?;

            if sent > 0 {
                self.timer.delay_ms(FAST_PACKET_INTER_FRAME_DELAY_MS).await;
            }

            self.can_bus
                .send(&frame)
                .await
                .map_err(SendPgnError::Send)?;
            on_sent(&frame);

            sent += 1;
            last = Some(frame);
        }

        Ok(if sent == 1 { last } else { None })
    }
}
