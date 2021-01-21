use crate::vault::api::v1::post_deposit::DepositQuoteParams;
use crate::{
    quoter::vault_node::VaultNodeInterface,
    vault::api::v1::{post_swap::SwapQuoteParams, post_withdraw::WithdrawParams, PortionsParams},
};
use std::{collections::VecDeque, sync::Mutex};

/// Test vault node API
pub struct TestVaultNodeAPI {
    /// Return values of get_blocks
    // pub get_blocks_return: Mutex<VecDeque<Vec<SideChainBlock>>>,
    /// Error value of get_blocks
    pub get_blocks_error: Mutex<Option<String>>,
}

impl TestVaultNodeAPI {
    /// Create a new test vault node api
    pub fn new() -> Self {
        TestVaultNodeAPI {
            // get_blocks_return: Mutex::new(VecDeque::new()),
            get_blocks_error: Mutex::new(None),
        }
    }

    /// Adds block to get_blocks_return queue.
    // pub fn add_blocks(&self, blocks: Vec<SideChainBlock>) {
    //     self.get_blocks_return.lock().unwrap().push_back(blocks);
    // }

    /// Set the get blocks error
    pub fn set_get_blocks_error(&self, error: Option<String>) {
        *self.get_blocks_error.lock().unwrap() = error;
    }
}

#[async_trait]
impl VaultNodeInterface for TestVaultNodeAPI {
    // async fn get_blocks(&self, _start: u32, _limit: u32) -> Result<Vec<SideChainBlock>, String> {
    //     if let Some(error) = self.get_blocks_error.lock().unwrap().as_ref() {
    //         return Err(error.clone());
    //     }

    //     let blocks = match self.get_blocks_return.lock().unwrap().pop_front() {
    //         Some(blocks) => blocks,
    //         _ => vec![],
    //     };
    //     Ok(blocks)
    // }
    async fn submit_swap(&self, _params: SwapQuoteParams) -> Result<serde_json::Value, String> {
        todo!()
    }

    async fn submit_deposit(
        &self,
        _params: DepositQuoteParams,
    ) -> Result<serde_json::Value, String> {
        todo!()
    }

    async fn submit_withdraw(&self, _params: WithdrawParams) -> Result<serde_json::Value, String> {
        todo!()
    }

    async fn get_portions(&self, _params: PortionsParams) -> Result<serde_json::Value, String> {
        todo!()
    }

    async fn get_events(
        &self,
        start: u64,
        limit: u64,
    ) -> Result<Vec<crate::local_store::LocalEvent>, String> {
        todo!()
    }
}
