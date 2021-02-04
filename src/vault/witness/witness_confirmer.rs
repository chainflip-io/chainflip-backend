use std::sync::Arc;

/// Defines a processor for fetching and setting the confirmed flag on witnesses
use chainflip_common::types::chain::{UniqueId, Witness};
use common::api::ScResponse;
use parking_lot::RwLock;
use reqwest::{header::CONTENT_TYPE, Response};
use serde::Deserialize;

use crate::{
    common,
    vault::transactions::{memory_provider::WitnessStatus, TransactionProvider},
};

#[derive(Deserialize, Debug)]
struct ConfirmedWitnessResponse {
    // success: bool,
    result: Vec<String>,
}

#[derive(Debug)]
enum WitnessConfirmerError {
    ResultParsingError(String),
    WitnessFetchError(String),
}

type Result<T> = std::result::Result<T, WitnessConfirmerError>;

/// Polls the state chain for any witnesses that it has seen and sets the confirmed flag
/// on witnesses it sees are confirmed on its local database
pub struct WitnessConfirmer<T>
where
    T: TransactionProvider,
{
    // provides access to underlying local store, used to store events
    provider: Arc<RwLock<T>>,
}

fn state_chain_id_to_local_store_id(sc_id: String) -> UniqueId {
    let split: Vec<&str> = sc_id.split("-").collect();
    Witness::id_from_id_fields(split.get(0).unwrap(), split.get(1).unwrap())
}

impl<T> WitnessConfirmer<T>
where
    T: TransactionProvider + Send + Sync + 'static,
{
    /// Create a new witness confirmer from a transaction provider
    pub fn new(provider: Arc<RwLock<T>>) -> Self {
        WitnessConfirmer { provider }
    }

    async fn event_loop(&mut self) {
        loop {
            self.poll_state_chain().await;

            std::thread::sleep(std::time::Duration::from_secs(10));
        }
    }

    /// Start witnessing the bitcoin chain on a new thread
    pub fn start(mut self) {
        std::thread::spawn(move || {
            let mut rt = tokio::runtime::Runtime::new().unwrap();

            rt.block_on(async {
                self.event_loop().await;
            });
        });
    }

    /// Polls state chain for witnesses that have been confirmed by the network
    async fn poll_state_chain(&self) {
        let sc_witness_ids = match self.get_confirmed_witness_ids().await {
            Ok(ws) => ws,
            Err(e) => {
                error!("Failed to fetch witnesses, with error: {:?}", e);
                return ();
            }
        };

        for sc_id in sc_witness_ids {
            let id = state_chain_id_to_local_store_id(sc_id);
            // confirms it in memory
            if let Err(e) = self.provider.write().confirm_witness(id) {
                error!("Failed to confirm witness {}. {:#?}", id, e);
            }
        }
    }

    async fn get_confirmed_witness_ids(&self) -> Result<Vec<String>> {
        let client = reqwest::Client::new();
        let req_body = serde_json::json!({
            "jsonrpc":"2.0",
            "id":1,
            "method":"get_confirmed_witnesses",
            "params": []
        });
        let res = client
            .post("http://localhost:9933")
            .header(CONTENT_TYPE, "application/json;charset=utf-8")
            .json(&req_body)
            .send()
            .await
            .map_err(|e| WitnessConfirmerError::WitnessFetchError(e.to_string()))?;

        let res = res
            .json::<ScResponse<Vec<String>>>()
            .await
            .map_err(|e| WitnessConfirmerError::ResultParsingError(e.to_string()))?;

        println!("Here's the res: {:#?}", res);

        Ok(res.result.unwrap_or(vec![]))
    }
}

#[cfg(test)]
mod test {

    use std::sync::Mutex;

    use crate::{local_store::MemoryLocalStore, vault::transactions::MemoryTransactionsProvider};

    use super::*;

    fn setup() -> WitnessConfirmer<MemoryTransactionsProvider<MemoryLocalStore>> {
        let local_store = Arc::new(Mutex::new(MemoryLocalStore::new()));
        let memory_provider = MemoryTransactionsProvider::new(local_store);
        WitnessConfirmer {
            provider: Arc::new(RwLock::new(memory_provider)),
        }
    }

    #[test]
    fn can_parse_sc_witness_id_to_local_id() {
        let sc_id =
            "btc-10207e83dd1661431e27df6556daaecf1145205915ebeefb1b391876bcb2d5e6".to_string();
        let local_store_id = state_chain_id_to_local_store_id(sc_id);

        assert_eq!(local_store_id, 10926892979007146660);
    }

    #[test]
    #[ignore]
    fn sets_status_confirmed_when_sc_witness_matches_local_witness() {}

    #[test]
    #[ignore]
    fn a_duplicate_witness_has_no_effect() {}

    #[tokio::test]
    #[ignore = "depends on state chain"]
    async fn get_confirmed_witnesses_rpc_call() {
        let witness_confirmer = setup();
        let confirmed_witnesses = witness_confirmer.get_confirmed_witness_ids().await;

        assert!(confirmed_witnesses.is_ok());
        let witness_ids = confirmed_witnesses.unwrap();
        println!("The witness ids are: {:#?}", witness_ids);
    }

    #[tokio::test]
    async fn state_chain_returns_witness_local_has_not_seen() {}
}
