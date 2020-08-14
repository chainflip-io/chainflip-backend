use serde::{Deserialize, Serialize};
use strum_macros::{EnumIter, ToString};

use std::str::FromStr;

/// Coin-specific info
pub struct CoinInfo {
    /// Full name
    pub name: &'static str,
    /// Representation in enum
    pub symbol: Coin,
    /// Number of decimal places
    pub decimals: u32,
    /// Whether depositing this coin requires a return address
    /// (so it could be refunded in necessary)
    pub requires_return_address: bool,
}

/// Enum for supported coin types
#[derive(Debug, ToString, EnumIter, Serialize, Deserialize)]
pub enum Coin {
    /// Bitcoin
    BTC,
    /// Etherium
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
    /// Get coin info
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
            Coin::BTC => CoinInfo {
                name: "Bitcoin",
                symbol: Coin::BTC,
                decimals: 8,
                requires_return_address: true,
            },
        }
    }

    /// Get the number of decimal places for a coin
    pub fn get_decimals(&self) -> u32 {
        self.get_info().decimals
    }
}
