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

/// Enum for supported coin types
#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq)]
pub enum Coin {
    /// Bitcoin
    BTC,
    /// Ethereum
    ETH,
    /// Loki
    LOKI,
}

/// Invalid coin literal error
pub const COIN_PARSING_ERROR: &'static str = "failed to parse coin";

impl FromStr for Coin {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "LOKI" | "Loki" | "loki" => Ok(Coin::LOKI),
            "BTC" | "btc" => Ok(Coin::BTC),
            "ETH" | "eth" => Ok(Coin::ETH),
            _ => Err(COIN_PARSING_ERROR),
        }
    }
}

impl Coin {
    /// Get all the coins
    pub const ALL: &'static [Coin] = &[Coin::ETH, Coin::LOKI, Coin::BTC]; // There might be a better way to dynamically generate this.

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

impl Display for Coin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn valid_coin_literal_parsing() {
        assert!(Coin::from_str("ETH").is_ok());
        assert!(Coin::from_str("eth").is_ok());

        assert!(Coin::from_str("BTC").is_ok());
        assert!(Coin::from_str("btc").is_ok());

        assert!(Coin::from_str("LOKI").is_ok());
        assert!(Coin::from_str("loki").is_ok());
    }
}
