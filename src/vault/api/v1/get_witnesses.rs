use crate::{
    common::api::ResponseError,
    side_chain::{ISideChain, SideChainTx},
};
use chainflip_common::types::chain::Witness;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

/// Parameters for GET /get_witnesses request
#[derive(Debug, Deserialize, Serialize)]
pub struct WitnessQueryParams {
    last_seen: Option<String>,
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
/// > GET /v1/witnesses
pub(super) async fn get_witnesses<S: ISideChain>(
    params: WitnessQueryParams,
    side_chain: Arc<Mutex<S>>,
) -> Result<WitnessQueryResponse, ResponseError> {
    let side_chain = side_chain.lock().unwrap();

    let WitnessQueryParams { last_seen } = params;

    let last_seen = match last_seen {
        Some(ts) => ts.parse::<u128>().map_err(|_| {
            ResponseError::new(StatusCode::BAD_REQUEST, "Invalid value for last_seen")
        })?,
        None => 0,
    };

    let total = side_chain.total_blocks();

    let mut witness_txs = vec![];

    for block_idx in 0..total {
        let block = side_chain.get_block(block_idx).expect("invalid index");

        for tx in &block.transactions {
            if let SideChainTx::Witness(tx) = tx {
                // There is a risk that we might get two witnesses with exact
                // same timestamp (down to a millisecond), but for now the timestamps will do.
                if tx.timestamp.0 > last_seen as u128 {
                    witness_txs.push(tx.clone());
                }
            }
        }
    }

    let res = WitnessQueryResponse { witness_txs };

    Ok(res)
}

#[cfg(test)]
mod tests {

    use chainflip_common::types::{coin::Coin, Timestamp, UUIDv4};

    use super::*;
    use crate::{
        common::{GenericCoinAmount, LokiAmount},
        side_chain::MemorySideChain,
        utils::test_utils::data::TestData,
    };

    fn init() -> MemorySideChain {
        let mut chain = MemorySideChain::new();

        let quote = TestData::deposit_quote(Coin::ETH);

        let loki_amount = LokiAmount::from_decimal_string("10");

        let eth_amount = GenericCoinAmount::from_decimal_string(Coin::ETH, "10");

        let witness = TestData::witness(quote.id, loki_amount.to_atomic(), Coin::LOKI);

        chain.add_block(vec![witness.into()]).expect("adding block");

        let witness = TestData::witness(quote.id, eth_amount.to_atomic(), Coin::ETH);

        chain.add_block(vec![witness.into()]).expect("adding block");

        chain
    }

    #[tokio::test]
    async fn get_witnesses_returns_all() {
        let chain = init();
        let chain = Arc::new(Mutex::new(chain));

        let params = WitnessQueryParams { last_seen: None };

        let res = get_witnesses(params, chain)
            .await
            .expect("result should be OK");

        assert_eq!(res.witness_txs.len(), 2);
    }

    #[tokio::test]
    async fn get_witnesses_returns_recent_only() {
        use std::thread::sleep;
        use std::time::Duration;

        let chain = init();
        let chain = Arc::new(Mutex::new(chain));

        let last_seen = Some(Timestamp::now().to_string());

        sleep(Duration::from_millis(1));

        let params = WitnessQueryParams { last_seen };

        // Add a fresh witness
        {
            let mut chain = chain.lock().unwrap();

            let tx =
                crate::utils::test_utils::data::TestData::witness(UUIDv4::new(), 10, Coin::BTC);

            chain
                .add_block(vec![tx.into()])
                .expect("Could not add block");
        }

        let res = get_witnesses(params, chain)
            .await
            .expect("result should be OK");

        assert_eq!(res.witness_txs.len(), 1);
    }
}
