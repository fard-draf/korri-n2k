//! `PgnSender` against a recording bus and timer: how many frames go out, how many
//! delays separate them, and that a failure stops the loop at once (Q-04), plus the
//! two boundaries AUD-012 b and c close.
use core::future::pending;
use korri_n2k::core::PgnValue;
use korri_n2k::error::{CanIdBuildError, DeserializationError, SendPgnError, SerializationError};
use korri_n2k::infra::codec::traits::{FieldAccess, PgnData};
use korri_n2k::protocol::transport::fast_packet::builder::FastPacketBuilder;
use korri_n2k::protocol::transport::traits::korri_timer::{Clock, KorriTimer};
use korri_n2k::protocol::transport::traits::{can_bus::CanBus, pgn_sender::PgnSender};
use korri_n2k::protocol::transport::{can_frame::CanFrame, FAST_PACKET_INTER_FRAME_DELAY_MS};

/// 129025 is single frame; 129540 is a Fast Packet PGN, 59904 is addressed.
const SINGLE_FRAME_PGN: u32 = 129025;
const FAST_PACKET_PGN: u32 = 129540;
const ADDRESSED_PGN: u32 = 59904;

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

impl Payload {
    fn of(len: usize) -> Self {
        Self {
            bytes: len,
            reported: Ok(len),
        }
    }
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

#[tokio::test]
async fn a_single_frame_pgn_of_8_bytes_is_one_frame_without_delay() {
    let (mut bus, mut timer) = Default::default();
    send(
        &Payload::of(8),
        SINGLE_FRAME_PGN,
        None,
        &mut bus,
        &mut timer,
    )
    .await
    .unwrap();
    assert_eq!(bus.accepted.len(), 1);
    assert_eq!(bus.accepted[0].len, 8);
    assert_eq!(bus.accepted[0].data, [0, 1, 2, 3, 4, 5, 6, 7]);
    assert_eq!(bus.accepted[0].id.pgn(), SINGLE_FRAME_PGN);
    assert_eq!(bus.accepted[0].id.source_address(), 42);
    assert!(timer.delays.is_empty());
}

#[tokio::test]
async fn a_short_fast_packet_is_one_frame_without_delay() {
    let (mut bus, mut timer) = Default::default();
    send(&Payload::of(3), FAST_PACKET_PGN, None, &mut bus, &mut timer)
        .await
        .unwrap();
    assert_eq!(bus.accepted.len(), 1);
    let frame = bus.accepted[0];
    assert_eq!(frame.len, 8);
    assert_eq!(frame.data[0] & 0x1F, 0, "frame counter");
    assert_eq!(&frame.data[1..5], &[3, 0, 1, 2], "length then payload");
    assert_eq!(&frame.data[5..], &[0xFF; 3], "padding");
    assert!(timer.delays.is_empty());
}

#[tokio::test]
async fn seven_bytes_make_two_frames_and_one_delay() {
    let (mut bus, mut timer) = Default::default();
    send(&Payload::of(7), FAST_PACKET_PGN, None, &mut bus, &mut timer)
        .await
        .unwrap();
    assert_eq!(bus.accepted.len(), 2);
    assert_eq!(timer.delays, vec![FAST_PACKET_INTER_FRAME_DELAY_MS]);
    assert_eq!(FAST_PACKET_INTER_FRAME_DELAY_MS, 2);
}

#[tokio::test]
async fn the_largest_payload_makes_32_frames_and_31_delays() {
    let (mut bus, mut timer) = Default::default();
    send(
        &Payload::of(223),
        FAST_PACKET_PGN,
        None,
        &mut bus,
        &mut timer,
    )
    .await
    .unwrap();
    assert_eq!(bus.accepted.len(), 32);
    assert_eq!(timer.delays, vec![2; 31]);
    let counters: Vec<u8> = bus.accepted.iter().map(|f| f.data[0] & 0x1F).collect();
    assert_eq!(counters, (0..32).collect::<Vec<u8>>());
}

#[tokio::test]
async fn a_codec_error_sends_nothing() {
    let (mut bus, mut timer) = Default::default();
    let failing = Payload {
        bytes: 0,
        reported: Err(()),
    };
    let result = send(&failing, SINGLE_FRAME_PGN, None, &mut bus, &mut timer).await;
    assert!(matches!(result, Err(SendPgnError::Serialization)));
    assert_eq!(bus.attempts, 0);
    assert!(timer.delays.is_empty());
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

#[tokio::test]
async fn an_identifier_error_sends_nothing() {
    let (mut bus, mut timer) = Default::default();
    // Addressed PGN without a destination.
    let missing = send(&Payload::of(8), ADDRESSED_PGN, None, &mut bus, &mut timer).await;
    assert!(matches!(
        missing,
        Err(SendPgnError::Build(CanIdBuildError::InvalidForBroadcast))
    ));
    // PGN past 18 bits.
    let past = send(
        &Payload::of(8),
        0x4_0000 | SINGLE_FRAME_PGN,
        None,
        &mut bus,
        &mut timer,
    )
    .await;
    assert!(matches!(
        past,
        Err(SendPgnError::Build(CanIdBuildError::InvalidData))
    ));
    assert_eq!(bus.attempts, 0);
    assert!(timer.delays.is_empty());
}

#[tokio::test]
async fn a_bus_error_stops_the_loop_at_once() {
    // 20 bytes make 3 frames; the bus refuses the second.
    let (mut bus, mut timer) = (
        RecordingBus {
            fail_at: Some(1),
            ..Default::default()
        },
        RecordingTimer::default(),
    );
    let result = send(
        &Payload::of(20),
        FAST_PACKET_PGN,
        None,
        &mut bus,
        &mut timer,
    )
    .await;
    assert!(matches!(result, Err(SendPgnError::Send("bus refused"))));
    assert_eq!(bus.attempts, 2, "no send after the refusal");
    assert_eq!(bus.accepted.len(), 1);
    assert_eq!(
        timer.delays,
        vec![2],
        "only the delay before the refused frame"
    );

    // Refused first frame: nothing else is tried, no delay.
    let (mut bus, mut timer) = (
        RecordingBus {
            fail_at: Some(0),
            ..Default::default()
        },
        RecordingTimer::default(),
    );
    let result = send(
        &Payload::of(20),
        FAST_PACKET_PGN,
        None,
        &mut bus,
        &mut timer,
    )
    .await;
    assert!(matches!(result, Err(SendPgnError::Send(_))));
    assert_eq!(bus.attempts, 1);
    assert!(timer.delays.is_empty());
}

/// AUD-012 c: an empty payload is one error, not a silent success.
#[tokio::test]
async fn an_empty_payload_is_refused_with_no_bus_call() {
    for pgn in [SINGLE_FRAME_PGN, FAST_PACKET_PGN] {
        let (mut bus, mut timer) = Default::default();
        let result = send(&Payload::of(0), pgn, None, &mut bus, &mut timer).await;
        assert!(
            matches!(
                result,
                Err(SendPgnError::Build(CanIdBuildError::EmptyPayload))
            ),
            "pgn {pgn}"
        );
        assert_eq!(bus.attempts, 0);
        assert!(timer.delays.is_empty());
    }
}

#[test]
fn the_frame_iterator_of_an_empty_payload_yields_one_error_then_ends() {
    for pgn in [SINGLE_FRAME_PGN, FAST_PACKET_PGN] {
        let mut frames = FastPacketBuilder::new(pgn, 42, None, &[]).build();
        assert!(matches!(
            frames.next(),
            Some(Err(CanIdBuildError::EmptyPayload))
        ));
        assert!(frames.next().is_none());
        assert!(frames.next().is_none());
    }
}
