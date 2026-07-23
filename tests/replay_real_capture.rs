#![cfg(feature = "full-pgns")]
//! Replay of a real NMEA 2000 backbone capture through the whole stack:
//! CAN identifier, Fast Packet reassembly, then decoding of every PGN.
//!
//! Skipped when the capture is absent, so CI is unaffected. Point it at another
//! recording with KORRI_N2K_CAPTURE=/path/to/capture.bin.
use korri_n2k::infra::codec::traits::PgnData;
use korri_n2k::protocol::messages::*;
use korri_n2k::protocol::transport::can_id::CanId;
use korri_n2k::protocol::transport::fast_packet::assembler::{FastPacketAssembler, ProcessResult};
use std::collections::BTreeMap;

const CAPTURE: &str = "../korri-n2k-examples/xtensa/esp32-s3/capture_long.bin";
const MAGIC: &[u8] = b"KN2KCAP\x01";

fn is_fast_packet(pgn: u32) -> bool {
    matches!(
        pgn,
        126464
            | 126720
            | 126983
            | 126984
            | 126985
            | 126986
            | 126987
            | 126988
            | 126996
            | 126998
            | 127237
            | 127489
            | 127490
            | 127491
            | 127494
            | 127495
            | 127496
            | 127497
            | 127498
            | 127503
            | 127506
            | 127507
            | 127509
            | 127510
            | 127511
            | 127512
            | 127513
            | 127514
            | 127751
            | 128275
            | 128520
            | 128538
            | 129029
            | 129038
            | 129039
            | 129040
            | 129041
            | 129044
            | 129045
            | 129284
            | 129285
            | 129301
            | 129302
            | 129538
            | 129540
            | 129541
            | 129542
            | 129545
            | 129547
            | 129549
            | 129551
            | 129556
            | 129793
            | 129794
            | 129796
            | 129798
            | 129799
            | 129800
            | 129801
            | 129802
            | 129803
            | 129804
            | 129805
            | 129806
            | 129807
            | 129809
            | 129810
            | 129813
            | 130052
            | 130053
            | 130054
            | 130060
            | 130061
            | 130064
            | 130065
            | 130066
            | 130070
            | 130073
            | 130320
            | 130321
            | 130322
            | 130323
            | 130324
            | 130329
            | 130330
            | 130561
            | 130562
            | 130563
            | 130564
            | 130565
            | 130566
            | 130567
            | 130568
            | 130569
            | 130570
            | 130571
            | 130572
            | 130574
            | 130575
            | 130577
            | 130578
            | 130580
            | 130583
            | 130586
            | 130817
            | 130819
            | 130821
            | 130825
            | 130826
            | 130827
            | 130828
            | 130829
            | 130830
            | 130831
            | 130832
            | 130833
            | 130834
            | 130835
            | 130836
            | 130837
            | 130838
            | 130839
            | 130840
            | 130841
            | 130842
            | 130843
            | 130844
            | 130847
            | 130848
            | 130849
            | 130850
            | 130851
            | 130856
            | 130860
            | 130880
            | 130881
            | 130900
            | 130910
            | 130911
            | 130912
            | 130913
            | 130918
            | 130921
            | 130939
            | 130944
            | 130945
            | 130946
            | 130947
            | 130951
            | 131008
            | 131011
            | 131012
    )
}

fn decode(pgn: u32, data: &[u8]) -> Option<Result<(), String>> {
    let r = match pgn {
        59904 => Pgn59904::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        60160 => Pgn60160::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        60416 => Pgn60416::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        60928 => Pgn60928::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        61440 => Pgn61440::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65001 => Pgn65001::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65002 => Pgn65002::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65003 => Pgn65003::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65004 => Pgn65004::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65005 => Pgn65005::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65006 => Pgn65006::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65007 => Pgn65007::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65008 => Pgn65008::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65009 => Pgn65009::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65010 => Pgn65010::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65011 => Pgn65011::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65012 => Pgn65012::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65013 => Pgn65013::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65014 => Pgn65014::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65015 => Pgn65015::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65016 => Pgn65016::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65017 => Pgn65017::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65018 => Pgn65018::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65019 => Pgn65019::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65020 => Pgn65020::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65021 => Pgn65021::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65022 => Pgn65022::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65023 => Pgn65023::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65024 => Pgn65024::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65025 => Pgn65025::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65026 => Pgn65026::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65027 => Pgn65027::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65028 => Pgn65028::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65029 => Pgn65029::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65030 => Pgn65030::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65240 => Pgn65240::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65281 => Pgn65281::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65282 => Pgn65282::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65283 => Pgn65283::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65284 => Pgn65284::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65285 => Pgn65285::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65286 => Pgn65286::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65287 => Pgn65287::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65290 => Pgn65290::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65291 => Pgn65291::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65292 => Pgn65292::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65293 => Pgn65293::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65294 => Pgn65294::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65295 => Pgn65295::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65296 => Pgn65296::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65297 => Pgn65297::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65298 => Pgn65298::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65299 => Pgn65299::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65300 => Pgn65300::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65301 => Pgn65301::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65302 => Pgn65302::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65303 => Pgn65303::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65304 => Pgn65304::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65305 => Pgn65305::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65306 => Pgn65306::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65308 => Pgn65308::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65309 => Pgn65309::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65310 => Pgn65310::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65311 => Pgn65311::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65312 => Pgn65312::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65313 => Pgn65313::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65314 => Pgn65314::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65315 => Pgn65315::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65316 => Pgn65316::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65317 => Pgn65317::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65323 => Pgn65323::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65324 => Pgn65324::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65325 => Pgn65325::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65329 => Pgn65329::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65330 => Pgn65330::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65332 => Pgn65332::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65340 => Pgn65340::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65341 => Pgn65341::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65344 => Pgn65344::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65345 => Pgn65345::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65346 => Pgn65346::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65348 => Pgn65348::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65349 => Pgn65349::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65350 => Pgn65350::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65359 => Pgn65359::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65360 => Pgn65360::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65361 => Pgn65361::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65371 => Pgn65371::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65374 => Pgn65374::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65379 => Pgn65379::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65403 => Pgn65403::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65408 => Pgn65408::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65409 => Pgn65409::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65410 => Pgn65410::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65420 => Pgn65420::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65424 => Pgn65424::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65440 => Pgn65440::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65441 => Pgn65441::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65472 => Pgn65472::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        65480 => Pgn65480::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        126464 => Pgn126464::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        126720 => Pgn126720::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        126976 => Pgn126976::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        126983 => Pgn126983::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        126984 => Pgn126984::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        126985 => Pgn126985::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        126986 => Pgn126986::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        126987 => Pgn126987::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        126988 => Pgn126988::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        126992 => Pgn126992::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        126993 => Pgn126993::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        126996 => Pgn126996::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        126998 => Pgn126998::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        127237 => Pgn127237::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        127245 => Pgn127245::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        127250 => Pgn127250::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        127251 => Pgn127251::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        127252 => Pgn127252::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        127257 => Pgn127257::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        127258 => Pgn127258::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        127488 => Pgn127488::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        127489 => Pgn127489::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        127490 => Pgn127490::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        127491 => Pgn127491::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        127493 => Pgn127493::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        127494 => Pgn127494::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        127495 => Pgn127495::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        127496 => Pgn127496::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        127497 => Pgn127497::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        127498 => Pgn127498::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        127500 => Pgn127500::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        127501 => Pgn127501::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        127502 => Pgn127502::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        127503 => Pgn127503::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        127505 => Pgn127505::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        127506 => Pgn127506::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        127507 => Pgn127507::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        127508 => Pgn127508::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        127509 => Pgn127509::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        127510 => Pgn127510::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        127511 => Pgn127511::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        127512 => Pgn127512::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        127513 => Pgn127513::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        127514 => Pgn127514::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        127744 => Pgn127744::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        127745 => Pgn127745::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        127746 => Pgn127746::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        127747 => Pgn127747::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        127748 => Pgn127748::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        127749 => Pgn127749::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        127750 => Pgn127750::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        127751 => Pgn127751::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        128000 => Pgn128000::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        128001 => Pgn128001::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        128002 => Pgn128002::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        128003 => Pgn128003::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        128006 => Pgn128006::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        128007 => Pgn128007::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        128008 => Pgn128008::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        128259 => Pgn128259::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        128267 => Pgn128267::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        128275 => Pgn128275::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        128520 => Pgn128520::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        128538 => Pgn128538::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        128768 => Pgn128768::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        128769 => Pgn128769::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        128776 => Pgn128776::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        128777 => Pgn128777::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        128778 => Pgn128778::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        128780 => Pgn128780::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        129025 => Pgn129025::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        129026 => Pgn129026::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        129027 => Pgn129027::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        129028 => Pgn129028::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        129029 => Pgn129029::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        129033 => Pgn129033::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        129038 => Pgn129038::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        129039 => Pgn129039::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        129040 => Pgn129040::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        129041 => Pgn129041::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        129044 => Pgn129044::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        129045 => Pgn129045::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        129283 => Pgn129283::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        129284 => Pgn129284::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        129285 => Pgn129285::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        129291 => Pgn129291::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        129301 => Pgn129301::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        129302 => Pgn129302::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        129538 => Pgn129538::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        129539 => Pgn129539::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        129540 => Pgn129540::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        129541 => Pgn129541::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        129542 => Pgn129542::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        129545 => Pgn129545::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        129546 => Pgn129546::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        129547 => Pgn129547::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        129549 => Pgn129549::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        129550 => Pgn129550::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        129551 => Pgn129551::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        129556 => Pgn129556::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        129793 => Pgn129793::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        129794 => Pgn129794::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        129796 => Pgn129796::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        129798 => Pgn129798::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        129799 => Pgn129799::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        129800 => Pgn129800::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        129801 => Pgn129801::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        129802 => Pgn129802::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        129803 => Pgn129803::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        129804 => Pgn129804::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        129805 => Pgn129805::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        129806 => Pgn129806::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        129807 => Pgn129807::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        129809 => Pgn129809::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        129810 => Pgn129810::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        129813 => Pgn129813::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130052 => Pgn130052::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130053 => Pgn130053::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130054 => Pgn130054::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130060 => Pgn130060::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130061 => Pgn130061::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130064 => Pgn130064::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130065 => Pgn130065::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130066 => Pgn130066::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130070 => Pgn130070::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130073 => Pgn130073::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130306 => Pgn130306::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130310 => Pgn130310::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130311 => Pgn130311::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130312 => Pgn130312::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130313 => Pgn130313::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130314 => Pgn130314::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130315 => Pgn130315::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130316 => Pgn130316::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130320 => Pgn130320::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130321 => Pgn130321::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130322 => Pgn130322::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130323 => Pgn130323::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130324 => Pgn130324::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130329 => Pgn130329::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130330 => Pgn130330::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130560 => Pgn130560::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130561 => Pgn130561::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130562 => Pgn130562::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130563 => Pgn130563::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130564 => Pgn130564::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130565 => Pgn130565::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130566 => Pgn130566::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130567 => Pgn130567::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130568 => Pgn130568::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130569 => Pgn130569::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130570 => Pgn130570::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130571 => Pgn130571::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130572 => Pgn130572::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130574 => Pgn130574::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130575 => Pgn130575::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130576 => Pgn130576::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130577 => Pgn130577::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130578 => Pgn130578::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130579 => Pgn130579::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130580 => Pgn130580::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130582 => Pgn130582::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130583 => Pgn130583::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130585 => Pgn130585::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130586 => Pgn130586::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130817 => Pgn130817::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130819 => Pgn130819::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130821 => Pgn130821::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130825 => Pgn130825::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130826 => Pgn130826::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130827 => Pgn130827::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130828 => Pgn130828::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130829 => Pgn130829::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130830 => Pgn130830::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130831 => Pgn130831::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130832 => Pgn130832::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130833 => Pgn130833::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130834 => Pgn130834::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130835 => Pgn130835::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130836 => Pgn130836::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130837 => Pgn130837::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130838 => Pgn130838::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130839 => Pgn130839::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130840 => Pgn130840::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130841 => Pgn130841::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130842 => Pgn130842::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130843 => Pgn130843::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130844 => Pgn130844::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130847 => Pgn130847::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130848 => Pgn130848::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130849 => Pgn130849::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130850 => Pgn130850::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130851 => Pgn130851::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130856 => Pgn130856::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130860 => Pgn130860::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130880 => Pgn130880::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130881 => Pgn130881::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130900 => Pgn130900::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130910 => Pgn130910::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130911 => Pgn130911::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130912 => Pgn130912::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130913 => Pgn130913::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130918 => Pgn130918::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130921 => Pgn130921::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130939 => Pgn130939::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130944 => Pgn130944::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130945 => Pgn130945::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130946 => Pgn130946::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130947 => Pgn130947::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        130951 => Pgn130951::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        131008 => Pgn131008::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        131011 => Pgn131011::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        131012 => Pgn131012::from_payload(data)
            .map(|_| ())
            .map_err(|e| format!("{e:?}")),
        _ => return None,
    };
    Some(r)
}

#[test]
fn replay_real_backbone_capture() {
    let path = std::env::var("KORRI_N2K_CAPTURE").unwrap_or_else(|_| CAPTURE.to_string());
    let Ok(bytes) = std::fs::read(&path) else {
        eprintln!("no capture at {path}, skipping");
        return;
    };

    let start = bytes
        .windows(MAGIC.len())
        .position(|w| w == MAGIC)
        .expect("session header");
    let mut assembler = FastPacketAssembler::new();

    let (mut frames, mut single, mut fp_frames, mut completed) = (0u64, 0u64, 0u64, 0u64);
    let mut ok: BTreeMap<u32, u64> = BTreeMap::new();
    let mut errs: BTreeMap<u32, BTreeMap<String, u64>> = BTreeMap::new();
    let mut unknown: BTreeMap<u32, u64> = BTreeMap::new();

    for rec in bytes[start..].chunks_exact(24) {
        if rec.starts_with(MAGIC) || rec[0] != 0x01 {
            continue;
        }
        let len = (rec[1] & 0x0F) as usize;
        let flags = rec[1] >> 4;
        if flags & 0b0001 == 0 || flags & 0b0010 != 0 {
            continue;
        }

        let raw = u32::from_le_bytes(rec[4..8].try_into().unwrap());
        let ts_us = u64::from_le_bytes(rec[8..16].try_into().unwrap());
        let data = &rec[16..16 + len.min(8)];

        let id = CanId(raw);
        let pgn = id.pgn();
        let src = id.source_address();
        frames += 1;

        let payload: Vec<u8>;
        let slice: &[u8] = if is_fast_packet(pgn) {
            fp_frames += 1;
            if data.len() < 8 {
                continue;
            }
            let mut buf = [0u8; 8];
            buf.copy_from_slice(data);
            match assembler.process_frame((ts_us / 1000) as u32, pgn, src, &buf) {
                ProcessResult::MessageComplete(m) => {
                    completed += 1;
                    payload = m.payload[..m.len].to_vec();
                    &payload
                }
                _ => continue,
            }
        } else {
            single += 1;
            data
        };

        match decode(pgn, slice) {
            None => {
                *unknown.entry(pgn).or_default() += 1;
            }
            Some(Ok(())) => {
                *ok.entry(pgn).or_default() += 1;
            }
            Some(Err(e)) => {
                *errs.entry(pgn).or_default().entry(e).or_default() += 1;
            }
        }
    }

    let total_ok: u64 = ok.values().sum();
    let total_err: u64 = errs.values().flat_map(|m| m.values()).sum();
    let total_unknown: u64 = unknown.values().sum();

    println!("frames {frames} | single {single} | fast-packet {fp_frames} -> {completed} messages");
    println!(
        "decoded OK {total_ok} | errors {total_err} | PGN absent from manifest {total_unknown}"
    );
    println!();
    println!("--- failures by PGN");
    for (pgn, kinds) in &errs {
        let n: u64 = kinds.values().sum();
        let good = ok.get(pgn).copied().unwrap_or(0);
        println!("  {pgn:>6}  {n:>7} failed / {good:>7} ok");
        for (k, c) in kinds {
            println!("            {c:>7}  {k}");
        }
    }
    println!();
    println!("--- PGNs seen but absent from the manifest");
    for (pgn, n) in &unknown {
        println!("  {pgn:>6}  {n:>7}");
    }

    assert!(total_ok > 250_000, "only {total_ok} messages decoded");
    // Residual: 4 Simnet 130850 frames whose variant declares 14 bytes while the
    // autopilot sends 12. Absent trailing numbers are missing data, not padding,
    // so they stay an error rather than being defaulted silently.
    assert!(
        total_err * 1000 < total_ok,
        "{total_err} failures for {total_ok} successes"
    );
}
