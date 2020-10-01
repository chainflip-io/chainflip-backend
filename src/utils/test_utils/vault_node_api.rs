use crate::quoter::vault_node::{QuoteParams, VaultNodeInterface};
use crate::side_chain::SideChainBlock;
use std::{collections::VecDeque, sync::Mutex};

pub struct TestVaultNodeAPI {
    pub get_blocks_return: Mutex<VecDeque<Vec<SideChainBlock>>>,
    pub get_blocks_error: Mutex<Option<String>>,
}

impl TestVaultNodeAPI {
    pub fn new() -> Self {
        TestVaultNodeAPI {
            get_blocks_return: Mutex::new(VecDeque::new()),
            get_blocks_error: Mutex::new(None),
        }
    }

    /// Adds block to get_blocks_return queue.
    pub fn add_blocks(&self, blocks: Vec<SideChainBlock>) {
        self.get_blocks_return.lock().unwrap().push_back(blocks);
    }

    pub fn set_get_blocks_error(&self, error: Option<String>) {
        *self.get_blocks_error.lock().unwrap() = error;
    }
}

impl VaultNodeInterface for TestVaultNodeAPI {
    fn get_blocks(&self, _start: u32, _limit: u32) -> Result<Vec<SideChainBlock>, String> {
        if let Some(error) = self.get_blocks_error.lock().unwrap().as_ref() {
            return Err(error.clone());
        }

        let blocks = match self.get_blocks_return.lock().unwrap().pop_front() {
            Some(blocks) => blocks,
            _ => vec![],
        };
        Ok(blocks)
    }
    fn submit_quote(&self, params: QuoteParams) -> Result<crate::transactions::QuoteTx, String> {
        todo!()
    }
}
