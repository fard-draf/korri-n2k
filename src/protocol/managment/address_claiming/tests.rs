use super::*;
#[test]
fn addr_capable_and_isoname_match() {
    let dut_name_no_aac: u64 = 0x0234567890ABCDEF; // AAC disabled
    let dut_name_aac: u64 = 0xF234567890ABCDEE; // AAC enabled

    const DUT_ADDRESSES: &[u8] = &[145, 158];
    let preferred = 145;

    let strategy_fixed = AddressClaimStrategy::Fixed { preferred };
    let strategy_arbitrary = AddressClaimStrategy::Arbitrary { preferred };
    let strategy_self_conf = AddressClaimStrategy::SelfConfigurable {
        addresses: DUT_ADDRESSES,
    };

    assert!(is_addr_capable_and_isoname_match(
        dut_name_no_aac,
        strategy_fixed
    ));
    assert!(!is_addr_capable_and_isoname_match(
        dut_name_aac,
        strategy_fixed
    ));
    assert!(is_addr_capable_and_isoname_match(
        dut_name_aac,
        strategy_arbitrary
    ));
    assert!(!is_addr_capable_and_isoname_match(
        dut_name_no_aac,
        strategy_arbitrary
    ));
    assert!(is_addr_capable_and_isoname_match(
        dut_name_no_aac,
        strategy_self_conf
    ));
    assert!(!is_addr_capable_and_isoname_match(
        dut_name_aac,
        strategy_self_conf
    ));
}

#[test]
fn arbitrary_emits_the_preferred_then_walks_upwards() {
    let mut it = AddressClaimIterator::new(AddressClaimStrategy::Arbitrary { preferred: 130 });
    assert_eq!(it.next(), Some(130));
    assert_eq!(it.next(), Some(131));
    assert_eq!(it.next(), Some(132));
}

#[test]
fn arbitrary_wraps_around_to_reach_the_low_addresses() {
    // Real buses cluster near zero, so a node preferring a high address must
    // still be able to walk down to them.
    let mut it = AddressClaimIterator::new(AddressClaimStrategy::Arbitrary {
        preferred: address::MAX_CLAIMABLE - 1,
    });
    assert_eq!(it.next(), Some(address::MAX_CLAIMABLE - 1));
    assert_eq!(it.next(), Some(address::MAX_CLAIMABLE));
    assert_eq!(it.next(), Some(address::MIN_CLAIMABLE));
    assert_eq!(it.next(), Some(address::MIN_CLAIMABLE + 1));
}

#[test]
fn arbitrary_covers_each_claimable_address_exactly_once() {
    for preferred in [address::MIN_CLAIMABLE, 43, 130, address::MAX_CLAIMABLE] {
        let mut seen = [false; address::CLAIMABLE_COUNT];
        let mut count = 0;
        for addr in AddressClaimIterator::new(AddressClaimStrategy::Arbitrary { preferred }) {
            assert!(address::is_claimable(addr), "yielded {addr}, not claimable");
            assert!(!seen[addr as usize], "yielded {addr} twice");
            seen[addr as usize] = true;
            count += 1;
        }
        assert_eq!(count, address::CLAIMABLE_COUNT, "preferred={preferred}");
    }
}

#[test]
fn every_claimable_address_is_reachable_including_the_low_ones() {
    // The bug this replaces: scanning started at 128, so a node could never
    // claim an address where real devices actually live.
    let mut reachable = [false; address::CLAIMABLE_COUNT];
    for addr in AddressClaimIterator::new(AddressClaimStrategy::Arbitrary { preferred: 130 }) {
        reachable[addr as usize] = true;
    }
    for addr in [0u8, 10, 43, 127, 208, address::MAX_CLAIMABLE] {
        assert!(reachable[addr as usize], "{addr} unreachable");
    }
}

#[test]
fn fixed_round_with_preferred() {
    let preferred = 177;
    let mut it = AddressClaimIterator::new(AddressClaimStrategy::Fixed { preferred });
    assert_eq!(it.next(), Some(177));
    assert_eq!(it.next(), None);
}

#[test]
fn self_configurable_round_with_sorted_array_and_unvalid_address() {
    // Only 252..=255 are out of reach; 210 and 222 are ordinary addresses.
    const ADDRESSES: &[u8] = &[128, 158, 199, 210, 222, 252, 253, 254, 255];
    let mut it = AddressClaimIterator::new(AddressClaimStrategy::SelfConfigurable {
        addresses: ADDRESSES,
    });

    assert_eq!(it.next(), Some(128));
    assert_eq!(it.next(), Some(158));
    assert_eq!(it.next(), Some(199));
    assert_eq!(it.next(), Some(210));
    assert_eq!(it.next(), Some(222));
    assert_eq!(it.next(), None);
}

#[test]
fn self_configurable_round_with_unsorted_array_and_unvalid_addresses() {
    // Order is preserved, and only 252..=255 are filtered out.
    const ADDRESSES: &[u8] = &[253, 245, 254, 235, 255, 128, 222, 108, 252];
    let mut it = AddressClaimIterator::new(AddressClaimStrategy::SelfConfigurable {
        addresses: ADDRESSES,
    });

    assert_eq!(it.next(), Some(245));
    assert_eq!(it.next(), Some(235));
    assert_eq!(it.next(), Some(128));
    assert_eq!(it.next(), Some(222));
    assert_eq!(it.next(), Some(108));
    assert_eq!(it.next(), None);
}
