use super::Address;
use crate::string::*;
use std::{convert::TryInto, fmt::Display, str::FromStr, vec::Vec};
use tiny_keccak::{Hasher, Keccak};

/// A structure for ethereum address
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct EthereumAddress(pub [u8; 20]);

impl EthereumAddress {
    /// Generate an ethereum address from an ECDSA public key
    ///
    /// # Example
    ///
    /// ```
    /// use chainflip_common::types::addresses::EthereumAddress;
    /// use std::convert::TryInto;
    /// use hex;
    ///
    /// // Uncompressed ECDSA public key (65 bytes)
    /// let uncompressed_public_key = hex::decode("044ac1bb1bc5fd7a9b173f6a136a40e4be64841c77d7f66ead444e101e0134812725d687034bed64cf7e6998d8558aa1930ae477a4e00b0c33033d7d651c8307eb").unwrap();
    /// let uncompressed_public_key: [u8; 65] = uncompressed_public_key.try_into().unwrap();
    /// let address = EthereumAddress::from_public_key(uncompressed_public_key);
    ///
    /// assert_eq!(&address.to_string(), "0x70E7Db0678460C5e53F1FFc9221d1C692111dCc5");
    /// ```
    pub fn from_public_key(bytes: [u8; 65]) -> Self {
        // apply a keccak_256 hash of the public key
        let mut result = [0u8; 32];
        let mut hasher = Keccak::v256();
        hasher.update(&bytes[1..]); // Strip the first byte to get 64 bytes
        hasher.finalize(&mut result);

        // The last 20 bytes in hex is the ethereum address
        let bytes: [u8; 20] = result[12..].try_into().expect("Expected to get 20 bytes");
        Self(bytes)
    }

    /// Generate an ethereum address from a CREATE2 function.
    /// See: https://eips.ethereum.org/EIPS/eip-1014
    ///
    /// # Example
    ///
    /// ```
    /// use chainflip_common::types::addresses::EthereumAddress;
    /// use std::{str::FromStr, convert::TryInto};
    /// use hex;
    ///
    /// let address = EthereumAddress::from_str("0x00000000000000000000000000000000deadbeef").unwrap();
    /// let salt = hex::decode("00000000000000000000000000000000000000000000000000000000cafebabe").unwrap();
    /// let salt: [u8; 32] = salt.try_into().unwrap();
    /// let init_code = hex::decode("deadbeef").unwrap();
    ///
    /// let new_address = EthereumAddress::create2(&address, salt, &init_code);
    /// assert_eq!(&new_address.to_string(), "0x60f3f640a8508fC6a86d45DF051962668E1e8AC7");
    /// ```
    pub fn create2(deployer: &EthereumAddress, salt: [u8; 32], contract_init_code: &[u8]) -> Self {
        // Create2 address = keccak_256(0xff + address bytes + salt bytes + contract init hash)[12:]

        // Hash the initt code
        let mut contract_init_hash = [0u8; 32];
        let mut hasher = Keccak::v256();
        hasher.update(contract_init_code);
        hasher.finalize(&mut contract_init_hash);

        // Hash the params
        let mut result = [0u8; 32];
        let mut hasher = Keccak::v256();
        hasher.update(&[0xff]);
        hasher.update(&deployer.0);
        hasher.update(&salt);
        hasher.update(&contract_init_hash);
        hasher.finalize(&mut result);

        // The last 20 bytes in hex is the create2 address
        let bytes: [u8; 20] = result[12..].try_into().expect("Expected to get 20 bytes");
        Self(bytes)
    }

    /// Get the checksummed string representation of an address
    fn checksummed(&self) -> String {
        // Checksum an address
        // Ref: https://github.com/ethereum/EIPs/blob/master/EIPS/eip-55.md
        let address = hex::encode(self.0).to_lowercase();
        let mut checksumed = "0x".to_string();

        // apply a keccak_256 hash of the address
        let mut result = [0u8; 32];
        let mut hasher = Keccak::v256();
        hasher.update(address.as_bytes());
        hasher.finalize(&mut result);

        let hash: Vec<char> = hex::encode(result).chars().collect();

        for (i, c) in address.chars().enumerate() {
            let val = match i32::from_str_radix(&hash[i].to_string(), 16) {
                Ok(val) => val,
                _ => 0,
            };

            if val > 7 {
                checksumed += &c.to_uppercase().to_string()
            } else {
                checksumed += &c.to_string()
            }
        }

        checksumed
    }
}

impl Address for EthereumAddress {}

impl FromStr for EthereumAddress {
    type Err = &'static str;

    /// Get an address from a string
    ///
    /// # Example
    ///
    /// ```
    /// use chainflip_common::types::addresses::EthereumAddress;
    /// use std::str::FromStr;
    ///
    /// assert!(EthereumAddress::from_str("0x70E7Db0678460C5e53F1FFc9221d1C692111dCc5").is_ok());
    /// assert!(EthereumAddress::from_str("Invalid address").is_err());
    /// ```
    fn from_str(string: &str) -> Result<Self, Self::Err> {
        if string.len() != 42 {
            return Err("Ethereum address must be 42 characters long");
        }

        let stripped = string.trim_start_matches("0x").to_lowercase();
        let bytes = hex::decode(stripped).map_err(|_| "Invalid hex format")?;
        let bytes: [u8; 20] = bytes
            .try_into()
            .map_err(|_| "Ethereum address must be 20 bytes long")?;

        Ok(Self(bytes))
    }
}

impl Display for EthereumAddress {
    /// Get the checksummed string representation of an address
    ///
    /// # Example
    ///
    /// ```
    /// use chainflip_common::types::addresses::EthereumAddress;
    /// use std::{convert::TryInto, str::FromStr};
    ///
    /// let uncompressed_public_key = hex::decode("044ac1bb1bc5fd7a9b173f6a136a40e4be64841c77d7f66ead444e101e0134812725d687034bed64cf7e6998d8558aa1930ae477a4e00b0c33033d7d651c8307eb").unwrap();
    /// let uncompressed_public_key: [u8; 65] = uncompressed_public_key.try_into().unwrap();
    /// let address = EthereumAddress::from_public_key(uncompressed_public_key);
    ///
    /// assert_eq!(&address.to_string(), "0x70E7Db0678460C5e53F1FFc9221d1C692111dCc5");
    /// ```
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.checksummed())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn parses_addresses() {
        let bytes = vec![
            112, 231, 219, 6, 120, 70, 12, 94, 83, 241, 255, 201, 34, 29, 28, 105, 33, 17, 220, 197,
        ];

        let checksummed =
            EthereumAddress::from_str("0x70E7Db0678460C5e53F1FFc9221d1C692111dCc5").unwrap();
        assert_eq!(checksummed.0.to_vec(), bytes);

        let normal =
            EthereumAddress::from_str("0x70e7db0678460c5e53f1ffc9221d1c692111dcc5").unwrap();
        assert_eq!(normal.0.to_vec(), bytes);
    }

    #[test]
    fn throws_error_on_invalid_addresses() {
        let address = EthereumAddress::from_str("70E7Db0678460C5e53F1FFc9221d1C692111dCc5");
        assert_eq!(
            address.unwrap_err(),
            "Ethereum address must be 42 characters long"
        );

        let address = EthereumAddress::from_str("8070E7Db0678460C5e53F1FFc9221d1C692111dCc5");
        assert_eq!(
            address.unwrap_err(),
            "Ethereum address must be 20 bytes long"
        );

        let address = EthereumAddress::from_str("0xHIJKDb0678460C5e53F1FFc9221d1C692111dCc5");
        assert_eq!(address.unwrap_err(), "Invalid hex format");
    }
}
