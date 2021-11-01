#![cfg_attr(not(feature = "std"), no_std)]

/// Note that the resulting `threshold` is the maximum number
/// of parties *not* enough to generate a signature,
/// i.e. at least `t+1` parties are required.
/// This follows the notation in the multisig library that
/// we are using and in the corresponding literature.
pub fn threshold_from_share_count(share_count: u32) -> u32 {
    ((share_count * 2) - 1) / 3
}

#[cfg(test)]
#[test]
fn check_threshold_calculation() {
    assert_eq!(threshold_from_share_count(150), 99);
    assert_eq!(threshold_from_share_count(100), 66);
    assert_eq!(threshold_from_share_count(90), 59);
    assert_eq!(threshold_from_share_count(3), 1);
    assert_eq!(threshold_from_share_count(4), 2);
}
