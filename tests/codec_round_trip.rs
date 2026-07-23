//! Fidelity invariants of the codec.
//!
//! Decoding then re-encoding a message must land on the same value. Two defects
//! broke this across most PGNs: the encoder truncated the scaled integer instead
//! of rounding it (23.767 came back as 23.766), and a 32-bit field scaled by 1e-7
//! was held in an f32, which has too little mantissa (0.75 m of position drift).

use core::fmt::Debug;
use korri_n2k::core::MAX_PGN_BYTES;
use korri_n2k::infra::codec::traits::PgnData;
use korri_n2k::protocol::messages::*;
use korri_n2k::protocol::transport::can_id::CanId;

/// xorshift64, so a failure is reproducible without pulling in a dependency.
struct Rng(u64);

impl Rng {
    fn next_byte(&mut self) -> u8 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 33) as u8
    }
}

/// decode -> encode -> decode must be a fixed point.
fn idempotent<T: PgnData + PartialEq + Debug>(label: &str, failures: &mut Vec<String>) {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut payload = [0u8; MAX_PGN_BYTES];

    for round in 0..200 {
        for byte in payload.iter_mut() {
            *byte = rng.next_byte();
        }
        let Ok(first) = T::from_payload(&payload) else {
            continue;
        };

        let mut out = [0u8; MAX_PGN_BYTES];
        let Ok(len) = first.to_payload(&mut out) else {
            failures.push(format!("{label}: encoding its own decoded value failed"));
            return;
        };
        let Ok(second) = T::from_payload(&out[..len]) else {
            failures.push(format!("{label}: re-decoding its own output failed"));
            return;
        };
        if first != second {
            failures.push(format!("{label}: round {round} is not a fixed point"));
            return;
        }
    }
}

#[test]
fn decode_encode_decode_is_a_fixed_point() {
    let mut failures = Vec::new();
    idempotent::<Pgn126985>("Pgn126985", &mut failures);
    idempotent::<Pgn126992>("Pgn126992", &mut failures);
    idempotent::<Pgn126993>("Pgn126993", &mut failures);
    idempotent::<Pgn126996>("Pgn126996", &mut failures);
    idempotent::<Pgn126998>("Pgn126998", &mut failures);
    idempotent::<Pgn127237>("Pgn127237", &mut failures);
    idempotent::<Pgn127245>("Pgn127245", &mut failures);
    idempotent::<Pgn127250>("Pgn127250", &mut failures);
    idempotent::<Pgn127251>("Pgn127251", &mut failures);
    idempotent::<Pgn127257>("Pgn127257", &mut failures);
    idempotent::<Pgn127488>("Pgn127488", &mut failures);
    idempotent::<Pgn127489>("Pgn127489", &mut failures);
    idempotent::<Pgn127497>("Pgn127497", &mut failures);
    idempotent::<Pgn127503>("Pgn127503", &mut failures);
    idempotent::<Pgn127505>("Pgn127505", &mut failures);
    idempotent::<Pgn127508>("Pgn127508", &mut failures);
    idempotent::<Pgn127750>("Pgn127750", &mut failures);
    idempotent::<Pgn128001>("Pgn128001", &mut failures);
    idempotent::<Pgn128259>("Pgn128259", &mut failures);
    idempotent::<Pgn128267>("Pgn128267", &mut failures);
    idempotent::<Pgn128275>("Pgn128275", &mut failures);
    idempotent::<Pgn129025>("Pgn129025", &mut failures);
    idempotent::<Pgn129026>("Pgn129026", &mut failures);
    idempotent::<Pgn129029>("Pgn129029", &mut failures);
    idempotent::<Pgn129038>("Pgn129038", &mut failures);
    idempotent::<Pgn129039>("Pgn129039", &mut failures);
    idempotent::<Pgn129040>("Pgn129040", &mut failures);
    idempotent::<Pgn129044>("Pgn129044", &mut failures);
    idempotent::<Pgn129283>("Pgn129283", &mut failures);
    idempotent::<Pgn129284>("Pgn129284", &mut failures);
    idempotent::<Pgn129540>("Pgn129540", &mut failures);
    idempotent::<Pgn129794>("Pgn129794", &mut failures);
    idempotent::<Pgn129809>("Pgn129809", &mut failures);
    idempotent::<Pgn129810>("Pgn129810", &mut failures);
    idempotent::<Pgn130306>("Pgn130306", &mut failures);
    idempotent::<Pgn130310>("Pgn130310", &mut failures);
    idempotent::<Pgn130311>("Pgn130311", &mut failures);
    idempotent::<Pgn130821>("Pgn130821", &mut failures);
    idempotent::<Pgn59904>("Pgn59904", &mut failures);
    idempotent::<Pgn60160>("Pgn60160", &mut failures);
    idempotent::<Pgn60416>("Pgn60416", &mut failures);
    idempotent::<Pgn60928>("Pgn60928", &mut failures);
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn position_survives_the_full_latitude_range() {
    // Latitude is 32 bits at 1e-7 deg. Every value must come back bit-exact;
    // an f32 field lost up to 67 raw units, i.e. 0.75 m on the ground.
    let mut raw: i64 = -900_000_000;
    while raw <= 900_000_000 {
        let lat = raw as i32;
        let mut payload = [0u8; 8];
        payload[0..4].copy_from_slice(&lat.to_le_bytes());
        payload[4..8].copy_from_slice(&0i32.to_le_bytes());

        let pgn = Pgn129025::from_payload(&payload).expect("decode 129025");
        let mut out = [0u8; 8];
        pgn.to_payload(&mut out).expect("encode 129025");

        let back = i32::from_le_bytes(out[0..4].try_into().unwrap());
        assert_eq!(back, lat, "latitude {lat} came back as {back}");
        raw += 997_331; // coprime stride, sweeps the whole range
    }
}

#[test]
fn scaled_values_round_rather_than_truncate() {
    // 23.767 at a resolution of 0.001 divides to 23766.999…; truncating shifts
    // every scaled field one LSB towards zero.
    let mut pgn = Pgn130306::new();
    pgn.wind_speed = 185.24;
    pgn.wind_angle = 2.8806;

    let mut out = [0u8; 8];
    pgn.to_payload(&mut out).expect("encode 130306");
    let back = Pgn130306::from_payload(&out).expect("decode 130306");

    assert!(
        (back.wind_speed - 185.24).abs() < 0.01,
        "{}",
        back.wind_speed
    );
    assert!(
        (back.wind_angle - 2.8806).abs() < 0.0001,
        "{}",
        back.wind_angle
    );
}

#[test]
fn can_id_round_trips_over_the_pgn_space() {
    let mut samples = 0usize;
    for pgn in (0u32..=0x3_FFFF).step_by(7) {
        let is_pdu1 = ((pgn >> 8) & 0xFF) < 240;
        for &src in &[0u8, 1, 128, 251, 254, 255] {
            for &priority in &[0u8, 3, 7] {
                let mut builder = CanId::builder(pgn, src).priority(priority);
                if is_pdu1 {
                    builder = builder.destination(255);
                }
                let Ok(id) = builder.build() else { continue };
                samples += 1;
                assert_eq!(id.pgn(), pgn);
                assert_eq!(id.source_address(), src);
                assert_eq!(id.priority(), priority);
            }
        }
    }
    assert!(samples > 40_000, "only {samples} identifiers exercised");
}
