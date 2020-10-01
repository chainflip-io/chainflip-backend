use crate::vault::blockchain_connection::btc::*;
use async_trait::async_trait;
use bitcoin::Network;
use bitcoin::Transaction;
use bitcoin::Txid;
use std::{collections::VecDeque, sync::Mutex};

/// An ethereum client for testing
pub struct TestBitcoinClient {
    blocks: Mutex<VecDeque<Vec<Transaction>>>,
    send_handler:
        Option<Box<dyn Fn(&SendTransaction) -> Result<Txid, String> + Send + Sync + 'static>>,
}

impl TestBitcoinClient {
    /// Create a new test ethereum client
    pub fn new() -> Self {
        TestBitcoinClient {
            blocks: Mutex::new(VecDeque::new()),
            send_handler: None,
        }
    }

    /// Add a block to the client
    pub fn add_block(&self, transactions: Vec<Transaction>) {
        self.blocks.lock().unwrap().push_back(transactions)
    }

    /// Set the handler for send
    pub fn set_send_handler<F>(&mut self, function: F)
    where
        F: 'static,
        F: Fn(&SendTransaction) -> Result<Txid, String> + Send + Sync,
    {
        self.send_handler = Some(Box::new(function));
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

    async fn send(&self, tx: &SendTransaction) -> Result<Txid, String> {
        if let Some(function) = &self.send_handler {
            return function(tx);
        }
        Err("Not handled".to_owned())
    }
}
