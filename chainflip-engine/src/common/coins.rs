use crate::common::OxenAmount;
use chainflip_common::types::coin::{Coin, CoinInfo};
use serde::{Deserialize, Serialize};
use std::{convert::TryFrom, fmt::Display};

/// A representation of a valid pool coin
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub struct PoolCoin(Coin);

impl PoolCoin {
    /// Construct a PoolCoin from a Coin
    pub fn from(coin: Coin) -> Result<Self, &'static str> {
        if coin == Coin::BASE_COIN {
            Err("Cannot have a BASE coin pool")
        } else {
            Ok(PoolCoin(coin))
        }
    }

    /// Get the coin associated with this pool coin
    pub fn get_coin(&self) -> Coin {
        self.0
    }
}

// Shortcuts
impl PoolCoin {
    /// Ethereum
    pub const ETH: PoolCoin = PoolCoin(Coin::ETH);
    /// Bitcoin
    pub const BTC: PoolCoin = PoolCoin(Coin::BTC);
}

impl TryFrom<Coin> for PoolCoin {
    type Error = &'static str;

    fn try_from(coin: Coin) -> Result<Self, Self::Error> {
        PoolCoin::from(coin)
    }
}

impl From<PoolCoin> for Coin {
    fn from(coin: PoolCoin) -> Self {
        coin.get_coin()
    }
}

impl Display for PoolCoin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.get_coin())
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
        let one_unit = 10u128.pow(info.decimals);
        atomic_amount / one_unit as f64
    }

    /// Get coin info for current coin type
    fn coin_info(&self) -> CoinInfo;

    /// Default implementation for user facing representation of the amount
    fn to_string_pretty(&self) -> String {
        let atomic_amount = self.to_atomic();
        let decimals = self.coin_info().decimals;

        let mut atomic_str = atomic_amount.to_string();

        // Add leading zeroes for fractional amounts:
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

    /// Create a coin amount from a decimal value.
    /// **For tests only**
    pub fn from_decimal_string(coin: Coin, decimal_string: &str) -> Self {
        let info = coin.get_info();
        let decimals = info.decimals;

        let decimal_string: Vec<&str> = decimal_string.split(".").collect();
        let integer = decimal_string.get(0).unwrap();

        let mut fraction = decimal_string.get(1).cloned().unwrap_or("0").to_string();
        fraction.truncate(decimals as usize);

        // Add leading zeroes for fractional amounts:
        if (fraction.len() as u32) < decimals {
            let extra = decimals - (fraction.len() as u32);

            // This is very inefficient, but should be good enough for now
            for _ in 0..extra {
                fraction.push('0');
            }
        }

        let string_amount = format!("{}{}", integer, fraction);

        let atomic_amount = string_amount
            .parse()
            .expect("Failed to convert decimal to atomic");

        GenericCoinAmount {
            coin,
            atomic_amount,
        }
    }

    /// Get coin type
    pub fn coin_type(&self) -> Coin {
        self.coin
    }

    /// Get the underlying atomic amount
    pub fn to_atomic(&self) -> u128 {
        self.atomic_amount
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

impl From<OxenAmount> for GenericCoinAmount {
    fn from(tx: OxenAmount) -> Self {
        GenericCoinAmount::from_atomic(Coin::OXEN, tx.to_atomic())
    }
}

impl From<GenericCoinAmount> for OxenAmount {
    fn from(tx: GenericCoinAmount) -> Self {
        if tx.coin != Coin::OXEN {
            panic!("Cannot convert non-oxen amount");
        }

        OxenAmount::from_atomic(tx.atomic_amount)
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

    #[test]
    fn test_generic_coin_from_decimal_string() {
        let coin = GenericCoinAmount::from_decimal_string(Coin::ETH, "12500");
        assert_eq!(coin.atomic_amount, 12500000000000000000000);

        let coin = GenericCoinAmount::from_decimal_string(Coin::ETH, "3199.36");
        assert_eq!(coin.atomic_amount, 3199360000000000000000);

        let coin = GenericCoinAmount::from_decimal_string(Coin::ETH, "12500.5");
        assert_eq!(coin.atomic_amount, 12500500000000000000000);

        let coin = GenericCoinAmount::from_decimal_string(Coin::OXEN, "12500.512345678123");
        assert_eq!(coin.atomic_amount, 12500512345678);
    }
}
