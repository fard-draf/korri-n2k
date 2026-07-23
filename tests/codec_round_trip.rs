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

/// Two invariants per PGN.
///
/// The encoding must be a fixed point: re-encoding a value decoded from our own
/// output has to produce the same bytes. Comparing bytes rather than values keeps
/// this meaningful for FLOAT fields, whose CANboat sentinel is NaN — and NaN is
/// never equal to itself.
///
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

        let mut once = [0u8; MAX_PGN_BYTES];
        let Ok(len) = first.to_payload(&mut once) else {
            failures.push(format!("{label}: encoding its own decoded value failed"));
            return;
        };
        let Ok(second) = T::from_payload(&once[..len]) else {
            failures.push(format!("{label}: re-decoding its own output failed"));
            return;
        };

        let mut twice = [0u8; MAX_PGN_BYTES];
        let Ok(len_again) = second.to_payload(&mut twice) else {
            failures.push(format!("{label}: second encoding failed"));
            return;
        };
        if once[..len] != twice[..len_again] {
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

/// The rest of the full manifest. Restricting the fixed-point check to the
/// default PGNs once let a 32-bit scaled `Duration` keep an f32 field while
/// the engine had moved to f64 — the mismatch only existed out here.
#[cfg(feature = "full-pgns")]
#[test]
fn decode_encode_decode_is_a_fixed_point_for_every_supported_pgn() {
    let mut failures = Vec::new();
    idempotent::<Pgn126464>("Pgn126464", &mut failures);
    idempotent::<Pgn126720>("Pgn126720", &mut failures);
    idempotent::<Pgn126976>("Pgn126976", &mut failures);
    idempotent::<Pgn126983>("Pgn126983", &mut failures);
    idempotent::<Pgn126984>("Pgn126984", &mut failures);
    idempotent::<Pgn126986>("Pgn126986", &mut failures);
    idempotent::<Pgn126987>("Pgn126987", &mut failures);
    idempotent::<Pgn126988>("Pgn126988", &mut failures);
    idempotent::<Pgn127252>("Pgn127252", &mut failures);
    idempotent::<Pgn127258>("Pgn127258", &mut failures);
    idempotent::<Pgn127490>("Pgn127490", &mut failures);
    idempotent::<Pgn127491>("Pgn127491", &mut failures);
    idempotent::<Pgn127493>("Pgn127493", &mut failures);
    idempotent::<Pgn127494>("Pgn127494", &mut failures);
    idempotent::<Pgn127495>("Pgn127495", &mut failures);
    idempotent::<Pgn127496>("Pgn127496", &mut failures);
    idempotent::<Pgn127498>("Pgn127498", &mut failures);
    idempotent::<Pgn127500>("Pgn127500", &mut failures);
    idempotent::<Pgn127501>("Pgn127501", &mut failures);
    idempotent::<Pgn127502>("Pgn127502", &mut failures);
    idempotent::<Pgn127506>("Pgn127506", &mut failures);
    idempotent::<Pgn127507>("Pgn127507", &mut failures);
    idempotent::<Pgn127509>("Pgn127509", &mut failures);
    idempotent::<Pgn127510>("Pgn127510", &mut failures);
    idempotent::<Pgn127511>("Pgn127511", &mut failures);
    idempotent::<Pgn127512>("Pgn127512", &mut failures);
    idempotent::<Pgn127513>("Pgn127513", &mut failures);
    idempotent::<Pgn127514>("Pgn127514", &mut failures);
    idempotent::<Pgn127744>("Pgn127744", &mut failures);
    idempotent::<Pgn127745>("Pgn127745", &mut failures);
    idempotent::<Pgn127746>("Pgn127746", &mut failures);
    idempotent::<Pgn127747>("Pgn127747", &mut failures);
    idempotent::<Pgn127748>("Pgn127748", &mut failures);
    idempotent::<Pgn127749>("Pgn127749", &mut failures);
    idempotent::<Pgn127751>("Pgn127751", &mut failures);
    idempotent::<Pgn128000>("Pgn128000", &mut failures);
    idempotent::<Pgn128002>("Pgn128002", &mut failures);
    idempotent::<Pgn128003>("Pgn128003", &mut failures);
    idempotent::<Pgn128006>("Pgn128006", &mut failures);
    idempotent::<Pgn128007>("Pgn128007", &mut failures);
    idempotent::<Pgn128008>("Pgn128008", &mut failures);
    idempotent::<Pgn128520>("Pgn128520", &mut failures);
    idempotent::<Pgn128538>("Pgn128538", &mut failures);
    idempotent::<Pgn128768>("Pgn128768", &mut failures);
    idempotent::<Pgn128769>("Pgn128769", &mut failures);
    idempotent::<Pgn128776>("Pgn128776", &mut failures);
    idempotent::<Pgn128777>("Pgn128777", &mut failures);
    idempotent::<Pgn128778>("Pgn128778", &mut failures);
    idempotent::<Pgn128780>("Pgn128780", &mut failures);
    idempotent::<Pgn129027>("Pgn129027", &mut failures);
    idempotent::<Pgn129028>("Pgn129028", &mut failures);
    idempotent::<Pgn129033>("Pgn129033", &mut failures);
    idempotent::<Pgn129041>("Pgn129041", &mut failures);
    idempotent::<Pgn129045>("Pgn129045", &mut failures);
    idempotent::<Pgn129285>("Pgn129285", &mut failures);
    idempotent::<Pgn129291>("Pgn129291", &mut failures);
    idempotent::<Pgn129301>("Pgn129301", &mut failures);
    idempotent::<Pgn129302>("Pgn129302", &mut failures);
    idempotent::<Pgn129538>("Pgn129538", &mut failures);
    idempotent::<Pgn129539>("Pgn129539", &mut failures);
    idempotent::<Pgn129541>("Pgn129541", &mut failures);
    idempotent::<Pgn129542>("Pgn129542", &mut failures);
    idempotent::<Pgn129545>("Pgn129545", &mut failures);
    idempotent::<Pgn129546>("Pgn129546", &mut failures);
    idempotent::<Pgn129547>("Pgn129547", &mut failures);
    idempotent::<Pgn129549>("Pgn129549", &mut failures);
    idempotent::<Pgn129550>("Pgn129550", &mut failures);
    idempotent::<Pgn129551>("Pgn129551", &mut failures);
    idempotent::<Pgn129556>("Pgn129556", &mut failures);
    idempotent::<Pgn129793>("Pgn129793", &mut failures);
    idempotent::<Pgn129796>("Pgn129796", &mut failures);
    idempotent::<Pgn129798>("Pgn129798", &mut failures);
    idempotent::<Pgn129799>("Pgn129799", &mut failures);
    idempotent::<Pgn129800>("Pgn129800", &mut failures);
    idempotent::<Pgn129801>("Pgn129801", &mut failures);
    idempotent::<Pgn129802>("Pgn129802", &mut failures);
    idempotent::<Pgn129803>("Pgn129803", &mut failures);
    idempotent::<Pgn129804>("Pgn129804", &mut failures);
    idempotent::<Pgn129805>("Pgn129805", &mut failures);
    idempotent::<Pgn129806>("Pgn129806", &mut failures);
    idempotent::<Pgn129807>("Pgn129807", &mut failures);
    idempotent::<Pgn129813>("Pgn129813", &mut failures);
    idempotent::<Pgn130052>("Pgn130052", &mut failures);
    idempotent::<Pgn130053>("Pgn130053", &mut failures);
    idempotent::<Pgn130054>("Pgn130054", &mut failures);
    idempotent::<Pgn130060>("Pgn130060", &mut failures);
    idempotent::<Pgn130061>("Pgn130061", &mut failures);
    idempotent::<Pgn130064>("Pgn130064", &mut failures);
    idempotent::<Pgn130065>("Pgn130065", &mut failures);
    idempotent::<Pgn130066>("Pgn130066", &mut failures);
    idempotent::<Pgn130070>("Pgn130070", &mut failures);
    idempotent::<Pgn130073>("Pgn130073", &mut failures);
    idempotent::<Pgn130312>("Pgn130312", &mut failures);
    idempotent::<Pgn130313>("Pgn130313", &mut failures);
    idempotent::<Pgn130314>("Pgn130314", &mut failures);
    idempotent::<Pgn130315>("Pgn130315", &mut failures);
    idempotent::<Pgn130316>("Pgn130316", &mut failures);
    idempotent::<Pgn130320>("Pgn130320", &mut failures);
    idempotent::<Pgn130321>("Pgn130321", &mut failures);
    idempotent::<Pgn130322>("Pgn130322", &mut failures);
    idempotent::<Pgn130323>("Pgn130323", &mut failures);
    idempotent::<Pgn130324>("Pgn130324", &mut failures);
    idempotent::<Pgn130329>("Pgn130329", &mut failures);
    idempotent::<Pgn130330>("Pgn130330", &mut failures);
    idempotent::<Pgn130560>("Pgn130560", &mut failures);
    idempotent::<Pgn130561>("Pgn130561", &mut failures);
    idempotent::<Pgn130562>("Pgn130562", &mut failures);
    idempotent::<Pgn130563>("Pgn130563", &mut failures);
    idempotent::<Pgn130564>("Pgn130564", &mut failures);
    idempotent::<Pgn130565>("Pgn130565", &mut failures);
    idempotent::<Pgn130566>("Pgn130566", &mut failures);
    idempotent::<Pgn130567>("Pgn130567", &mut failures);
    idempotent::<Pgn130568>("Pgn130568", &mut failures);
    idempotent::<Pgn130569>("Pgn130569", &mut failures);
    idempotent::<Pgn130570>("Pgn130570", &mut failures);
    idempotent::<Pgn130571>("Pgn130571", &mut failures);
    idempotent::<Pgn130572>("Pgn130572", &mut failures);
    idempotent::<Pgn130574>("Pgn130574", &mut failures);
    idempotent::<Pgn130575>("Pgn130575", &mut failures);
    idempotent::<Pgn130576>("Pgn130576", &mut failures);
    idempotent::<Pgn130577>("Pgn130577", &mut failures);
    idempotent::<Pgn130578>("Pgn130578", &mut failures);
    idempotent::<Pgn130579>("Pgn130579", &mut failures);
    idempotent::<Pgn130580>("Pgn130580", &mut failures);
    idempotent::<Pgn130582>("Pgn130582", &mut failures);
    idempotent::<Pgn130583>("Pgn130583", &mut failures);
    idempotent::<Pgn130585>("Pgn130585", &mut failures);
    idempotent::<Pgn130586>("Pgn130586", &mut failures);
    idempotent::<Pgn130817>("Pgn130817", &mut failures);
    idempotent::<Pgn130819>("Pgn130819", &mut failures);
    idempotent::<Pgn130825>("Pgn130825", &mut failures);
    idempotent::<Pgn130826>("Pgn130826", &mut failures);
    idempotent::<Pgn130827>("Pgn130827", &mut failures);
    idempotent::<Pgn130828>("Pgn130828", &mut failures);
    idempotent::<Pgn130829>("Pgn130829", &mut failures);
    idempotent::<Pgn130830>("Pgn130830", &mut failures);
    idempotent::<Pgn130831>("Pgn130831", &mut failures);
    idempotent::<Pgn130832>("Pgn130832", &mut failures);
    idempotent::<Pgn130833>("Pgn130833", &mut failures);
    idempotent::<Pgn130834>("Pgn130834", &mut failures);
    idempotent::<Pgn130835>("Pgn130835", &mut failures);
    idempotent::<Pgn130836>("Pgn130836", &mut failures);
    idempotent::<Pgn130837>("Pgn130837", &mut failures);
    idempotent::<Pgn130838>("Pgn130838", &mut failures);
    idempotent::<Pgn130839>("Pgn130839", &mut failures);
    idempotent::<Pgn130840>("Pgn130840", &mut failures);
    idempotent::<Pgn130841>("Pgn130841", &mut failures);
    idempotent::<Pgn130842>("Pgn130842", &mut failures);
    idempotent::<Pgn130843>("Pgn130843", &mut failures);
    idempotent::<Pgn130844>("Pgn130844", &mut failures);
    idempotent::<Pgn130847>("Pgn130847", &mut failures);
    idempotent::<Pgn130848>("Pgn130848", &mut failures);
    idempotent::<Pgn130849>("Pgn130849", &mut failures);
    idempotent::<Pgn130850>("Pgn130850", &mut failures);
    idempotent::<Pgn130851>("Pgn130851", &mut failures);
    idempotent::<Pgn130856>("Pgn130856", &mut failures);
    idempotent::<Pgn130860>("Pgn130860", &mut failures);
    idempotent::<Pgn130880>("Pgn130880", &mut failures);
    idempotent::<Pgn130881>("Pgn130881", &mut failures);
    idempotent::<Pgn130900>("Pgn130900", &mut failures);
    idempotent::<Pgn130910>("Pgn130910", &mut failures);
    idempotent::<Pgn130911>("Pgn130911", &mut failures);
    idempotent::<Pgn130912>("Pgn130912", &mut failures);
    idempotent::<Pgn130913>("Pgn130913", &mut failures);
    idempotent::<Pgn130918>("Pgn130918", &mut failures);
    idempotent::<Pgn130921>("Pgn130921", &mut failures);
    idempotent::<Pgn130939>("Pgn130939", &mut failures);
    idempotent::<Pgn130944>("Pgn130944", &mut failures);
    idempotent::<Pgn130945>("Pgn130945", &mut failures);
    idempotent::<Pgn130946>("Pgn130946", &mut failures);
    idempotent::<Pgn130947>("Pgn130947", &mut failures);
    idempotent::<Pgn130951>("Pgn130951", &mut failures);
    idempotent::<Pgn131008>("Pgn131008", &mut failures);
    idempotent::<Pgn131011>("Pgn131011", &mut failures);
    idempotent::<Pgn131012>("Pgn131012", &mut failures);
    idempotent::<Pgn61440>("Pgn61440", &mut failures);
    idempotent::<Pgn65001>("Pgn65001", &mut failures);
    idempotent::<Pgn65002>("Pgn65002", &mut failures);
    idempotent::<Pgn65003>("Pgn65003", &mut failures);
    idempotent::<Pgn65004>("Pgn65004", &mut failures);
    idempotent::<Pgn65005>("Pgn65005", &mut failures);
    idempotent::<Pgn65006>("Pgn65006", &mut failures);
    idempotent::<Pgn65007>("Pgn65007", &mut failures);
    idempotent::<Pgn65008>("Pgn65008", &mut failures);
    idempotent::<Pgn65009>("Pgn65009", &mut failures);
    idempotent::<Pgn65010>("Pgn65010", &mut failures);
    idempotent::<Pgn65011>("Pgn65011", &mut failures);
    idempotent::<Pgn65012>("Pgn65012", &mut failures);
    idempotent::<Pgn65013>("Pgn65013", &mut failures);
    idempotent::<Pgn65014>("Pgn65014", &mut failures);
    idempotent::<Pgn65015>("Pgn65015", &mut failures);
    idempotent::<Pgn65016>("Pgn65016", &mut failures);
    idempotent::<Pgn65017>("Pgn65017", &mut failures);
    idempotent::<Pgn65018>("Pgn65018", &mut failures);
    idempotent::<Pgn65019>("Pgn65019", &mut failures);
    idempotent::<Pgn65020>("Pgn65020", &mut failures);
    idempotent::<Pgn65021>("Pgn65021", &mut failures);
    idempotent::<Pgn65022>("Pgn65022", &mut failures);
    idempotent::<Pgn65023>("Pgn65023", &mut failures);
    idempotent::<Pgn65024>("Pgn65024", &mut failures);
    idempotent::<Pgn65025>("Pgn65025", &mut failures);
    idempotent::<Pgn65026>("Pgn65026", &mut failures);
    idempotent::<Pgn65027>("Pgn65027", &mut failures);
    idempotent::<Pgn65028>("Pgn65028", &mut failures);
    idempotent::<Pgn65029>("Pgn65029", &mut failures);
    idempotent::<Pgn65030>("Pgn65030", &mut failures);
    idempotent::<Pgn65240>("Pgn65240", &mut failures);
    idempotent::<Pgn65281>("Pgn65281", &mut failures);
    idempotent::<Pgn65282>("Pgn65282", &mut failures);
    idempotent::<Pgn65283>("Pgn65283", &mut failures);
    idempotent::<Pgn65284>("Pgn65284", &mut failures);
    idempotent::<Pgn65285>("Pgn65285", &mut failures);
    idempotent::<Pgn65286>("Pgn65286", &mut failures);
    idempotent::<Pgn65287>("Pgn65287", &mut failures);
    idempotent::<Pgn65290>("Pgn65290", &mut failures);
    idempotent::<Pgn65291>("Pgn65291", &mut failures);
    idempotent::<Pgn65292>("Pgn65292", &mut failures);
    idempotent::<Pgn65293>("Pgn65293", &mut failures);
    idempotent::<Pgn65294>("Pgn65294", &mut failures);
    idempotent::<Pgn65295>("Pgn65295", &mut failures);
    idempotent::<Pgn65296>("Pgn65296", &mut failures);
    idempotent::<Pgn65297>("Pgn65297", &mut failures);
    idempotent::<Pgn65298>("Pgn65298", &mut failures);
    idempotent::<Pgn65299>("Pgn65299", &mut failures);
    idempotent::<Pgn65300>("Pgn65300", &mut failures);
    idempotent::<Pgn65301>("Pgn65301", &mut failures);
    idempotent::<Pgn65302>("Pgn65302", &mut failures);
    idempotent::<Pgn65303>("Pgn65303", &mut failures);
    idempotent::<Pgn65304>("Pgn65304", &mut failures);
    idempotent::<Pgn65305>("Pgn65305", &mut failures);
    idempotent::<Pgn65306>("Pgn65306", &mut failures);
    idempotent::<Pgn65308>("Pgn65308", &mut failures);
    idempotent::<Pgn65309>("Pgn65309", &mut failures);
    idempotent::<Pgn65310>("Pgn65310", &mut failures);
    idempotent::<Pgn65311>("Pgn65311", &mut failures);
    idempotent::<Pgn65312>("Pgn65312", &mut failures);
    idempotent::<Pgn65313>("Pgn65313", &mut failures);
    idempotent::<Pgn65314>("Pgn65314", &mut failures);
    idempotent::<Pgn65315>("Pgn65315", &mut failures);
    idempotent::<Pgn65316>("Pgn65316", &mut failures);
    idempotent::<Pgn65317>("Pgn65317", &mut failures);
    idempotent::<Pgn65323>("Pgn65323", &mut failures);
    idempotent::<Pgn65324>("Pgn65324", &mut failures);
    idempotent::<Pgn65325>("Pgn65325", &mut failures);
    idempotent::<Pgn65329>("Pgn65329", &mut failures);
    idempotent::<Pgn65330>("Pgn65330", &mut failures);
    idempotent::<Pgn65332>("Pgn65332", &mut failures);
    idempotent::<Pgn65340>("Pgn65340", &mut failures);
    idempotent::<Pgn65341>("Pgn65341", &mut failures);
    idempotent::<Pgn65344>("Pgn65344", &mut failures);
    idempotent::<Pgn65345>("Pgn65345", &mut failures);
    idempotent::<Pgn65346>("Pgn65346", &mut failures);
    idempotent::<Pgn65348>("Pgn65348", &mut failures);
    idempotent::<Pgn65349>("Pgn65349", &mut failures);
    idempotent::<Pgn65350>("Pgn65350", &mut failures);
    idempotent::<Pgn65359>("Pgn65359", &mut failures);
    idempotent::<Pgn65360>("Pgn65360", &mut failures);
    idempotent::<Pgn65361>("Pgn65361", &mut failures);
    idempotent::<Pgn65371>("Pgn65371", &mut failures);
    idempotent::<Pgn65374>("Pgn65374", &mut failures);
    idempotent::<Pgn65379>("Pgn65379", &mut failures);
    idempotent::<Pgn65403>("Pgn65403", &mut failures);
    idempotent::<Pgn65408>("Pgn65408", &mut failures);
    idempotent::<Pgn65409>("Pgn65409", &mut failures);
    idempotent::<Pgn65410>("Pgn65410", &mut failures);
    idempotent::<Pgn65420>("Pgn65420", &mut failures);
    idempotent::<Pgn65424>("Pgn65424", &mut failures);
    idempotent::<Pgn65440>("Pgn65440", &mut failures);
    idempotent::<Pgn65441>("Pgn65441", &mut failures);
    idempotent::<Pgn65472>("Pgn65472", &mut failures);
    idempotent::<Pgn65480>("Pgn65480", &mut failures);
    assert!(failures.is_empty(), "{failures:#?}");
}

/// A `Duration` carrying a resolution takes its own branch in the generator. That
/// branch kept its own 32-bit threshold when the generic one moved to 24, so a
/// 32-bit duration stayed an f32 while the engine had switched to F64 — the setter
/// then rejected every payload.
#[cfg(feature = "full-pgns")]
#[test]
fn wide_scaled_duration_decodes() {
    use korri_n2k::protocol::messages::Pgn127496;

    // TimeToEmpty sits at bit 0, TripRunTime at bit 80; both 32 bits at 0.001.
    let mut payload = [0u8; 14];
    payload[0..4].copy_from_slice(&1_234_567u32.to_le_bytes());
    payload[10..14].copy_from_slice(&4_294_000u32.to_le_bytes());

    let pgn = Pgn127496::from_payload(&payload).expect("decode 127496");
    assert!(
        (pgn.time_to_empty - 1234.567).abs() < 0.0005,
        "{}",
        pgn.time_to_empty
    );
    assert!(
        (pgn.trip_run_time - 4294.0).abs() < 0.0005,
        "{}",
        pgn.trip_run_time
    );
}
