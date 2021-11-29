#![cfg_attr(not(feature = "std"), no_std)]

/// Note that the resulting `threshold` is the maximum number
/// of parties *not* enough to generate a signature,
/// i.e. at least `t+1` parties are required.
/// This follows the notation in the multisig library that
/// we are using and in the corresponding literature.

pub fn threshold_from_share_count(share_count: u32) -> u32 {
    if share_count == 0 {
        return 0;
    }
    ((share_count * 2) - 1) / 3
}

#[test]
fn check_threshold_calculation() {
    assert_eq!(threshold_from_share_count(150), 99);
    assert_eq!(threshold_from_share_count(100), 66);
    assert_eq!(threshold_from_share_count(90), 59);
    assert_eq!(threshold_from_share_count(3), 1);
    assert_eq!(threshold_from_share_count(4), 2);
}

use core::convert::TryInto;

pub fn clean_eth_address(dirty_eth_address: &str) -> Result<[u8; 20], &str> {
    let eth_address_hex_str = match dirty_eth_address.strip_prefix("0x") {
        Some(eth_address_stripped) => eth_address_stripped,
        None => dirty_eth_address,
    };

    let eth_address: [u8; 20] = hex::decode(eth_address_hex_str)
        .map_err(|_| "Invalid hex")?
        .try_into()
        .map_err(|_| "Could not create a [u8; 20]")?;

    Ok(eth_address)
}

#[test]
fn cleans_eth_address() {
    // fail too short
    let input = "0x323232";
    assert!(clean_eth_address(input).is_err());

    // fail invalid chars
    let input = "0xZ29aB9EbDb421CE48b70flippya6e9a3DBD609C5";
    assert!(clean_eth_address(input).is_err());

    // success with 0x
    let input = "0xB29aB9EbDb421CE48b70699758a6e9a3DBD609C5";
    assert!(clean_eth_address(input).is_ok());

    // success without 0x
    let input = "B29aB9EbDb421CE48b70699758a6e9a3DBD609C5";
    assert!(clean_eth_address(input).is_ok());
}
