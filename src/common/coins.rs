use serde::{Deserialize, Serialize};
use std::{fmt::Display, str::FromStr};

/// Information about a coin
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CoinInfo {
    /// The full name of the coin
    pub name: &'static str,
    /// The coin symbol
    pub symbol: Coin,
    /// The amount of decimals the coin uses.
    pub decimals: u32,
    /// Wether this coin requires a return address
    /// (so it could be refunded in necessary)
    pub requires_return_address: bool,
}

/// The list of coins we support
#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq)]
pub enum Coin {
    /// Ethereum
    ETH,
    /// Loki
    LOKI,
}

impl Coin {
    /// Get all the coins
    pub const ALL: &'static [Coin] = &[Coin::ETH, Coin::LOKI]; // There might be a better way to dynamically generate this.

    /// Get information about this coin
    pub fn get_info(&self) -> CoinInfo {
        match self {
            Coin::LOKI => CoinInfo {
                name: "Loki Network",
                symbol: Coin::LOKI,
                decimals: 9,
                requires_return_address: true,
            },
            Coin::ETH => CoinInfo {
                name: "Ethereum",
                symbol: Coin::ETH,
                decimals: 18,
                requires_return_address: true,
            },
        }
    }
}

impl FromStr for Coin {
    type Err = &'static str;

    fn from_str(symbol: &str) -> Result<Self, Self::Err> {
        let uppercased = &symbol.trim().to_uppercase()[..];
        match uppercased {
            "LOKI" => Ok(Coin::LOKI),
            "ETH" => Ok(Coin::ETH),
            _ => Err("Invalid coin"),
        }
    }
}

impl Display for Coin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}
