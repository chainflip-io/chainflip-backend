use serde::{Deserialize, Serialize};
use std::{
    fmt::{self, Display},
    str::FromStr,
};

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
    /// Whether this coin requires a return address
    /// (so it could be refunded in necessary)
    pub requires_return_address: bool,
}

/// Enum for supported coin types
#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
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

/// Generic coin amount interface
pub trait CoinAmount {
    /// Get the internal representation of the amount in atomic values
    fn to_atomic(&self) -> u128;

    /// Create an instance from atomic coin amount
    fn from_atomic(n: u128) -> Self;

    /// Get coin info for current coin type
    fn coin_info() -> CoinInfo;

    /// Default implementation for user facing representation of the amount
    fn to_string_pretty(&self) -> String {
        let atomic_amount = self.to_atomic();
        let decimals = LokiAmount::coin_info().decimals;

        let mut atomic_str = atomic_amount.to_string();

        // Add learding zeroes for fractional amounts:
        if (atomic_str.len() as u32) < decimals + 1 {
            let extra = decimals + 1 - (atomic_str.len() as u32);

            // This is very inefficient, but should be good enough for now
            for _ in 0..extra {
                atomic_str.insert(0, '0');
            }
        }

        let dot_pos = atomic_str.len() - decimals as usize;

        atomic_str.insert(dot_pos, '.');

        atomic_str
    }
}

/// Loki coin amount
#[derive(Debug)]
pub struct LokiAmount {
    atomic_amount: u128,
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

    fn from_atomic(n: u128) -> Self {
        LokiAmount { atomic_amount: n }
    }

    fn coin_info() -> CoinInfo {
        Coin::LOKI.get_info()
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

    #[test]
    fn test_coin_pretty_printing() {
        let amount = LokiAmount {
            atomic_amount: 100_000_000_000,
        };

        assert_eq!(&amount.to_string_pretty(), "100.000000000");

        let amount = LokiAmount {
            atomic_amount: 123_456_789_987,
        };

        assert_eq!(&amount.to_string_pretty(), "123.456789987");

        let amount = LokiAmount {
            atomic_amount: 23_456_789,
        };

        assert_eq!(&amount.to_string_pretty(), "0.023456789");
    }
}
