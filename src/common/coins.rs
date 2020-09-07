use crate::common::LokiAmount;
use serde::{Deserialize, Serialize};
use std::{fmt::Display, str::FromStr};

/// A representation of a valid pool coin
#[derive(Debug, Copy, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct PoolCoin(Coin);

impl PoolCoin {
    /// Shortcut for etherium variant of pool coin
    pub const ETH: PoolCoin = PoolCoin(Coin::ETH);

    /// Construct a PoolCoin from a Coin
    pub fn from(coin: Coin) -> Result<Self, &'static str> {
        if coin == Coin::LOKI {
            Err("Cannot have a LOKI coin pool")
        } else {
            Ok(PoolCoin(coin))
        }
    }

    /// Get the coin associated with this pool coin
    pub fn get_coin(&self) -> Coin {
        self.0
    }
}

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

impl CoinInfo {
    /// Get 1 unit of this coin in atomic value.
    ///
    /// This is the same as doing: `10^decimals`
    pub fn one_unit(&self) -> u128 {
        10u128.pow(self.decimals)
    }
}

/// Enum for supported coin types
#[derive(Debug, Serialize, Copy, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum Coin {
    /// Bitcoin
    BTC,
    /// Ethereum
    ETH,
    /// Loki
    LOKI,
}

impl<'de> Deserialize<'de> for Coin {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{self, Unexpected, Visitor};
        use std::fmt;

        struct PIDVisitor;

        impl<'de> Visitor<'de> for PIDVisitor {
            type Value = Coin;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                write!(formatter, "Expecting a coin as a string")
            }

            fn visit_str<E>(self, s: &str) -> Result<Coin, E>
            where
                E: de::Error,
            {
                Coin::from_str(s).map_err(|_| de::Error::invalid_value(Unexpected::Str(s), &self))
            }
        }

        deserializer.deserialize_str(PIDVisitor)
    }
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
    /// The list of supported coins
    pub const SUPPORTED: &'static [Coin] = &[Coin::ETH, Coin::LOKI];

    /// Check if this coin is supported
    pub fn is_supported(&self) -> bool {
        Self::SUPPORTED.contains(self)
    }

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

    /// Get the decimal representation of the amount
    fn to_decimal(&self) -> f64 {
        let atomic_amount = self.to_atomic() as f64;
        let info = self.coin_info();
        let decimals = info.decimals as i32;
        atomic_amount / info.one_unit() as f64
    }

    /// Get coin info for current coin type
    fn coin_info(&self) -> CoinInfo;

    /// Default implementation for user facing representation of the amount
    fn to_string_pretty(&self) -> String {
        let atomic_amount = self.to_atomic();
        let decimals = self.coin_info().decimals;

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

/// A generic coin amount
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenericCoinAmount {
    coin: Coin,
    atomic_amount: u128,
}

impl GenericCoinAmount {
    /// Create a coin amount from atomic value
    pub fn from_atomic(coin: Coin, atomic_amount: u128) -> Self {
        GenericCoinAmount {
            coin,
            atomic_amount,
        }
    }

    /// Create a coin amount from a decimal value
    pub fn from_decimal(coin: Coin, decimal_amount: f64) -> Self {
        let info = coin.get_info();
        let decimals = info.decimals as i32;
        let atomic_amount = (decimal_amount * info.one_unit() as f64).round() as u128;
        GenericCoinAmount {
            coin,
            atomic_amount,
        }
    }

    /// Get coin type
    pub fn coin_type(&self) -> Coin {
        self.coin
    }
}

impl CoinAmount for GenericCoinAmount {
    fn to_atomic(&self) -> u128 {
        self.atomic_amount
    }

    fn coin_info(&self) -> CoinInfo {
        self.coin.get_info()
    }
}

impl From<LokiAmount> for GenericCoinAmount {
    fn from(tx: LokiAmount) -> Self {
        GenericCoinAmount::from_atomic(Coin::LOKI, tx.to_atomic())
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    struct TestAmount(u128);

    impl CoinAmount for TestAmount {
        fn to_atomic(&self) -> u128 {
            self.0
        }

        fn coin_info(&self) -> CoinInfo {
            CoinInfo {
                name: "TEST",
                symbol: Coin::ETH,
                decimals: 18,
                requires_return_address: false,
            }
        }
    }

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
        let amount = TestAmount(100_000_000_000_000_000_000);

        assert_eq!(&amount.to_string_pretty(), "100.000000000000000000");

        let amount = TestAmount(123_456_789_987_000_000_000);

        assert_eq!(&amount.to_string_pretty(), "123.456789987000000000");

        let amount = TestAmount(23_456_789_000_000_000);

        assert_eq!(&amount.to_string_pretty(), "0.023456789000000000");
    }

    #[test]
    fn test_coin_to_decimal() {
        let amount = TestAmount(105_403_140_000_000_000);

        assert_eq!(amount.to_decimal(), 0.10540314);
    }
}
