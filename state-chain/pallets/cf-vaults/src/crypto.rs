//! Cryptography helper methods

use core::array::TryFromSliceError;
use core::convert::TryInto;
use sp_std::prelude::*;

/// Takes a 33 byte serialized compressed pubkey as a Vec<u8>
/// strip y-parity from key (first byte) and use 0 if even, 1 if odd
// https://github.com/chainflip-io/chainflip-eth-contracts/blob/master/contracts/abstract/SchnorrSECP256K1.sol
// https://github.com/chainflip-io/chainflip-eth-contracts/blob/master/tests/crypto.py
pub fn destructure_pubkey(pubkey_bytes: Vec<u8>) -> Result<([u8; 32], u8), TryFromSliceError> {
	let pubkey_compressed: [u8; 33] = pubkey_bytes[..].try_into()?;
	let pubkey_y_parity = if pubkey_compressed[0] == 2 { 0u8 } else { 1u8 };
	let pubkey_x: [u8; 32] = pubkey_compressed[1..].try_into()?;
	Ok((pubkey_x, pubkey_y_parity))
}
