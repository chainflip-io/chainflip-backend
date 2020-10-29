use crate::utils::{clone_into_array, loki::address::get_integrated_address};

use super::{
    coins::{CoinAmount, CoinInfo},
    Coin, GenericCoinAmount, WalletAddress,
};

use serde::{Deserialize, Serialize};

use std::{
    convert::TryFrom,
    fmt::{self, Display},
};

/// Represents regular and integrated wallet addresses for Loki
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct LokiWalletAddress {
    /// base58 (monero flavor) representation
    pub address: String,
}

impl TryFrom<WalletAddress> for LokiWalletAddress {
    type Error = String;
    fn try_from(a: WalletAddress) -> Result<Self, Self::Error> {
        LokiWalletAddress::from_string(a.0)
    }
}

impl From<LokiWalletAddress> for WalletAddress {
    fn from(a: LokiWalletAddress) -> Self {
        WalletAddress::new(&a.address)
    }
}

impl std::str::FromStr for LokiWalletAddress {
    type Err = String;

    /// Construct from string, validating address length
    fn from_str(addr: &str) -> Result<Self, Self::Err> {
        LokiWalletAddress::from_string(addr.to_owned())
    }
}

impl LokiWalletAddress {
    /// Get internal string representation
    pub fn to_str(&self) -> &str {
        &self.address
    }

    /// Helper function to convert from an owned string
    fn from_string(addr: String) -> Result<LokiWalletAddress, String> {
        match addr.len() {
            97 | 108 => Ok(LokiWalletAddress {
                address: addr.to_owned(),
            }),
            x @ _ => Err(format!("Invalid address length: {}", x)),
        }
    }
}

/// Payment id that can be used to identify loki transactions
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LokiPaymentId {
    // String representation of payment id with trailing zeros added at construction time.
    // We might consider using a constant size array/string on the stack for this
    long_pid: String,
}

impl LokiPaymentId {
    /// Get payment id as a string slice
    pub fn to_str(&self) -> &str {
        &self.long_pid
    }

    /// Get the short payment id
    pub fn short(&self) -> &str {
        // A short payment id consists of 16 characters
        &self.long_pid[0..16]
    }

    /// Get the byte value of the payment id
    pub fn to_bytes(&self) -> [u8; 8] {
        let decoded = hex::decode(self.to_str()).unwrap();
        clone_into_array(&decoded[..8])
    }

    /// get the integrated address from this payment id
    pub fn get_integrated_address(&self, base_address: &str) -> Result<LokiWalletAddress, String> {
        let address = get_integrated_address(base_address, &self.to_bytes())
            .map_err(|err| err.to_string())?;
        LokiWalletAddress::from_string(address)
    }
}

impl std::str::FromStr for LokiPaymentId {
    type Err = String;

    /// Construct loki payment id from a short (long) 16 (64) hex character string
    fn from_str(pid: &str) -> Result<Self, String> {
        match pid.len() {
            16 => {
                // There is a bug in the wallet that requires trailing zero. Apparently
                // there is a fix for that on some branch, but for now let's just
                // defencively add zeros on our side
                let long_pid = format!("{}000000000000000000000000000000000000000000000000", pid);

                Ok(LokiPaymentId { long_pid })
            }
            64 => Ok(LokiPaymentId {
                long_pid: pid.to_owned(),
            }),
            x @ _ => Err(format!("Incorect size for short payment id: {}", x)),
        }
    }
}

impl std::fmt::Display for LokiPaymentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", &self.short())
    }
}

/// Serialize LokiPaymentId as a simple string
impl serde::Serialize for LokiPaymentId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.long_pid)
    }
}

impl<'de> Deserialize<'de> for LokiPaymentId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{self, Unexpected, Visitor};
        use std::str::FromStr;

        struct PIDVisitor;

        impl<'de> Visitor<'de> for PIDVisitor {
            type Value = LokiPaymentId;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                write!(
                    formatter,
                    "Expecting a short or long payment id as a string"
                )
            }

            fn visit_str<E>(self, s: &str) -> Result<LokiPaymentId, E>
            where
                E: de::Error,
            {
                LokiPaymentId::from_str(s)
                    .map_err(|_| de::Error::invalid_value(Unexpected::Str(s), &self))
            }
        }

        deserializer.deserialize_str(PIDVisitor)
    }
}

/// Loki coin amount
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub struct LokiAmount {
    atomic_amount: u128,
}

impl LokiAmount {
    /// Create from atomic amount
    pub fn from_atomic(n: u128) -> Self {
        LokiAmount { atomic_amount: n }
    }

    /// Create from decimal
    /// **For tests only**
    pub fn from_decimal_string(n: &str) -> Self {
        GenericCoinAmount::from_decimal_string(Coin::LOKI, n).into()
    }

    /// Get inner atomic representation
    pub fn to_atomic(&self) -> u128 {
        self.atomic_amount
    }

    /// Subtract checking for underflow
    pub fn checked_sub(&self, v: &Self) -> Option<Self> {
        let amount = self.to_atomic().checked_sub(v.to_atomic())?;
        Some(LokiAmount::from_atomic(amount))
    }

    /// Add atomic amounts w/o overflow
    pub fn saturating_add(&self, v: &Self) -> Self {
        let amount = self.to_atomic().saturating_add(v.to_atomic());
        LokiAmount::from_atomic(amount)
    }
}

impl Display for LokiAmount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} LOKI", self.to_string_pretty())
    }
}

impl CoinAmount for LokiAmount {
    fn to_atomic(&self) -> u128 {
        self.atomic_amount
    }

    fn coin_info(&self) -> CoinInfo {
        Coin::LOKI.get_info()
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use std::str::FromStr;

    #[test]
    fn payment_id_serialization_and_deserialization() {
        let pid = LokiPaymentId::from_str("60900e5603bf96e3").unwrap();

        let serialized = serde_json::to_string(&pid).expect("Payment id serialization");

        assert_eq!(
            &serialized,
            "\"60900e5603bf96e3000000000000000000000000000000000000000000000000\""
        );

        let deserialized: LokiPaymentId =
            serde_json::from_str(&serialized).expect("Payment id deserialization");

        assert_eq!(deserialized, pid);
    }

    #[test]
    fn payment_id_converts_to_bytes() {
        let pid = LokiPaymentId::from_str("420fa29b2d9a49f5").unwrap();
        assert_eq!(pid.to_bytes(), [66, 15, 162, 155, 45, 154, 73, 245])
    }

    #[test]
    fn payment_id_generates_integrated_address() {
        let address = "T6SvvzhYyo2cUwiZBtLoTKGBSqoeGYKP12nsJx3ZsHNm7NhLDwYezTU3Ya9Cgb1UgW3gZTE5RG5ny4QKTUbHiXS8267AzhpZs";
        let expected = "TG9bwoX3b4YcUwiZBtLoTKGBSqoeGYKP12nsJx3ZsHNm7NhLDwYezTU3Ya9Cgb1UgW3gZTE5RG5ny4QKTUbHiXS8NCR92bBy6zV1dq77maK4";

        let pid = LokiPaymentId::from_str("420fa29b2d9a49f5").unwrap();
        assert_eq!(
            &pid.get_integrated_address(address).unwrap().address,
            expected
        );
    }
}
