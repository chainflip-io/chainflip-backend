use super::{
    coins::{CoinAmount, CoinInfo},
    Coin,
};
use std::fmt::{self, Display};

/// Represents regular and integrated wallet addresses for Loki
#[derive(Debug)]
pub struct LokiWalletAddress {
    /// base58 (monero flavor) representation
    address: String,
}

impl std::str::FromStr for LokiWalletAddress {
    type Err = String;

    /// Construct from string, validating address length
    fn from_str(addr: &str) -> Result<Self, Self::Err> {
        match addr.len() {
            97 | 108 => Ok(LokiWalletAddress {
                address: addr.to_owned(),
            }),
            x @ _ => Err(format!("Invalid address length: {}", x)),
        }
    }
}

impl LokiWalletAddress {
    /// Get internal string representation
    pub fn to_str(&self) -> &str {
        &self.address
    }
}

/// Payment id that can be used to identify loki transactions
#[derive(Debug)]
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
        write!(f, "{}", &self.long_pid[0..16])
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

/// Loki coin amount
#[derive(Debug)]
pub struct LokiAmount {
    atomic_amount: u128,
}

impl LokiAmount {
    pub fn from_atomic(n: u128) -> Self {
        LokiAmount { atomic_amount: n }
    }

    pub fn from_decimal(n: f64) -> Self {
        let decimals = Coin::LOKI.get_info().decimals as i32;
        let atomic_amount = (n * 10f64.powi(decimals)).round() as u128;
        LokiAmount::from_atomic(atomic_amount)
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
