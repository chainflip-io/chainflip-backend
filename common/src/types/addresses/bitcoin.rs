use super::Address;
use crate::{string::*, types::Network};
use std::{fmt::Display, str::FromStr, vec::Vec};

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum BitcoinAddressType {
    /// pay-to-pubkey-hash
    P2pkh,
    /// pay-to-script-hash
    P2sh,
    /// pay-to-witness-pubkey-hash
    P2wpkh,
    /// pay-to-witness-script-hash
    P2wsh,
}

/// Bitcoin Address which holds the compressed ECDSA public key
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct BitcoinAddress {
    address: String,
    pub address_type: Option<BitcoinAddressType>,
    pub network: Network,
}

impl Address for BitcoinAddress {}

/// Extract the bech32 prefix.
/// Returns the same slice when no prefix is found.
fn find_bech32_prefix(bech32: &str) -> &str {
    // Split at the last occurrence of the separator character '1'.
    match bech32.rfind('1') {
        None => bech32,
        Some(sep) => bech32.split_at(sep).0,
    }
}

impl FromStr for BitcoinAddress {
    type Err = &'static str;

    /// Get an address from a string
    ///
    /// # Example
    ///
    /// ```
    /// use chainflip_common::types::{Network, addresses::{BitcoinAddressType, BitcoinAddress}};
    /// use std::str::FromStr;
    ///
    /// let address = BitcoinAddress::from_str("bc1q2a2wtjjslcx2pm79rsk8fsd7ztj3xy88njccxn").unwrap();
    /// assert_eq!(address.network, Network::Mainnet);
    /// assert_eq!(address.address_type, Some(BitcoinAddressType::P2wpkh));
    ///
    /// assert!(BitcoinAddress::from_str("invalid").is_err());
    /// ```
    #[deprecated = "BROKEN: Could mistake a bs58 encoded address as bech32 encoded and fail, if the
        bs58 encoding result begins with bc1,BC1,tb1, or TB1. I didn't remove this code as that would cause
        large parts of common to fail to compile. If needed use an external library to do this stuff."
    ]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        panic!(); // See deprecated

        let bech32_network = match find_bech32_prefix(s) {
            // note that upper or lowercase is allowed but NOT mixed case
            "bc" | "BC" => Some(Network::Mainnet),
            "tb" | "TB" => Some(Network::Testnet),
            _ => None,
        };

        if let Some(network) = bech32_network {
            // decode as bech32
            let (_, payload) = bech32::decode(s).map_err(|_| "Failed to decode address")?;
            if payload.is_empty() {
                return Err("Empty payload");
            }

            // Get the script version and program (converted from 5-bit to 8-bit)
            let (version, program): (bech32::u5, Vec<u8>) = {
                let (v, p5) = payload.split_at(1);
                (
                    v[0],
                    bech32::FromBase32::from_base32(p5).map_err(|_| "Failed to decode program")?,
                )
            };

            // Generic segwit checks.
            if version.to_u8() > 16 {
                return Err("Invalid witness version");
            }
            if program.len() < 2 || program.len() > 40 {
                return Err("Invalid witness program length");
            }

            // Specific segwit v0 check.
            if version.to_u8() == 0 && (program.len() != 20 && program.len() != 32) {
                return Err("Invalid Segwit v0 program length");
            }

            let address_type = match version.to_u8() {
                0 => match program.len() {
                    20 => Some(BitcoinAddressType::P2wpkh),
                    32 => Some(BitcoinAddressType::P2wsh),
                    _ => None,
                },
                _ => None,
            };

            return Ok(BitcoinAddress {
                address: s.to_string(),
                address_type,
                network,
            });
        }

        // Base58
        let data = bs58::decode(s)
            .with_check(None)
            .into_vec()
            .map_err(|_| "Failed to decode address")?;

        if data.len() != 21 {
            return Err("Invalid address length");
        }

        let (network, address_type) = match data[0] {
            0 => (Network::Mainnet, BitcoinAddressType::P2pkh),
            5 => (Network::Mainnet, BitcoinAddressType::P2sh),
            111 => (Network::Testnet, BitcoinAddressType::P2pkh),
            196 => (Network::Testnet, BitcoinAddressType::P2sh),
            _ => return Err("Invalid version"),
        };

        Ok(BitcoinAddress {
            address: s.to_string(),
            address_type: Some(address_type),
            network,
        })
    }
}

impl Display for BitcoinAddress {
    /// Get the string representation of an address
    ///
    /// # Example
    ///
    /// ```
    /// use chainflip_common::types::addresses::BitcoinAddress;
    /// use std::str::FromStr;
    ///
    /// let address = BitcoinAddress::from_str("bc1q2a2wtjjslcx2pm79rsk8fsd7ztj3xy88njccxn").unwrap();
    ///
    /// assert_eq!(&address.to_string(), "bc1q2a2wtjjslcx2pm79rsk8fsd7ztj3xy88njccxn");
    /// ```
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.address)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn validates_addresses() {
        let valid_vectors = [
            (
                "BC1QW508D6QEJXTDG4Y5R3ZARVARY0C5XW7KV8F3T4",
                Network::Mainnet,
                Some(BitcoinAddressType::P2wpkh),
            ),
            (
                "bc1q0z237qgru6qx5h9qc6vf6gpd09kna6yqgx5ld7",
                Network::Mainnet,
                Some(BitcoinAddressType::P2wpkh),
            ),
            (
                "1BvBMSEYstWetqTFn5Au4m4GFg7xJaNVN2",
                Network::Mainnet,
                Some(BitcoinAddressType::P2pkh),
            ),
            (
                "3Fug8fr5JUQgNnE7JjonCZ9PsoCjqRVfE7",
                Network::Mainnet,
                Some(BitcoinAddressType::P2sh),
            ),
            (
                "tb1qrp33g0q5c5txsp9arysrx4k6zdkfs4nce4xj0gdcccefvpysxf3q0sl5k7",
                Network::Testnet,
                Some(BitcoinAddressType::P2wsh),
            ),
            (
                "tb1qqqqqp399et2xygdj5xreqhjjvcmzhxw4aywxecjdzew6hylgvsesrxh6hy",
                Network::Testnet,
                Some(BitcoinAddressType::P2wsh),
            ),
            (
                "mwSjz5AnSVu27U5tzmsLJ37R7CxykpeXNc",
                Network::Testnet,
                Some(BitcoinAddressType::P2pkh),
            ),
            (
                "bc1pw508d6qejxtdg4y5r3zarvary0c5xw7kw508d6qejxtdg4y5r3zarvary0c5xw7k7grplx",
                Network::Mainnet,
                None,
            ),
            (
                "bc1zw508d6qejxtdg4y5r3zarvaryvg6kdaj",
                Network::Mainnet,
                None,
            ),
        ];

        for vector in &valid_vectors {
            let addr = BitcoinAddress::from_str(vector.0).unwrap();
            assert_eq!(addr.network, vector.1);
            assert_eq!(addr.address_type, vector.2);
        }

        let invalid_vectors = [
            "tc1qw508d6qejxtdg4y5r3zarvary0c5xw7kg3g4ty",
            "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t5",
            "BC13W508D6QEJXTDG4Y5R3ZARVARY0C5XW7KN40WF2",
            "bc1rw5uspcuh",
            "bc10w508d6qejxtdg4y5r3zarvary0c5xw7kw508d6qejxtdg4y5r3zarvary0c5xw7kw5rljs90",
            "BC1QR508D6QEJXTDG4Y5R3ZARVARYV98GJ9P",
            "tb1qrp33g0q5c5txsp9arysrx4k6zdkfs4nce4xj0gdcccefvpysxf3q0sL5k7",
            "bc1zw508d6qejxtdg4y5r3zarvaryvqyzf3du",
            "tb1qrp33g0q5c5txsp9arysrx4k6zdkfs4nce4xj0gdcccefvpysxf3pjxtptv",
            "bc1gmk9yu",
        ];
        for vector in &invalid_vectors {
            assert!(BitcoinAddress::from_str(vector).is_err());
        }
    }
}
