use crate::vault::blockchain_connection::btc::BitcoinClient;
use async_trait::async_trait;
use bitcoin::Network;
use bitcoin::Transaction;
use std::{collections::VecDeque, sync::Mutex};

/// An ethereum client for testing
pub struct TestBitcoinClient {
    blocks: Mutex<VecDeque<Vec<Transaction>>>,
}

impl TestBitcoinClient {
    /// Create a new test ethereum client
    pub fn new() -> Self {
        TestBitcoinClient {
            blocks: Mutex::new(VecDeque::new()),
        }
    }

    /// Add a block to the client
    pub fn add_block(&self, transactions: Vec<Transaction>) {
        self.blocks.lock().unwrap().push_back(transactions)
    }
}

#[async_trait]
impl BitcoinClient for TestBitcoinClient {
    async fn get_latest_block_number(&self) -> Result<u64, String> {
        Ok(0)
    }

    async fn get_transactions(&self, _block_number: u64) -> Option<Vec<Transaction>> {
        self.blocks.lock().unwrap().pop_front()
    }

    fn get_network_type(&self) -> Network {
        Network::Testnet
    }
}
