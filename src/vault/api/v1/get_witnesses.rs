use crate::{
    common::api::ResponseError,
    local_store::{ILocalStore, LocalEvent},
};
use chainflip_common::types::chain::Witness;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

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
/// > GET /v1/witnesses
pub(super) async fn get_local_witnesses<L: ILocalStore>(
    local_store: Arc<Mutex<L>>,
) -> Result<WitnessQueryResponse, ResponseError> {
    let mut local_store = local_store.lock().unwrap();

    let mut witness_txs = vec![];

    // get *all* events from the beginning of time
    let events = local_store.get_events(0).expect("invalid index");

    for evt in &events {
        if let LocalEvent::Witness(e) = evt {
            witness_txs.push(e.clone());
        }
    }

    Ok(WitnessQueryResponse { witness_txs })
}

#[cfg(test)]
mod tests {

    use chainflip_common::types::coin::Coin;

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

        let witness = TestData::witness(quote.id, loki_amount.to_atomic(), Coin::LOKI);

        store
            .add_events(vec![witness.into()])
            .expect("adding events");

        let witness = TestData::witness(quote.id, eth_amount.to_atomic(), Coin::ETH);

        store
            .add_events(vec![witness.into()])
            .expect("adding events");

        store
    }

    #[tokio::test]
    async fn check_get_witnesses() {
        let store = init();
        let store = Arc::new(Mutex::new(store));

        let res = get_local_witnesses(store)
            .await
            .expect("result should be OK");

        assert_eq!(res.witness_txs.len(), 2);
    }
}
