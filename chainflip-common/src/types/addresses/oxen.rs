use super::Address;
use crate::{
    string::*,
    types::{Bytes, Network},
};
use std::{convert::TryInto, fmt::Display, str::FromStr};

/// Get the network type from a byte
fn get_network(byte: u8) -> Result<Network, &'static str> {
    // Oxen information from: https://docs.loki.network/Wallets/Addresses/MainAddress/
    match byte {
        // Oxen - main net (main, integrated, subaddress)
        114 | 115 | 116 => Ok(Network::Mainnet),
        // Oxen - test net (main, integrated, subaddress)
        156 | 157 | 158 => Ok(Network::Testnet),
        // We don't have enough information of other coins
        _ => Err("Failed to decode network type"),
    }
}

/// The payment id type for Oxen
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct OxenPaymentId(pub [u8; 8]);

impl OxenPaymentId {
    pub fn hex_encoded(&self) -> String {
        hex::encode(self.0)
    }

    pub fn bytes(&self) -> Bytes {
        self.0.to_vec()
    }
}

impl core::convert::TryFrom<String> for OxenPaymentId {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let mut hex_id = hex::decode(value.as_str()).map_err(|e| e.to_string())?;
        // take the first 8 bytes since the RPC can return 32 bytes due to
        // legacy payment ids being 32 bytes. This can be done safely as the
        // short payment id is always in the first 8 bytes.
        hex_id.truncate(8);
        let id: [u8; 8] = hex_id
            .try_into()
            .map_err(|_| "Could not convert hex id to [u8; 8]")?;

        Ok(OxenPaymentId(id))
    }
}

/// A structure for oxen address
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct OxenAddress {
    base_address: Bytes,
    payment_id: Option<OxenPaymentId>,
}

impl OxenAddress {
    /// Get the network type of the address
    pub fn network(&self) -> Network {
        get_network(self.base_address[0]).expect("Invalid oxen address")
    }

    /// Get the payment id of the address
    pub fn payment_id(&self) -> Option<&OxenPaymentId> {
        self.payment_id.as_ref()
    }

    /// Get the current oxen address with the given payment id
    ///
    /// # Example
    ///
    /// ```
    /// use chainflip_common::types::addresses::{OxenAddress, OxenPaymentId};
    /// use std::str::FromStr;
    ///
    /// let address = OxenAddress::from_str("L7fffztMU6PF6gmx2TtY8wTBsqU51Wy9B4RwvJzbE4bUK1zFtK99yNz2rXEAnHH53qf63NANZGdYXZdUpwUvo19RGxxHWxK").unwrap();
    /// let integrated_address = address.with_payment_id(Some(OxenPaymentId([66, 15, 162, 155, 45, 154, 73, 245])));
    /// assert_eq!(&integrated_address.to_string(), "LHNLgohr5MuF6gmx2TtY8wTBsqU51Wy9B4RwvJzbE4bUK1zFtK99yNz2rXEAnHH53qf63NANZGdYXZdUpwUvo19RQdP3TqmQTySUkKMhZf")
    /// ```
    pub fn with_payment_id(&self, payment_id: Option<OxenPaymentId>) -> Self {
        let mut other = self.clone();
        other.payment_id = payment_id;

        other
    }

    /// Get the string representation of an address
    fn string(&self) -> String {
        let mut buffer = self.base_address.clone();

        let network = self.network();
        let network_byte = if self.payment_id.is_some() {
            // Integrated address
            match network {
                Network::Mainnet => 115,
                Network::Testnet => 157,
            }
        } else {
            // Regular address
            match network {
                Network::Mainnet => 114,
                Network::Testnet => 156,
            }
        };

        buffer[0] = network_byte;
        if let Some(payment_id) = self.payment_id() {
            buffer.extend(payment_id.bytes());
        }

        base58_monero::encode_check(&buffer).expect("Failed to create oxen address")
    }
}

impl Address for OxenAddress {}

impl FromStr for OxenAddress {
    type Err = &'static str;

    /// Get an address from a string
    ///
    /// # Example
    ///
    /// ```
    /// use chainflip_common::types::{Network, addresses::{OxenAddress, OxenPaymentId}};
    /// use std::str::FromStr;
    ///
    /// let address = OxenAddress::from_str("LHNLgohr5MuF6gmx2TtY8wTBsqU51Wy9B4RwvJzbE4bUK1zFtK99yNz2rXEAnHH53qf63NANZGdYXZdUpwUvo19RQdP3TqmQTySUkKMhZf").unwrap();
    /// assert_eq!(address.network(), Network::Mainnet);
    /// assert_eq!(address.payment_id(), Some(&OxenPaymentId([66, 15, 162, 155, 45, 154, 73, 245])));
    /// ```
    fn from_str(string: &str) -> Result<Self, Self::Err> {
        if string.is_empty() {
            return Err("Oxen address is empty");
        }

        // We use base58_monero over bs58 because there seems to be an issue with validating checksums with the latter
        let decoded_address =
            base58_monero::decode_check(string).map_err(|_| "Failed to decode oxen address")?;

        if decoded_address.is_empty() {
            return Err("Oxen address is empty");
        }

        let network = get_network(decoded_address[0]).map_err(|_| "Invalid network byte")?;

        // Due to monero encoding, if a network byte value is > 128 (which it is for testnet)
        // It adds an extra byte after it, thus we get different address lengths
        let address_length = match network {
            Network::Testnet => 66,
            _ => 65,
        };

        let base_address = decoded_address[..address_length].to_vec();
        let payment_id = {
            if decoded_address.len() > address_length {
                let id: [u8; 8] = decoded_address[address_length..]
                    .try_into()
                    .map_err(|_| "Invalid payment id")?;
                Some(OxenPaymentId(id))
            } else {
                None
            }
        };

        Ok(OxenAddress {
            base_address,
            payment_id,
        })
    }
}

impl Display for OxenAddress {
    /// Get the string representation of an address
    ///
    /// # Example
    ///
    /// ```
    /// use chainflip_common::types::addresses::{OxenAddress, OxenPaymentId};
    /// use std::str::FromStr;
    ///
    /// let address = OxenAddress::from_str("L7fffztMU6PF6gmx2TtY8wTBsqU51Wy9B4RwvJzbE4bUK1zFtK99yNz2rXEAnHH53qf63NANZGdYXZdUpwUvo19RGxxHWxK").unwrap();
    /// let integrated_address = address.with_payment_id(Some(OxenPaymentId([66, 15, 162, 155, 45, 154, 73, 245])));
    /// assert_eq!(&integrated_address.to_string(), "LHNLgohr5MuF6gmx2TtY8wTBsqU51Wy9B4RwvJzbE4bUK1zFtK99yNz2rXEAnHH53qf63NANZGdYXZdUpwUvo19RQdP3TqmQTySUkKMhZf")
    /// ```
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.string())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    // 420fa29b2d9a49f5
    const PAYMENT_ID: OxenPaymentId = OxenPaymentId([66, 15, 162, 155, 45, 154, 73, 245]);

    // 5ea5bd6c91501a69
    const LONG_PAYMENT_ID: OxenPaymentId = OxenPaymentId([94, 165, 189, 108, 145, 80, 26, 105]);

    #[test]
    fn produces_same_address() {
        let address = "L7fffztMU6PF6gmx2TtY8wTBsqU51Wy9B4RwvJzbE4bUK1zFtK99yNz2rXEAnHH53qf63NANZGdYXZdUpwUvo19RGxxHWxK";
        let oxen_address = OxenAddress::from_str(address).unwrap();
        assert_eq!(&oxen_address.to_string(), address);

        let address = "T6SvvzhYyo2cUwiZBtLoTKGBSqoeGYKP12nsJx3ZsHNm7NhLDwYezTU3Ya9Cgb1UgW3gZTE5RG5ny4QKTUbHiXS8267AzhpZs";
        let oxen_address = OxenAddress::from_str(address).unwrap();
        assert_eq!(&oxen_address.to_string(), address);
    }

    #[test]
    fn integrated_addresses() {
        let address = "L7fffztMU6PF6gmx2TtY8wTBsqU51Wy9B4RwvJzbE4bUK1zFtK99yNz2rXEAnHH53qf63NANZGdYXZdUpwUvo19RGxxHWxK";
        let expected = "LHNLgohr5MuF6gmx2TtY8wTBsqU51Wy9B4RwvJzbE4bUK1zFtK99yNz2rXEAnHH53qf63NANZGdYXZdUpwUvo19RQdP3TqmQTySUkKMhZf";

        let address = OxenAddress::from_str(address).unwrap();
        let integrated_address = address.with_payment_id(Some(PAYMENT_ID));
        assert_eq!(&integrated_address.to_string(), expected);

        let address = "T6SvvzhYyo2cUwiZBtLoTKGBSqoeGYKP12nsJx3ZsHNm7NhLDwYezTU3Ya9Cgb1UgW3gZTE5RG5ny4QKTUbHiXS8267AzhpZs";
        let expected = "TG9bwoX3b4YcUwiZBtLoTKGBSqoeGYKP12nsJx3ZsHNm7NhLDwYezTU3Ya9Cgb1UgW3gZTE5RG5ny4QKTUbHiXS8NCR92bBy6zV1dq77maK4";

        let address = OxenAddress::from_str(address).unwrap();
        let integrated_address = address.with_payment_id(Some(PAYMENT_ID));
        assert_eq!(&integrated_address.to_string(), expected);
    }

    #[test]
    fn parses_oxen_address() {
        let main_net_address = OxenAddress::from_str("L7fffztMU6PF6gmx2TtY8wTBsqU51Wy9B4RwvJzbE4bUK1zFtK99yNz2rXEAnHH53qf63NANZGdYXZdUpwUvo19RGxxHWxK").unwrap();
        assert_eq!(main_net_address.network(), Network::Mainnet);
        assert_eq!(main_net_address.payment_id(), None);

        let main_net_integrated_address = OxenAddress::from_str("LHNLgohr5MuF6gmx2TtY8wTBsqU51Wy9B4RwvJzbE4bUK1zFtK99yNz2rXEAnHH53qf63NANZGdYXZdUpwUvo19RQdP3TqmQTySUkKMhZf").unwrap();
        assert_eq!(main_net_integrated_address.network(), Network::Mainnet);
        assert_eq!(main_net_integrated_address.payment_id(), Some(&PAYMENT_ID));

        let test_net_address = OxenAddress::from_str("T6SvvzhYyo2cUwiZBtLoTKGBSqoeGYKP12nsJx3ZsHNm7NhLDwYezTU3Ya9Cgb1UgW3gZTE5RG5ny4QKTUbHiXS8267AzhpZs").unwrap();
        assert_eq!(test_net_address.network(), Network::Testnet);
        assert_eq!(test_net_address.payment_id(), None);

        let test_net_integrated_address = OxenAddress::from_str("TG9bwoX3b4YcUwiZBtLoTKGBSqoeGYKP12nsJx3ZsHNm7NhLDwYezTU3Ya9Cgb1UgW3gZTE5RG5ny4QKTUbHiXS8NCR92bBy6zV1dq77maK4").unwrap();
        assert_eq!(test_net_integrated_address.network(), Network::Testnet);
        assert_eq!(test_net_integrated_address.payment_id(), Some(&PAYMENT_ID));
    }

    #[test]
    fn throws_error_on_invalid_addresses() {
        assert_eq!(
            OxenAddress::from_str("").unwrap_err(),
            "Oxen address is empty"
        );

        assert_eq!(
            OxenAddress::from_str("fake_address").unwrap_err(),
            "Failed to decode oxen address"
        );

        assert_eq!(
            OxenAddress::from_str("4LL9oSLmtpccfufTMvppY6JwXNouMBzSkbLYfpAV5Usx3skxNgYeYTRj5UzqtReoS44qo9mtmXCqY45DJ852K5Jv2bYXZKKQePHES9khPK").unwrap_err(),
            "Invalid network byte"
        );
    }

    #[test]
    fn try_from_long_payment_id() {
        let long_payment_id: String =
            "5ea5bd6c91501a69000000000000000000000000000000000000000000000000".to_string();

        let oxen_payment_id: Result<OxenPaymentId, _> = long_payment_id.try_into();

        assert!(oxen_payment_id.is_ok());
        assert_eq!(oxen_payment_id.unwrap(), LONG_PAYMENT_ID);
    }
}
