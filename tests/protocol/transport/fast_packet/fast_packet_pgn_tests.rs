//! Ensure Fast Packet PGNs perform a full round-trip correctly.
use korri_n2k::core::PgnBytes;
use korri_n2k::infra::codec::traits::PgnData;
use korri_n2k::protocol::transport::fast_packet::{
    assembler::{FastPacketAssembler, ProcessResult},
    builder::FastPacketBuilder,
};
use korri_n2k::protocol::{
    lookups::CertificationLevel,
    messages::{Pgn126996, Pgn126998, Pgn129040},
};

#[test]
fn test_pgn_129040_fast_packet_roundtrip() {
    // Serialize → segment → reassemble → deserialize and compare to original values.
    let mut ais = Pgn129040::new();
    ais.user_id = 123_456_789;
    ais.latitude = 48.8566;
    ais.longitude = 2.3522;

    let mut buffer = [0u8; 64];
    let len = ais.to_payload(&mut buffer).expect("serialize PGN 129040");
    assert!(
        len > 8,
        "PGN 129040 should generate a Fast Packet; current length: {len}"
    );

    let builder = FastPacketBuilder::new(129040, 42, None, &buffer[..len]);
    let mut frames = builder.build();

    let mut assembler = FastPacketAssembler::new();
    let mut complete = None;
    let mut frame_count = 0;

    while let Some(frame_result) = frames.next() {
        let frame = frame_result.expect("frame build");
        frame_count += 1;

        if let ProcessResult::MessageComplete(msg) = assembler.process_frame(42, &frame.data) {
            complete = Some(msg);
            break;
        }
    }

    let message = complete.expect("message complet");
    assert_eq!(message.len, len);
    assert_eq!(&message.payload[..len], &buffer[..len]);

    let decoded =
        Pgn129040::from_payload(&message.payload[..message.len]).expect("decode reassembled PGN");

    assert_eq!(ais.user_id, decoded.user_id);
    assert!((ais.latitude - decoded.latitude).abs() < 1e-6);
    assert!((ais.longitude - decoded.longitude).abs() < 1e-6);
    assert!(
        frame_count >= 2,
        "A Fast Packet must generate multiple frames"
    );
}

#[test]
fn test_pgn_126996_fast_packet_roundtrip() {
    // PGN 126996 carries several fixed ASCII strings (32 bytes each).
    // Verify serialization preserves size, padding, and metadata ordering.
    let mut product = Pgn126996::new();
    product.nmea2000_version = 2.005; // version 02.005
    product.product_code = 0x42AF;
    product.certification_level = CertificationLevel::LevelB;
    product.load_equivalency = 12;

    fn write_ascii<const N: usize>(dest: &mut [u8; N], text: &[u8]) {
        let len = text.len().min(N);
        dest[..len].copy_from_slice(&text[..len]);
    }

    write_ascii(&mut product.model_id, b"KORRI-N2K CORE");
    write_ascii(&mut product.software_version_code, b"v0.1.0-alpha+20251009");
    write_ascii(&mut product.model_version, b"rev-A");
    write_ascii(&mut product.model_serial_code, b"SN-123456789ABCDEF");

    let mut buffer = [0u8; 256];
    let len = product
        .to_payload(&mut buffer)
        .expect("serialize PGN 126996");

    assert_eq!(len, 134, "PGN 126996 must occupy 134 bytes");
    assert!(
        len > 8,
        "PGN 126996 should produce a Fast Packet; current length: {len}"
    );

    let builder = FastPacketBuilder::new(126996, 35, None, &buffer[..len]);
    let mut frames = builder.build();
    let mut assembler = FastPacketAssembler::new();
    let mut complete = None;
    let mut frame_count = 0;

    while let Some(frame_result) = frames.next() {
        let frame = frame_result.expect("frame build");
        frame_count += 1;

        if let ProcessResult::MessageComplete(msg) = assembler.process_frame(35, &frame.data) {
            complete = Some(msg);
            break;
        }
    }

    assert!(
        frame_count >= 2,
        "Fast Packet 126996 should generate multiple frames (observed: {frame_count})"
    );

    let message = complete.expect("message complet");
    assert_eq!(message.len, len);
    assert_eq!(&message.payload[..len], &buffer[..len]);

    let decoded = Pgn126996::from_payload(&message.payload[..message.len])
        .expect("decode reassembled PGN 126996");

    assert!(
        (decoded.nmea2000_version - product.nmea2000_version).abs() < 1e-6,
        "NMEA 2000 version must be preserved"
    );
    assert_eq!(decoded.product_code, product.product_code);
    assert_eq!(decoded.certification_level, product.certification_level);
    assert_eq!(decoded.load_equivalency, product.load_equivalency);
    assert_eq!(decoded.model_id, product.model_id);
    assert_eq!(decoded.software_version_code, product.software_version_code);
    assert_eq!(decoded.model_version, product.model_version);
    assert_eq!(decoded.model_serial_code, product.model_serial_code);
}

#[test]
fn test_pgn_126998_fast_packet_roundtrip() {
    fn set_lau(bytes: &mut PgnBytes, ascii: &[u8]) {
        bytes.clear();
        let max_len = bytes.data.len().saturating_sub(1);
        let copy_len = ascii.len().min(max_len);
        bytes.len = copy_len + 1;
        bytes.data[0] = 1; // ASCII encoding
        if copy_len > 0 {
            bytes.data[1..1 + copy_len].copy_from_slice(&ascii[..copy_len]);
        }
    }

    let mut config = Pgn126998::new();
    let mut desc1 = PgnBytes::new();
    let mut desc2 = PgnBytes::new();
    let mut manufacturer = PgnBytes::new();

    set_lau(&mut desc1, b"Korri Sensor Suite - Starboard installation");
    set_lau(&mut desc2, b"Firmware configured via korri-diag 1.2.3");
    set_lau(
        &mut manufacturer,
        b"Korri Marine Systems - Support +33 1 23 45 67 89",
    );

    config.installation_description1 = desc1;
    config.installation_description2 = desc2;
    config.manufacturer_information = manufacturer;

    let mut buffer = [0u8; 256];
    let len = config
        .to_payload(&mut buffer)
        .expect("serialize PGN 126998");
    assert!(len > 8, "PGN 126998 must be encoded as a Fast Packet");

    let builder = FastPacketBuilder::new(126998, 77, None, &buffer[..len]);
    let mut frames = builder.build();
    let mut assembler = FastPacketAssembler::new();
    let mut complete = None;

    while let Some(frame_result) = frames.next() {
        let frame = frame_result.expect("frame build");
        if let ProcessResult::MessageComplete(msg) = assembler.process_frame(77, &frame.data) {
            complete = Some(msg);
            break;
        }
    }

    let message = complete.expect("complete message 126998");
    assert_eq!(message.len, len);
    assert_eq!(&message.payload[..len], &buffer[..len]);

    let decoded = Pgn126998::from_payload(&message.payload[..message.len])
        .expect("decode reassembled PGN 126998");

    assert_eq!(
        decoded.installation_description1,
        config.installation_description1
    );
    assert_eq!(
        decoded.installation_description2,
        config.installation_description2
    );
    assert_eq!(
        decoded.manufacturer_information,
        config.manufacturer_information
    );
}
