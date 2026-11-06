//! `PgnSender` against a recording bus and timer. AUD-012 b: a codec reporting more
//! bytes than the 223-byte buffer holds is a serialization error, not a panic.
use core::future::pending;
use korri_n2k::core::PgnValue;
use korri_n2k::error::{DeserializationError, SendPgnError, SerializationError};
use korri_n2k::infra::codec::traits::{FieldAccess, PgnData};
use korri_n2k::protocol::transport::can_frame::CanFrame;
use korri_n2k::protocol::transport::traits::korri_timer::{Clock, KorriTimer};
use korri_n2k::protocol::transport::traits::{can_bus::CanBus, pgn_sender::PgnSender};

/// 129540 is a Fast Packet PGN.
const FAST_PACKET_PGN: u32 = 129540;

/// What the bus saw. `fail_at` makes the n-th `send` (0-based) return an error.
#[derive(Default)]
struct RecordingBus {
    accepted: Vec<CanFrame>,
    attempts: usize,
    fail_at: Option<usize>,
}

impl CanBus for RecordingBus {
    type Error = &'static str;

    async fn send(&mut self, frame: &CanFrame) -> Result<(), Self::Error> {
        let attempt = self.attempts;
        self.attempts += 1;
        if self.fail_at == Some(attempt) {
            return Err("bus refused");
        }
        self.accepted.push(*frame);
        Ok(())
    }

    async fn recv(&mut self) -> Result<CanFrame, Self::Error> {
        pending().await
    }
}

#[derive(Default)]
struct RecordingTimer {
    delays: Vec<u32>,
}

impl Clock for RecordingTimer {
    fn now_ms(&self) -> u64 {
        0
    }
}

impl KorriTimer for RecordingTimer {
    async fn delay_ms(&mut self, millis: u32) {
        self.delays.push(millis);
    }
}

/// A codec double: writes `bytes` bytes `0..` then reports `reported`.
struct Payload {
    bytes: usize,
    reported: Result<usize, ()>,
}

impl FieldAccess for Payload {
    fn field(&self, _: &'static str) -> Option<PgnValue> {
        None
    }
    fn field_mut(&mut self, _: &'static str, _: PgnValue) -> Option<()> {
        None
    }
}

impl PgnData for Payload {
    fn from_payload(_: &[u8]) -> Result<Self, DeserializationError> {
        Err(DeserializationError::InvalidDataLength)
    }
    fn to_payload(&self, buffer: &mut [u8]) -> Result<usize, SerializationError> {
        for (i, byte) in buffer.iter_mut().take(self.bytes).enumerate() {
            *byte = i as u8;
        }
        self.reported.map_err(|_| SerializationError::InvalidData)
    }
}

async fn send(
    payload: &Payload,
    pgn: u32,
    destination: Option<u8>,
    bus: &mut RecordingBus,
    timer: &mut RecordingTimer,
) -> Result<(), SendPgnError<&'static str>> {
    bus.send_pgn(payload, pgn, 42, destination, timer).await
}

/// AUD-012 b: a codec reporting more bytes than the 223-byte buffer holds.
#[tokio::test]
async fn a_codec_reporting_more_than_the_buffer_is_a_serialization_error() {
    let (mut bus, mut timer) = Default::default();
    let oversized = Payload {
        bytes: 223,
        reported: Ok(224),
    };
    let result = send(&oversized, FAST_PACKET_PGN, None, &mut bus, &mut timer).await;
    assert!(matches!(result, Err(SendPgnError::Serialization)));
    assert_eq!(bus.attempts, 0);
    assert!(timer.delays.is_empty());
}
