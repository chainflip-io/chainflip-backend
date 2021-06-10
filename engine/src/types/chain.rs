use codec::{Decode, Encode};
use serde::{Deserialize, Serialize};
use std::{fmt, str};

/// Enum for supported chain types
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Ord, PartialOrd, Encode, Decode, Serialize)]
pub enum Chain {
    /// Bitcoin
    BTC,
    /// Polkadot
    DOT,
    /// Ethereum
    ETH,
    /// Oxen
    OXEN,
}

impl<'de> Deserialize<'de> for Chain {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{self, Unexpected, Visitor};
        use std::str::FromStr;

        struct ChainVisitor;

        impl<'de> Visitor<'de> for ChainVisitor {
            type Value = Chain;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                write!(formatter, "a chain as a string")
            }

            fn visit_str<E>(self, s: &str) -> Result<Chain, E>
            where
                E: de::Error,
            {
                Chain::from_str(s).map_err(|_| de::Error::invalid_value(Unexpected::Str(s), &self))
            }
        }

        deserializer.deserialize_str(ChainVisitor)
    }
}

impl str::FromStr for Chain {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "OXEN" | "Oxen" | "oxen" => Ok(Chain::OXEN),
            "BTC" | "btc" => Ok(Chain::BTC),
            "ETH" | "eth" => Ok(Chain::ETH),
            "DOT" | "dot" => Ok(Chain::DOT),
            _ => Err("Failed to parse chain"),
        }
    }
}

impl fmt::Display for Chain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}
