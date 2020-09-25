use crate::{
    common::ethereum::Hash,
    vault::blockchain_connection::ethereum::{EstimateResult, EthereumClient, SendTransaction},
};
use crate::{
    common::ethereum::Transaction, vault::blockchain_connection::ethereum::EstimateRequest,
};
use async_trait::async_trait;
use std::{collections::VecDeque, sync::Mutex};

/// An ethereum client for testing
pub struct TestEthereumClient {
    blocks: Mutex<VecDeque<Vec<Transaction>>>,
    estimated_fee_handler: Option<
        Box<dyn Fn(&EstimateRequest) -> Result<EstimateResult, String> + Send + Sync + 'static>,
    >,
    send_handler:
        Option<Box<dyn Fn(&SendTransaction) -> Result<Hash, String> + Send + Sync + 'static>>,
}

impl TestEthereumClient {
    /// Create a new test ethereum client
    pub fn new() -> Self {
        TestEthereumClient {
            blocks: Mutex::new(VecDeque::new()),
            estimated_fee_handler: None,
            send_handler: None,
        }
    }

    /// Add a block to the client
    pub fn add_block(&self, transactions: Vec<Transaction>) {
        self.blocks.lock().unwrap().push_back(transactions)
    }

    /// Set the handler for estimate fee
    pub fn set_get_estimate_fee_handler<F>(&mut self, function: F)
    where
        F: 'static,
        F: Fn(&EstimateRequest) -> Result<EstimateResult, String> + Send + Sync,
    {
        self.estimated_fee_handler = Some(Box::new(function));
    }

    /// Set the handler for send
    pub fn set_send_handler<F>(&mut self, function: F)
    where
        F: 'static,
        F: Fn(&SendTransaction) -> Result<Hash, String> + Send + Sync,
    {
        self.send_handler = Some(Box::new(function));
    }
}

#[async_trait]
impl EthereumClient for TestEthereumClient {
    async fn get_latest_block_number(&self) -> Result<u64, String> {
        Ok(0)
    }

    async fn get_transactions(&self, _block_number: u64) -> Option<Vec<Transaction>> {
        self.blocks.lock().unwrap().pop_front()
    }

    async fn get_estimated_fee(&self, tx: &EstimateRequest) -> Result<EstimateResult, String> {
        if let Some(function) = &self.estimated_fee_handler {
            return function(tx);
        }

        Err("Not handled".to_owned())
    }

    /// Send a transaction
    async fn send(&self, tx: &SendTransaction) -> Result<Hash, String> {
        if let Some(function) = &self.send_handler {
            return function(tx);
        }
        Err("Not handled".to_owned())
    }
}
