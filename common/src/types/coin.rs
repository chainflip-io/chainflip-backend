use codec::{Decode, Encode};
use serde::{Deserialize, Serialize};
use std::{fmt, str};

// TODO: Should we make a macro for this which automatically generates coin info and from str implementations?

/// Enum for supported coin types
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Ord, PartialOrd, Encode, Decode, Serialize)]
pub enum Coin {
    /// Bitcoin
    BTC,
    /// Ethereum
    ETH,
    /// Oxen
    OXEN,
}

impl<'de> Deserialize<'de> for Coin {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{self, Unexpected, Visitor};
        use std::str::FromStr;

        struct CoinVisitor;

        impl<'de> Visitor<'de> for CoinVisitor {
            type Value = Coin;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                write!(formatter, "a coin as a string")
            }

            fn visit_str<E>(self, s: &str) -> Result<Coin, E>
            where
                E: de::Error,
            {
                Coin::from_str(s).map_err(|_| de::Error::invalid_value(Unexpected::Str(s), &self))
            }
        }

        deserializer.deserialize_str(CoinVisitor)
    }
}

impl str::FromStr for Coin {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "OXEN" | "Oxen" | "oxen" => Ok(Coin::OXEN),
            "BTC" | "btc" => Ok(Coin::BTC),
            "ETH" | "eth" => Ok(Coin::ETH),
            _ => Err("Failed to parse coin"),
        }
    }
}

impl fmt::Display for Coin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl Coin {
    /// The base coin of the system
    pub const BASE_COIN: Coin = Coin::OXEN;

    /// The list of supported coins
    pub const SUPPORTED: &'static [Coin] = &[Coin::ETH, Coin::OXEN, Coin::BTC];

    /// Check if this coin is supported
    pub fn is_supported(&self) -> bool {
        Self::SUPPORTED.contains(self)
    }

    /// Get information about this coin
    pub fn get_info(&self) -> CoinInfo {
        match self {
            Coin::OXEN => CoinInfo {
                name: "Oxen",
                symbol: Coin::OXEN,
                decimals: 9,
                requires_return_address: true,
            },
            Coin::ETH => CoinInfo {
                name: "Ethereum",
                symbol: Coin::ETH,
                decimals: 18,
                requires_return_address: false,
            },
            Coin::BTC => CoinInfo {
                name: "Bitcoin",
                symbol: Coin::BTC,
                decimals: 8,
                requires_return_address: false,
            },
        }
    }
}

/// Information about a coin
#[derive(Debug, Copy, Clone, Serialize)]
pub struct CoinInfo {
    /// The full name of the coin
    pub name: &'static str,
    /// The coin symbol
    pub symbol: Coin,
    /// The amount of decimals the coin uses.
    pub decimals: u32,
    /// Whether this coin requires a return address
    /// (so it could be refunded in necessary)
    pub requires_return_address: bool,
}
