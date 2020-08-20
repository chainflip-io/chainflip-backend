use crate::utils::clone_into_array;
use bs58;
use hdwallet::{ExtendedPrivKey, ExtendedPubKey};

const INVALID_KEY: &str = "Invalid key";
const INVALID_KEY_LENGTH: &str = "Invalid key length";

/// Type alias for an error which returns a static string
type StaticError = &'static str;

/// A small util enum to help identify key types
#[derive(Debug, PartialEq, Eq)]
enum KeyType {
    Public,
    Private,
}

impl KeyType {
    /// Get the key type from version bytes
    fn from(bytes: &[u8]) -> Option<Self> {
        match bytes {
            // 0x0488B21E | 0x043587CF
            [4, 136, 178, 30] | [4, 53, 135, 207] => Some(KeyType::Public),
            // 0x0488ADE4 | 0x04358394
            [4, 136, 173, 228] | [4, 53, 131, 148] => Some(KeyType::Private),
            _ => None,
        }
    }
}

/// A BIP32 raw key representation
/// See: https://github.com/bitcoin/bips/blob/master/bip-0032.mediawiki#serialization-format
#[derive(Debug, Eq, PartialEq)]
pub struct RawKey {
    /// 4 byte: version bytes (mainnet: 0x0488B21E public, 0x0488ADE4 private; testnet: 0x043587CF public,  private)
    version: [u8; 4],
    /// 1 byte: depth: 0x00 for master nodes, 0x01 for level-1 derived keys, ....
    depth: [u8; 1],
    /// 4 bytes: the fingerprint of the parent's key (0x00000000 if master key)
    fingerprint: [u8; 4],
    /// 4 bytes: child number. This is ser32(i) for i in xi = xpar/i, with xi the key being serialized. (0x00000000 if master key)
    child_number: [u8; 4],
    /// 32 bytes: the chain code
    chain_code: [u8; 32],
    /// 33 bytes: the public key or private key data (33 byte public keys, last 32 bytes for private key)
    /// Represented as a Vector here because rust only supports trait functions in arrays with upto 32 bytes.
    data: Vec<u8>,
}

impl RawKey {
    /// Parse an extended key from a string
    ///
    /// # Example
    ///
    /// ```
    /// use blockswap::utils::bip44::RawKey;
    ///
    /// let key = "xpub661MyMwAqRbcG8Zah6TcX3QpP5yJApaXcyLK8CJcZkuYjczivsHxVL5qm9cw8BYLYehgFeddK5WrxhntpcvqJKTVg96dUVL9P7hZ7Kcvqvd";
    /// let raw_key = RawKey::decode(key);
    /// assert!(raw_key.is_ok());
    /// ```
    pub fn decode(key: &str) -> Result<Self, StaticError> {
        let bytes = bs58::decode(key)
            .with_alphabet(bs58::alphabet::BITCOIN)
            .into_vec()
            .map_err(|_| INVALID_KEY)?;

        if bytes.len() < 78 {
            return Err(INVALID_KEY_LENGTH);
        }

        Ok(RawKey {
            version: clone_into_array(&bytes[..4]),
            depth: clone_into_array(&bytes[4..5]),
            fingerprint: clone_into_array(&bytes[5..9]),
            child_number: clone_into_array(&bytes[9..13]),
            chain_code: clone_into_array(&bytes[13..45]),
            data: bytes[45..78].to_vec(),
            // There's also a 4 byte base58 checksum but we ignore it
        })
    }

    /// Convert to an ExtendedPubKey
    ///
    /// # Example
    ///
    /// ## Successful conversion
    ///
    /// ```
    /// use blockswap::utils::bip44::RawKey;
    ///
    /// let key = "xpub661MyMwAqRbcG8Zah6TcX3QpP5yJApaXcyLK8CJcZkuYjczivsHxVL5qm9cw8BYLYehgFeddK5WrxhntpcvqJKTVg96dUVL9P7hZ7Kcvqvd";
    /// let raw_key = RawKey::decode(key).unwrap();
    ///
    /// assert!(raw_key.to_public_key().is_some());
    /// ```
    ///
    /// ## Failed conversion
    ///
    /// ```
    /// use blockswap::utils::bip44::RawKey;
    ///
    /// let key = "xprv9zkiHpWM7sSAmi9iU8dSNnQ5dVb4J54zFcd137js4yykpxHrzjTXHQThjGHkCVjPCYxKo5AZKon4KRAXC4ZsR4prRtGTBPqNivjDgFdSnCc";
    /// let raw_key = RawKey::decode(key).unwrap();
    ///
    /// assert!(raw_key.to_public_key().is_none());
    /// ```
    pub fn to_public_key(&self) -> Option<ExtendedPubKey> {
        if self.data.len() != 33 {
            return None;
        }

        if KeyType::from(&self.version) != Some(KeyType::Public) {
            return None;
        }

        let public_key = hdwallet::secp256k1::PublicKey::from_slice(&self.data).ok()?;

        Some(ExtendedPubKey {
            chain_code: self.chain_code.to_vec(),
            public_key,
        })
    }

    /// Convert to an ExtendedPrivKey
    ///
    /// # Example
    ///
    /// ## Successful conversion
    ///
    /// ```
    /// use blockswap::utils::bip44::RawKey;
    ///
    /// let key = "xprv9zkiHpWM7sSAmi9iU8dSNnQ5dVb4J54zFcd137js4yykpxHrzjTXHQThjGHkCVjPCYxKo5AZKon4KRAXC4ZsR4prRtGTBPqNivjDgFdSnCc";
    /// let raw_key = RawKey::decode(key).unwrap();
    ///
    /// assert!(raw_key.to_private_key().is_some());
    /// ```
    ///
    /// ## Failed conversion
    ///
    /// ```
    /// use blockswap::utils::bip44::RawKey;
    ///
    /// let key = "xpub661MyMwAqRbcG8Zah6TcX3QpP5yJApaXcyLK8CJcZkuYjczivsHxVL5qm9cw8BYLYehgFeddK5WrxhntpcvqJKTVg96dUVL9P7hZ7Kcvqvd";
    /// let raw_key = RawKey::decode(key).unwrap();
    ///
    /// assert!(raw_key.to_private_key().is_none());
    /// ```
    pub fn to_private_key(&self) -> Option<ExtendedPrivKey> {
        if self.data.len() != 33 {
            return None;
        }

        if KeyType::from(&self.version) != Some(KeyType::Private) {
            return None;
        }

        let private_key = hdwallet::secp256k1::SecretKey::from_slice(&self.data[1..]).ok()?;

        Some(ExtendedPrivKey {
            chain_code: self.chain_code.to_vec(),
            private_key,
        })
    }
}

#[cfg(test)]
/// Test values can be generated from: https://iancoleman.io/bip39/
mod test {
    use super::*;

    #[test]
    fn decodes_xpub_correctly() {
        let xpub = "xpub661MyMwAqRbcG8Zah6TcX3QpP5yJApaXcyLK8CJcZkuYjczivsHxVL5qm9cw8BYLYehgFeddK5WrxhntpcvqJKTVg96dUVL9P7hZ7Kcvqvd";
        let expected = RawKey {
            version: [4, 136, 178, 30], // 0x0488ADE4
            depth: [0],
            fingerprint: [0, 0, 0, 0],
            child_number: [0, 0, 0, 0],
            chain_code: [
                159, 139, 32, 243, 78, 206, 239, 110, 166, 13, 53, 219, 0, 68, 103, 99, 247, 220,
                118, 189, 96, 236, 140, 246, 253, 99, 220, 145, 36, 153, 203, 212,
            ],
            data: vec![
                3, 158, 220, 204, 224, 233, 63, 67, 106, 40, 57, 71, 65, 35, 120, 139, 51, 162,
                142, 215, 173, 124, 255, 195, 161, 48, 136, 159, 35, 35, 68, 173, 28,
            ],
        };
        let result = RawKey::decode(xpub).unwrap();

        assert_eq!(result, expected);
        assert!(result.to_public_key().is_some());
        assert!(result.to_private_key().is_none());
    }

    #[test]
    fn decodes_xpriv_correctly() {
        let xpriv = "xprv9zkiHpWM7sSAmi9iU8dSNnQ5dVb4J54zFcd137js4yykpxHrzjTXHQThjGHkCVjPCYxKo5AZKon4KRAXC4ZsR4prRtGTBPqNivjDgFdSnCc";
        let expected = RawKey {
            version: [4, 136, 173, 228], // 0x0488B21E
            depth: [4],
            fingerprint: [28, 134, 51, 82],
            child_number: [0, 0, 0, 0],
            chain_code: [
                201, 129, 248, 142, 103, 29, 178, 198, 49, 45, 7, 45, 4, 70, 58, 77, 64, 243, 202,
                138, 153, 138, 59, 174, 230, 246, 192, 223, 109, 156, 138, 110,
            ],
            data: vec![
                0, 79, 55, 3, 218, 225, 85, 1, 157, 33, 121, 93, 117, 55, 1, 53, 58, 38, 5, 112,
                32, 11, 123, 220, 215, 81, 55, 240, 28, 177, 250, 18, 191,
            ],
        };
        let result = RawKey::decode(xpriv).unwrap();

        assert_eq!(result, expected);
        assert!(result.to_public_key().is_none());
        assert!(result.to_private_key().is_some());
    }

    #[test]
    fn returns_error_if_invalid_key_provided() {
        assert_eq!(RawKey::decode("H@ker").unwrap_err(), INVALID_KEY);
    }
}
