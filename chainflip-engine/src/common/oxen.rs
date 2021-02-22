use super::{coins::CoinAmount, GenericCoinAmount};
use chainflip_common::types::coin::{Coin, CoinInfo};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Oxen coin amount
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub struct OxenAmount {
    atomic_amount: u128,
}

impl OxenAmount {
    /// Create from atomic amount
    pub fn from_atomic(n: u128) -> Self {
        OxenAmount { atomic_amount: n }
    }

    /// Create from decimal
    /// **For tests only**
    pub fn from_decimal_string(n: &str) -> Self {
        GenericCoinAmount::from_decimal_string(Coin::OXEN, n).into()
    }

    /// Get inner atomic representation
    pub fn to_atomic(&self) -> u128 {
        self.atomic_amount
    }

    /// Subtract checking for underflow
    pub fn checked_sub(&self, v: &Self) -> Option<Self> {
        let amount = self.to_atomic().checked_sub(v.to_atomic())?;
        Some(OxenAmount::from_atomic(amount))
    }

    /// Add atomic amounts w/o overflow
    pub fn saturating_add(&self, v: &Self) -> Self {
        let amount = self.to_atomic().saturating_add(v.to_atomic());
        OxenAmount::from_atomic(amount)
    }
}

impl fmt::Display for OxenAmount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} OXEN", self.to_string_pretty())
    }
}

impl CoinAmount for OxenAmount {
    fn to_atomic(&self) -> u128 {
        self.atomic_amount
    }

    fn coin_info(&self) -> CoinInfo {
        Coin::OXEN.get_info()
    }
}
