use chainflip_common::types::{addresses::EthereumAddress, coin::Coin};
use hdwallet::{
    secp256k1::{PublicKey, Secp256k1, SecretKey},
    DefaultKeyChain, ExtendedPrivKey, ExtendedPubKey, KeyChain,
};
use std::{convert::TryFrom, str::FromStr};

/// Utils for decoding xpriv and xpub strings
mod raw_key;
pub use raw_key::RawKey;

/// BIP44 supported coin types
#[derive(Debug, Copy, Clone)]
pub enum CoinType {
    /// Bitcoin
    BTC,
    /// Etherum
    ETH,
}

impl CoinType {
    /// The coin index for bip44
    /// See: https://github.com/satoshilabs/slips/blob/master/slip-0044.md
    fn index(&self) -> u32 {
        match self {
            CoinType::BTC => 0,
            CoinType::ETH => 60,
        }
    }
}

impl TryFrom<Coin> for CoinType {
    type Error = &'static str;

    fn try_from(value: Coin) -> Result<Self, Self::Error> {
        match value {
            Coin::BTC => Ok(CoinType::BTC),
            Coin::ETH => Ok(CoinType::ETH),
            _ => Err("Cannot convert the given coin into a bip44 coin type"),
        }
    }
}

/// A representation of a Keypair
#[derive(Debug, Clone, PartialEq)]
pub struct KeyPair {
    /// The ECDSA public key
    pub public_key: PublicKey,
    /// The ECDSA private key
    pub private_key: SecretKey,
}

impl KeyPair {
    /// Construct a key pair from a hex private key
    ///
    /// # Example
    ///
    /// ```
    /// use chainflip::utils::bip44::KeyPair;
    ///
    /// let private_key = "58a99f6e6f89cbbb7fc8c86ea95e6012b68a9cd9a41c4ffa7c8f20c201d0667f";
    /// let key_pair = KeyPair::from_private_key(private_key).unwrap();
    ///
    /// assert_eq!(
    ///     format!("{:x}", key_pair.public_key),
    ///     "034ac1bb1bc5fd7a9b173f6a136a40e4be64841c77d7f66ead444e101e01348127"
    /// );
    ///
    /// assert_eq!(
    ///     format!("{:x}", key_pair.private_key),
    ///     "58a99f6e6f89cbbb7fc8c86ea95e6012b68a9cd9a41c4ffa7c8f20c201d0667f"
    /// );
    /// ```
    pub fn from_private_key(hex: &str) -> Result<KeyPair, String> {
        let secp = Secp256k1::signing_only();
        let private_key = SecretKey::from_str(hex).map_err(|error| error.to_string())?;
        let public_key = PublicKey::from_secret_key(&secp, &private_key);
        Ok(KeyPair {
            private_key,
            public_key,
        })
    }
}

/// Derive a key pair from the given `master_key`
///
/// # Example
///
/// ```
/// use chainflip::utils::bip44::{get_key_pair, RawKey, CoinType};
///
/// let xpriv = "xprv9s21ZrQH143K2h2Jo5HX95FFUbu8QYXRDvmpStejFQQXSYw7LnsuczMXvfh9mVFCukNz6bXoYDSZhMzwQqtoDeMFkjG8PqzHCf4kDHYwYqK";
/// let root_key = RawKey::decode(xpriv).unwrap().to_private_key().unwrap();
/// let key_pair = get_key_pair(root_key, CoinType::BTC, 0).unwrap();
///
/// assert_eq!(
///     format!("{:x}", key_pair.public_key),
///     "034ac1bb1bc5fd7a9b173f6a136a40e4be64841c77d7f66ead444e101e01348127"
/// );
///
/// assert_eq!(
///     format!("{:x}", key_pair.private_key),
///     "58a99f6e6f89cbbb7fc8c86ea95e6012b68a9cd9a41c4ffa7c8f20c201d0667f"
/// );
/// ```
pub fn get_key_pair(
    root_key: ExtendedPrivKey,
    coin: CoinType,
    address_index: u32,
) -> Result<KeyPair, String> {
    let priv_key = derive_private_key(root_key, coin, address_index)?;
    let pub_key = ExtendedPubKey::from_private_key(&priv_key);

    Ok(KeyPair {
        private_key: priv_key.private_key,
        public_key: pub_key.public_key,
    })
}

/// Derive a private key from the given `master_key`, `coin` and `address_index`
fn derive_private_key(
    root_key: ExtendedPrivKey,
    coin: CoinType,
    address_index: u32,
) -> Result<ExtendedPrivKey, String> {
    // Derivation path we're using: m/44'/coin_type'/0'/0/address_index
    // See: https://github.com/bitcoin/bips/blob/master/bip-0044.mediawiki#path-levels
    // Note: If we move to using extended public keys to derive public keys then hardened paths won't work
    let derivation_path = format!("m/44H/{}H/0H/0/{}", coin.index(), address_index);

    let key_chain = DefaultKeyChain::new(root_key);
    let (child_key, _) = key_chain
        .derive_private_key(derivation_path.into())
        .map_err(|err| format!("{:?}", err))?;

    Ok(child_key)
}

impl From<KeyPair> for EthereumAddress {
    fn from(keys: KeyPair) -> EthereumAddress {
        EthereumAddress::from_public_key(keys.public_key.serialize_uncompressed())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_derive_private_key() {
        let xpriv = "xprv9s21ZrQH143K2h2Jo5HX95FFUbu8QYXRDvmpStejFQQXSYw7LnsuczMXvfh9mVFCukNz6bXoYDSZhMzwQqtoDeMFkjG8PqzHCf4kDHYwYqK";
        let root_key = RawKey::decode(xpriv).unwrap().to_private_key().unwrap();
        for coin in vec![CoinType::BTC, CoinType::ETH] {
            for index in vec![0, 1, 999, u32::MAX] {
                assert!(
                    derive_private_key(root_key.clone(), coin, index).is_ok(),
                    "Expected to generate a key pair for index {}",
                    index
                )
            }
        }
    }
}
