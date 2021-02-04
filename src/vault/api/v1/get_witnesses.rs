use crate::{
    common::api::ResponseError,
    local_store::{ILocalStore, LocalEvent},
};
use chainflip_common::types::chain::Witness;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

/// Parameters for GET /get_witnesses request
#[derive(Debug, Deserialize, Serialize)]
pub struct WitnessQueryParams {
    last_seen: Option<u64>,
}

/// Typed representation of the response for /get_witnesses
#[serde(rename_all = "camelCase")]
#[derive(Debug, Deserialize, Serialize)]
pub(super) struct WitnessQueryResponse {
    /// The current blocks
    pub witness_txs: Vec<Witness>,
}

/// Get all known transaction witnesses
///
/// # Example Query
///
/// > GET /v1/witnesses?last_seen=2
pub(super) async fn get_local_witnesses<L: ILocalStore>(
    params: WitnessQueryParams,
    local_store: Arc<Mutex<L>>,
) -> Result<WitnessQueryResponse, ResponseError> {
    let local_store = local_store.lock().unwrap();

    let WitnessQueryParams { last_seen } = params;

    let witness_txs = local_store.get_witnesses(last_seen.unwrap_or(0));

    Ok(WitnessQueryResponse { witness_txs })
}

#[cfg(test)]
mod tests {

    use chainflip_common::types::{coin::Coin, unique_id::GetUniqueId};

    use super::*;
    use crate::{
        common::{GenericCoinAmount, LokiAmount},
        local_store::MemoryLocalStore,
        utils::test_utils::data::TestData,
    };

    fn init() -> MemoryLocalStore {
        let mut store = MemoryLocalStore::new();

        let quote = TestData::deposit_quote(Coin::ETH);

        let loki_amount = LokiAmount::from_decimal_string("10");

        let eth_amount = GenericCoinAmount::from_decimal_string(Coin::ETH, "10");

        let witness = TestData::witness(quote.unique_id(), loki_amount.to_atomic(), Coin::LOKI);
        let witness2 = TestData::witness(quote.unique_id(), eth_amount.to_atomic(), Coin::ETH);

        store
            .add_events(vec![witness.into(), witness2.into()])
            .expect("adding events");

        store
    }

    #[tokio::test]
    async fn check_get_witnesses() {
        let store = init();
        let store = Arc::new(Mutex::new(store));

        let params = WitnessQueryParams { last_seen: None };

        let res = get_local_witnesses(params, store)
            .await
            .expect("result should be OK");

        assert_eq!(res.witness_txs.len(), 2);
    }

    #[tokio::test]
    async fn get_witnesses_returns_recent_only() {
        let store = init();
        let store = Arc::new(Mutex::new(store));

        let last_seen: Option<u64> = Some(1);

        let params = WitnessQueryParams { last_seen };

        // Add a fresh witness, to the 2 already in there
        {
            let mut store = store.lock().unwrap();

            let evt1 = TestData::witness(0, 10, Coin::BTC);

            store
                .add_events(vec![evt1.into()])
                .expect("Could not add event");
        }

        let res = get_local_witnesses(params, store)
            .await
            .expect("result should be OK");

        assert_eq!(res.witness_txs.len(), 2);
    }
}
