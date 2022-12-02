#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "std")]
mod with_std;
#[cfg(feature = "std")]
pub use with_std::*;

pub type Port = u16;

/// Simply unwraps the value. Advantage of this is to make it clear in tests
/// what we are testing
#[macro_export]
macro_rules! assert_ok {
	($result:expr) => {
		$result.unwrap()
	};
}

#[macro_export]
macro_rules! assert_err {
	($result:expr) => {
		$result.unwrap_err()
	};
}

#[cfg(test)]
mod test_asserts {
	use crate::assert_panics;

	#[test]
	fn test_assert_ok_unwrap_ok() {
		fn works() -> Result<i32, i32> {
			Ok(1)
		}
		let result = assert_ok!(works());
		assert_eq!(result, 1);
	}

	#[test]
	fn test_assert_ok_err() {
		assert_panics!(assert_ok!(Err::<u32, u32>(1)));
	}
}

/// Note that the resulting `threshold` is the maximum number
/// of parties *not* enough to generate a signature,
/// i.e. at least `t+1` parties are required.
/// This follows the notation in the multisig library that
/// we are using and in the corresponding literature.
///
/// For the *success* threshold, use [success_threshold_from_share_count].
pub fn threshold_from_share_count(share_count: u32) -> u32 {
	if 0 == share_count {
		0
	} else {
		(share_count.checked_mul(2).unwrap() - 1) / 3
	}
}

/// Returns the number of parties required for a threshold signature
/// ceremony to *succeed*.
pub fn success_threshold_from_share_count(share_count: u32) -> u32 {
	threshold_from_share_count(share_count).checked_add(1).unwrap()
}

/// Returns the number of bad parties required for a threshold signature
/// ceremony to *fail*.
pub fn failure_threshold_from_share_count(share_count: u32) -> u32 {
	share_count - threshold_from_share_count(share_count)
}

#[test]
fn check_threshold_calculation() {
	assert_eq!(threshold_from_share_count(150), 99);
	assert_eq!(threshold_from_share_count(100), 66);
	assert_eq!(threshold_from_share_count(90), 59);
	assert_eq!(threshold_from_share_count(3), 1);
	assert_eq!(threshold_from_share_count(4), 2);

	assert_eq!(success_threshold_from_share_count(150), 100);
	assert_eq!(success_threshold_from_share_count(100), 67);
	assert_eq!(success_threshold_from_share_count(90), 60);
	assert_eq!(success_threshold_from_share_count(3), 2);
	assert_eq!(success_threshold_from_share_count(4), 3);

	assert_eq!(failure_threshold_from_share_count(150), 51);
	assert_eq!(failure_threshold_from_share_count(100), 34);
	assert_eq!(failure_threshold_from_share_count(90), 31);
	assert_eq!(failure_threshold_from_share_count(3), 2);
	assert_eq!(failure_threshold_from_share_count(4), 2);
}

pub fn clean_hex_address<const LEN: usize>(address_str: &str) -> Result<[u8; LEN], &str> {
	let address_hex_str = match address_str.strip_prefix("0x") {
		Some(address_stripped) => address_stripped,
		None => address_str,
	};

	let address: [u8; LEN] = hex::decode(address_hex_str)
		.map_err(|_| "Invalid hex")?
		.try_into()
		.map_err(|_| "Invalid address length")?;

	Ok(address)
}

pub fn clean_eth_address(dirty_eth_address: &str) -> Result<[u8; 20], &str> {
	clean_hex_address(dirty_eth_address)
}

pub fn clean_dot_address(dirty_dot_address: &str) -> Result<[u8; 32], &str> {
	clean_hex_address(dirty_dot_address)
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
