//! Various utilities for manipulating Ethereum related data.

use tiny_keccak::{Hasher, Keccak};

/// Compute the Keccak-256 hash of input bytes.
///
/// Note that strings are interpreted as UTF-8 bytes,
// TODO: Add Solidity Keccak256 packing support
pub fn keccak256<T: AsRef<[u8]>>(bytes: T) -> [u8; 32] {
	let mut output = [0u8; 32];

	let mut hasher = Keccak::v256();
	hasher.update(bytes.as_ref());
	hasher.finalize(&mut output);

	output
}
