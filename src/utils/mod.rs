use std::convert::AsMut;

/// The test utils
pub mod test_utils;

/// Loki utils
pub mod loki;

/// Utils for generating HD wallets (bip32/bip44)
pub mod bip44;

use hdwallet::secp256k1::PublicKey;
use hex;
use tiny_keccak::{Hasher, Keccak};

/// Clone slice values into an array
///
/// # Example
///
/// ```
/// use blockswap::utils::clone_into_array;
///
/// let original = [1, 2, 3, 4, 5];
/// let cloned: [u8; 4] = clone_into_array(&original[..4]);
/// assert_eq!(cloned, [1, 2, 3, 4]);
/// ```
pub fn clone_into_array<A, T>(slice: &[T]) -> A
where
    A: Sized + Default + AsMut<[T]>,
    T: Clone,
{
    let mut a = Default::default();
    <A as AsMut<[T]>>::as_mut(&mut a).clone_from_slice(slice);
    a
}

/// Get the ethereum address from a ECDSA public key
///
/// # Example
///
/// ```
/// use blockswap::utils::{get_ethereum_address, bip44::KeyPair};
/// use hdwallet::secp256k1::PublicKey;
/// use std::str::FromStr;
///
/// let public_key = PublicKey::from_str("034ac1bb1bc5fd7a9b173f6a136a40e4be64841c77d7f66ead444e101e01348127").unwrap();
/// let address = get_ethereum_address(public_key);
///
/// assert_eq!(address, "0x70e7db0678460c5e53f1ffc9221d1c692111dcc5".to_owned());
/// ```
pub fn get_ethereum_address(public_key: PublicKey) -> String {
    let bytes: [u8; 65] = public_key.serialize_uncompressed();

    // apply a keccak_256 hash of the public key
    let mut result = [0u8; 32];
    let mut hasher = Keccak::v256();
    hasher.update(&bytes[1..]); // Strip the first byte to get 64 bytes
    hasher.finalize(&mut result);

    // The last 20 bytes in hex is the ethereum address
    let address = &result[12..];
    format!("0x{}", hex::encode(address))
}
