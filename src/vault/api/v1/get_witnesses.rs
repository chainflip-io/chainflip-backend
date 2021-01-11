use crate::{
    common::api::ResponseError,
    side_chain::{ISideChain, SideChainTx},
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
pub(super) async fn get_witnesses<S: ISideChain>(
    side_chain: Arc<Mutex<S>>,
) -> Result<WitnessQueryResponse, ResponseError> {
    let side_chain = side_chain.lock().unwrap();

    let total = side_chain.total_blocks();

    let mut witness_txs = vec![];

    for block_idx in 0..total {
        let block = side_chain.get_block(block_idx).expect("invalid index");

        for tx in &block.transactions {
            if let SideChainTx::Witness(tx) = tx {
                witness_txs.push(tx.clone());
            }
        }
    }

    let res = WitnessQueryResponse { witness_txs };

    Ok(res)
}

#[cfg(test)]
mod tests {

    use chainflip_common::types::coin::Coin;

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
    async fn check_get_witnesses() {
        let chain = init();
        let chain = Arc::new(Mutex::new(chain));

        let res = get_witnesses(chain).await.expect("result should be OK");

        assert_eq!(res.witness_txs.len(), 2);
    }
}
