use crate::common::ethereum::Transaction;
use crate::vault::blockchain_connection::ethereum::EthereumClient;
use async_trait::async_trait;
use std::{collections::VecDeque, sync::Mutex};

/// An ethereum client for testing
pub struct TestEthereumClient {
    blocks: Mutex<VecDeque<Vec<Transaction>>>,
}

impl TestEthereumClient {
    /// Create a new test ethereum client
    pub fn new() -> Self {
        TestEthereumClient {
            blocks: Mutex::new(VecDeque::new()),
        }
    }

    /// Add a block to the client
    pub fn add_block(&self, transactions: Vec<Transaction>) {
        self.blocks.lock().unwrap().push_back(transactions)
    }
}

#[async_trait]
impl EthereumClient for TestEthereumClient {
    async fn get_latest_block_number(&self) -> u64 {
        0
    }

    async fn get_transactions(&self, block_number: u64) -> Option<Vec<Transaction>> {
        self.blocks.lock().unwrap().pop_front()
    }
}
