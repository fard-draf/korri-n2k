//! No CAN frame, however malformed, may panic the decoder. A device on a live
//! bus receives truncated, oversized and corrupted payloads; every one of them
//! must come back as an `Err`, never as an abort.
//!
//! Covers every PGN of the default manifest against truncation, over-length
//! input and a deterministic byte sweep.

use korri_n2k::core::MAX_PGN_BYTES;
use korri_n2k::infra::codec::bits::{BitReader, BitWriter};
use korri_n2k::infra::codec::traits::PgnData;
use korri_n2k::protocol::messages::*;
use korri_n2k::protocol::transport::fast_packet::assembler::{FastPacketAssembler, ProcessResult};
use korri_n2k::protocol::transport::fast_packet::MAX_FAST_PACKET_PAYLOAD;

/// xorshift64*, so a failure is reproducible without a dependency.
struct Rng(u64);

impl Rng {
    fn next_byte(&mut self) -> u8 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 33) as u8
    }
}

/// Every length from empty to just over the maximum payload, filled with a few
/// fixed patterns and then with pseudo-random bytes. The decode may fail; it
/// may not panic.
fn sweep<T: PgnData>() {
    let mut buffer = [0u8; MAX_PGN_BYTES + 8];

    for pattern in [0x00u8, 0xFF, 0xAA, 0x55] {
        buffer.fill(pattern);
        for len in 0..buffer.len() {
            let _ = T::from_payload(&buffer[..len]);
        }
    }

    let mut rng = Rng(0x2545F4914F6CDD1D);
    for _ in 0..64 {
        for byte in buffer.iter_mut() {
            *byte = rng.next_byte();
        }
        for len in 0..buffer.len() {
            let _ = T::from_payload(&buffer[..len]);
        }
    }
}

#[test]
fn no_payload_panics_the_decoder() {
    sweep::<Pgn126985>();
    sweep::<Pgn126992>();
    sweep::<Pgn126993>();
    sweep::<Pgn126996>();
    sweep::<Pgn126998>();
    sweep::<Pgn127237>();
    sweep::<Pgn127245>();
    sweep::<Pgn127250>();
    sweep::<Pgn127251>();
    sweep::<Pgn127257>();
    sweep::<Pgn127488>();
    sweep::<Pgn127489>();
    sweep::<Pgn127497>();
    sweep::<Pgn127503>();
    sweep::<Pgn127505>();
    sweep::<Pgn127508>();
    sweep::<Pgn127750>();
    sweep::<Pgn128001>();
    sweep::<Pgn128259>();
    sweep::<Pgn128267>();
    sweep::<Pgn128275>();
    sweep::<Pgn129025>();
    sweep::<Pgn129026>();
    sweep::<Pgn129029>();
    sweep::<Pgn129038>();
    sweep::<Pgn129039>();
    sweep::<Pgn129040>();
    sweep::<Pgn129044>();
    sweep::<Pgn129283>();
    sweep::<Pgn129284>();
    sweep::<Pgn129540>();
    sweep::<Pgn129794>();
    sweep::<Pgn129809>();
    sweep::<Pgn129810>();
    sweep::<Pgn130306>();
    sweep::<Pgn130310>();
    sweep::<Pgn130311>();
    sweep::<Pgn130821>();
    sweep::<Pgn59904>();
    sweep::<Pgn60160>();
    sweep::<Pgn60416>();
    sweep::<Pgn60928>();
}

/// The remaining PGNs of the full manifest: polymorphic dispatch, strings,
/// repeating groups and sub-byte binary fields all get the same treatment.
#[cfg(feature = "full-pgns")]
#[test]
fn no_payload_panics_any_supported_pgn() {
    sweep::<Pgn126464>();
    sweep::<Pgn126720>();
    sweep::<Pgn126976>();
    sweep::<Pgn126983>();
    sweep::<Pgn126984>();
    sweep::<Pgn126986>();
    sweep::<Pgn126987>();
    sweep::<Pgn126988>();
    sweep::<Pgn127252>();
    sweep::<Pgn127258>();
    sweep::<Pgn127490>();
    sweep::<Pgn127491>();
    sweep::<Pgn127493>();
    sweep::<Pgn127494>();
    sweep::<Pgn127495>();
    sweep::<Pgn127496>();
    sweep::<Pgn127498>();
    sweep::<Pgn127500>();
    sweep::<Pgn127501>();
    sweep::<Pgn127502>();
    sweep::<Pgn127506>();
    sweep::<Pgn127507>();
    sweep::<Pgn127509>();
    sweep::<Pgn127510>();
    sweep::<Pgn127511>();
    sweep::<Pgn127512>();
    sweep::<Pgn127513>();
    sweep::<Pgn127514>();
    sweep::<Pgn127744>();
    sweep::<Pgn127745>();
    sweep::<Pgn127746>();
    sweep::<Pgn127747>();
    sweep::<Pgn127748>();
    sweep::<Pgn127749>();
    sweep::<Pgn127751>();
    sweep::<Pgn128000>();
    sweep::<Pgn128002>();
    sweep::<Pgn128003>();
    sweep::<Pgn128006>();
    sweep::<Pgn128007>();
    sweep::<Pgn128008>();
    sweep::<Pgn128520>();
    sweep::<Pgn128538>();
    sweep::<Pgn128768>();
    sweep::<Pgn128769>();
    sweep::<Pgn128776>();
    sweep::<Pgn128777>();
    sweep::<Pgn128778>();
    sweep::<Pgn128780>();
    sweep::<Pgn129027>();
    sweep::<Pgn129028>();
    sweep::<Pgn129033>();
    sweep::<Pgn129041>();
    sweep::<Pgn129045>();
    sweep::<Pgn129285>();
    sweep::<Pgn129291>();
    sweep::<Pgn129301>();
    sweep::<Pgn129302>();
    sweep::<Pgn129538>();
    sweep::<Pgn129539>();
    sweep::<Pgn129541>();
    sweep::<Pgn129542>();
    sweep::<Pgn129545>();
    sweep::<Pgn129546>();
    sweep::<Pgn129547>();
    sweep::<Pgn129549>();
    sweep::<Pgn129550>();
    sweep::<Pgn129551>();
    sweep::<Pgn129556>();
    sweep::<Pgn129793>();
    sweep::<Pgn129796>();
    sweep::<Pgn129798>();
    sweep::<Pgn129799>();
    sweep::<Pgn129800>();
    sweep::<Pgn129801>();
    sweep::<Pgn129802>();
    sweep::<Pgn129803>();
    sweep::<Pgn129804>();
    sweep::<Pgn129805>();
    sweep::<Pgn129806>();
    sweep::<Pgn129807>();
    sweep::<Pgn129813>();
    sweep::<Pgn130052>();
    sweep::<Pgn130053>();
    sweep::<Pgn130054>();
    sweep::<Pgn130060>();
    sweep::<Pgn130061>();
    sweep::<Pgn130064>();
    sweep::<Pgn130065>();
    sweep::<Pgn130066>();
    sweep::<Pgn130070>();
    sweep::<Pgn130073>();
    sweep::<Pgn130312>();
    sweep::<Pgn130313>();
    sweep::<Pgn130314>();
    sweep::<Pgn130315>();
    sweep::<Pgn130316>();
    sweep::<Pgn130320>();
    sweep::<Pgn130321>();
    sweep::<Pgn130322>();
    sweep::<Pgn130323>();
    sweep::<Pgn130324>();
    sweep::<Pgn130329>();
    sweep::<Pgn130330>();
    sweep::<Pgn130560>();
    sweep::<Pgn130561>();
    sweep::<Pgn130562>();
    sweep::<Pgn130563>();
    sweep::<Pgn130564>();
    sweep::<Pgn130565>();
    sweep::<Pgn130566>();
    sweep::<Pgn130567>();
    sweep::<Pgn130568>();
    sweep::<Pgn130569>();
    sweep::<Pgn130570>();
    sweep::<Pgn130571>();
    sweep::<Pgn130572>();
    sweep::<Pgn130574>();
    sweep::<Pgn130575>();
    sweep::<Pgn130576>();
    sweep::<Pgn130577>();
    sweep::<Pgn130578>();
    sweep::<Pgn130579>();
    sweep::<Pgn130580>();
    sweep::<Pgn130582>();
    sweep::<Pgn130583>();
    sweep::<Pgn130585>();
    sweep::<Pgn130586>();
    sweep::<Pgn130817>();
    sweep::<Pgn130819>();
    sweep::<Pgn130825>();
    sweep::<Pgn130826>();
    sweep::<Pgn130827>();
    sweep::<Pgn130828>();
    sweep::<Pgn130829>();
    sweep::<Pgn130830>();
    sweep::<Pgn130831>();
    sweep::<Pgn130832>();
    sweep::<Pgn130833>();
    sweep::<Pgn130834>();
    sweep::<Pgn130835>();
    sweep::<Pgn130836>();
    sweep::<Pgn130837>();
    sweep::<Pgn130838>();
    sweep::<Pgn130839>();
    sweep::<Pgn130840>();
    sweep::<Pgn130841>();
    sweep::<Pgn130842>();
    sweep::<Pgn130843>();
    sweep::<Pgn130844>();
    sweep::<Pgn130847>();
    sweep::<Pgn130848>();
    sweep::<Pgn130849>();
    sweep::<Pgn130850>();
    sweep::<Pgn130851>();
    sweep::<Pgn130856>();
    sweep::<Pgn130860>();
    sweep::<Pgn130880>();
    sweep::<Pgn130881>();
    sweep::<Pgn130900>();
    sweep::<Pgn130910>();
    sweep::<Pgn130911>();
    sweep::<Pgn130912>();
    sweep::<Pgn130913>();
    sweep::<Pgn130918>();
    sweep::<Pgn130921>();
    sweep::<Pgn130939>();
    sweep::<Pgn130944>();
    sweep::<Pgn130945>();
    sweep::<Pgn130946>();
    sweep::<Pgn130947>();
    sweep::<Pgn130951>();
    sweep::<Pgn131008>();
    sweep::<Pgn131011>();
    sweep::<Pgn131012>();
    sweep::<Pgn61440>();
    sweep::<Pgn65001>();
    sweep::<Pgn65002>();
    sweep::<Pgn65003>();
    sweep::<Pgn65004>();
    sweep::<Pgn65005>();
    sweep::<Pgn65006>();
    sweep::<Pgn65007>();
    sweep::<Pgn65008>();
    sweep::<Pgn65009>();
    sweep::<Pgn65010>();
    sweep::<Pgn65011>();
    sweep::<Pgn65012>();
    sweep::<Pgn65013>();
    sweep::<Pgn65014>();
    sweep::<Pgn65015>();
    sweep::<Pgn65016>();
    sweep::<Pgn65017>();
    sweep::<Pgn65018>();
    sweep::<Pgn65019>();
    sweep::<Pgn65020>();
    sweep::<Pgn65021>();
    sweep::<Pgn65022>();
    sweep::<Pgn65023>();
    sweep::<Pgn65024>();
    sweep::<Pgn65025>();
    sweep::<Pgn65026>();
    sweep::<Pgn65027>();
    sweep::<Pgn65028>();
    sweep::<Pgn65029>();
    sweep::<Pgn65030>();
    sweep::<Pgn65240>();
    sweep::<Pgn65281>();
    sweep::<Pgn65282>();
    sweep::<Pgn65283>();
    sweep::<Pgn65284>();
    sweep::<Pgn65285>();
    sweep::<Pgn65286>();
    sweep::<Pgn65287>();
    sweep::<Pgn65290>();
    sweep::<Pgn65291>();
    sweep::<Pgn65292>();
    sweep::<Pgn65293>();
    sweep::<Pgn65294>();
    sweep::<Pgn65295>();
    sweep::<Pgn65296>();
    sweep::<Pgn65297>();
    sweep::<Pgn65298>();
    sweep::<Pgn65299>();
    sweep::<Pgn65300>();
    sweep::<Pgn65301>();
    sweep::<Pgn65302>();
    sweep::<Pgn65303>();
    sweep::<Pgn65304>();
    sweep::<Pgn65305>();
    sweep::<Pgn65306>();
    sweep::<Pgn65308>();
    sweep::<Pgn65309>();
    sweep::<Pgn65310>();
    sweep::<Pgn65311>();
    sweep::<Pgn65312>();
    sweep::<Pgn65313>();
    sweep::<Pgn65314>();
    sweep::<Pgn65315>();
    sweep::<Pgn65316>();
    sweep::<Pgn65317>();
    sweep::<Pgn65323>();
    sweep::<Pgn65324>();
    sweep::<Pgn65325>();
    sweep::<Pgn65329>();
    sweep::<Pgn65330>();
    sweep::<Pgn65332>();
    sweep::<Pgn65340>();
    sweep::<Pgn65341>();
    sweep::<Pgn65344>();
    sweep::<Pgn65345>();
    sweep::<Pgn65346>();
    sweep::<Pgn65348>();
    sweep::<Pgn65349>();
    sweep::<Pgn65350>();
    sweep::<Pgn65359>();
    sweep::<Pgn65360>();
    sweep::<Pgn65361>();
    sweep::<Pgn65371>();
    sweep::<Pgn65374>();
    sweep::<Pgn65379>();
    sweep::<Pgn65403>();
    sweep::<Pgn65408>();
    sweep::<Pgn65409>();
    sweep::<Pgn65410>();
    sweep::<Pgn65420>();
    sweep::<Pgn65424>();
    sweep::<Pgn65440>();
    sweep::<Pgn65441>();
    sweep::<Pgn65472>();
    sweep::<Pgn65480>();
}

//==============================================================================
// Bit cursor bounds
//==============================================================================

/// `BitReader::seek` accepts any position, so the cursor may sit past the end of
/// the buffer. Computing the remaining room then underflowed and aborted: a
/// truncated frame was enough to bring the decoder down.
#[test]
fn cursor_past_the_end_reports_an_error() {
    let buffer = [0u8; 8];

    let mut reader = BitReader::new(&buffer);
    reader.seek(1000);
    assert!(reader.read_u64(8).is_err());

    let mut reader = BitReader::new(&buffer);
    reader.seek(1000);
    assert!(reader.advance(8).is_err());

    let mut reader = BitReader::new(&buffer);
    reader.seek(usize::MAX - 7);
    assert!(reader.read_slice(4).is_err());
}

#[test]
fn every_width_round_trips_at_every_offset() {
    for offset in 0..8u8 {
        for bits in 1..=32u8 {
            let value = (1u64 << bits) - 1;

            let mut buffer = [0u8; 16];
            let mut writer = BitWriter::new(&mut buffer);
            if offset > 0 {
                writer.advance(offset).expect("advance writer");
            }
            writer.write_u64(value, bits).expect("write");

            let mut reader = BitReader::new(&buffer);
            if offset > 0 {
                reader.advance(offset).expect("advance reader");
            }
            assert_eq!(
                reader.read_u64(bits).expect("read"),
                value,
                "{offset}+{bits}"
            );
        }
    }
}

#[test]
fn sixty_four_bit_values_survive() {
    let mut buffer = [0u8; 16];
    let mut writer = BitWriter::new(&mut buffer);
    writer.write_u64(u64::MAX, 64).expect("write");
    let mut reader = BitReader::new(&buffer);
    assert_eq!(reader.read_u64(64).expect("read"), u64::MAX);
}

//==============================================================================
// Fast Packet assembler
//==============================================================================

/// Random frames from several sources, with a clock that jumps and wraps.
#[test]
fn assembler_survives_an_adversarial_stream() {
    let mut assembler = FastPacketAssembler::new();
    let mut rng = Rng(0xDEAD_BEEF_CAFE_F00D);
    let mut now: u32 = 0;

    for i in 0..200_000u32 {
        let mut data = [0u8; 8];
        for byte in data.iter_mut() {
            *byte = rng.next_byte();
        }
        let source = rng.next_byte() % 6;
        let pgn = 126_000 + (rng.next_byte() % 3) as u32;

        now = now.wrapping_add((rng.next_byte() % 200) as u32);
        if i % 10_000 == 0 {
            now = now.wrapping_add(u32::MAX / 3); // force expiry and wrap-around
        }

        if let ProcessResult::MessageComplete(message) =
            assembler.process_frame(now, pgn, source, &data)
        {
            assert!(message.len <= message.payload.len());
        }
    }
}

/// Every length a Fast Packet can carry must reassemble byte for byte.
#[test]
fn assembler_reassembles_every_valid_length() {
    for expected in 8..=MAX_FAST_PACKET_PAYLOAD {
        let mut assembler = FastPacketAssembler::new();
        let source: Vec<u8> = (0..expected).map(|i| (i % 251) as u8).collect();

        let mut frame = [0u8; 8];
        frame[1] = expected as u8;
        let first = 6.min(expected);
        frame[2..2 + first].copy_from_slice(&source[..first]);
        let mut result = assembler.process_frame(0, 126_996, 7, &frame);

        let mut sent = first;
        let mut index = 1u8;
        while sent < expected {
            let take = 7.min(expected - sent);
            let mut frame = [0xFFu8; 8];
            frame[0] = index;
            frame[1..1 + take].copy_from_slice(&source[sent..sent + take]);
            result = assembler.process_frame(0, 126_996, 7, &frame);
            sent += take;
            index += 1;
        }

        match result {
            ProcessResult::MessageComplete(message) => {
                assert_eq!(message.len, expected);
                assert_eq!(&message.payload[..expected], &source[..]);
            }
            other => panic!("length {expected}: expected completion, got {other:?}"),
        }
    }
}
