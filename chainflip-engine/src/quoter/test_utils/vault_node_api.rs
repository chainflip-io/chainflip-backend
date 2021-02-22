use std::{collections::VecDeque, sync::Mutex};

use crate::{
    local_store::LocalEvent,
    quoter::vault_node::VaultNodeInterface,
    vault::api::v1::{
        post_deposit::DepositQuoteParams, post_swap::SwapQuoteParams,
        post_withdraw::WithdrawParams, PortionsParams,
    },
};

/// Test vault node API
pub struct TestVaultNodeAPI {
    /// Return values of get_events
    pub get_events_return: Mutex<VecDeque<Vec<LocalEvent>>>,
    /// Error value of get_events
    pub get_events_error: Mutex<Option<String>>,
}

impl TestVaultNodeAPI {
    /// Create a new test vault node api
    pub fn new() -> Self {
        TestVaultNodeAPI {
            get_events_return: Mutex::new(VecDeque::new()),
            get_events_error: Mutex::new(None),
        }
    }

    /// Adds events to get_events_return queue.
    pub fn add_events(&self, events: Vec<LocalEvent>) {
        self.get_events_return.lock().unwrap().push_back(events);
    }

    /// Set the get events error
    pub fn set_get_events_error(&self, error: Option<String>) {
        *self.get_events_error.lock().unwrap() = error;
    }
}

#[async_trait]
impl VaultNodeInterface for TestVaultNodeAPI {
    async fn get_events(&self, _start: u64, _limit: u64) -> Result<Vec<LocalEvent>, String> {
        if let Some(error) = self.get_events_error.lock().unwrap().as_ref() {
            return Err(error.clone());
        }

        let events = match self.get_events_return.lock().unwrap().pop_front() {
            Some(events) => events,
            _ => vec![],
        };
        Ok(events)
    }

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
}
