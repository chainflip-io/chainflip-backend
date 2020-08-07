use crate::transactions::CoinTx;
use std::time::SystemTime;

pub mod coins;
use serde::{Deserialize, Serialize};

// Note: time is not reliable in a distributed environment,
// so it should probably be replaced by block_id when we
// go distributed
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Timestamp(SystemTime);

impl Timestamp {
    pub fn new(ts: SystemTime) -> Self {
        Timestamp { 0: ts }
    }

    pub fn now() -> Self {
        Timestamp {
            0: SystemTime::now(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct WalletAddress(String);

impl WalletAddress {
    pub fn new(address: &str) -> Self {
        WalletAddress {
            0: address.to_owned(),
        }
    }
}

#[derive(Debug)]
pub struct Block {
    pub txs: Vec<CoinTx>,
}
