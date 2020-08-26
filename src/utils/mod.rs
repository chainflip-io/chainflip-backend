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
