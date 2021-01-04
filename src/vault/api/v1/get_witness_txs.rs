use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

use crate::{
    common::api::ResponseError,
    side_chain::{ISideChain, SideChainTx},
    transactions::WitnessTx,
};

/// Typed representation of the response for /get_witness_txs
#[serde(rename_all = "camelCase")]
#[derive(Debug, Deserialize, Serialize)]
pub(super) struct WitnessQueryResponse {
    /// The current blocks
    pub witness_txs: Vec<WitnessTx>,
}

/// Get the side chain witness transactions
///
/// # Example Query
///
/// > GET /v1/get_witness_txs
pub(super) async fn get_witness_txs<S: ISideChain>(
    side_chain: Arc<Mutex<S>>,
) -> Result<WitnessQueryResponse, ResponseError> {
    let side_chain = side_chain.lock().unwrap();

    let total = side_chain.total_blocks();

    let mut witness_txs = vec![];

    for block_idx in 0..total {
        let block = side_chain.get_block(block_idx).expect("invalid index");

        for tx in &block.transactions {
            if let SideChainTx::WitnessTx(tx) = tx {
                witness_txs.push(tx.clone());
            }
        }
    }

    let res = WitnessQueryResponse { witness_txs };

    Ok(res)
}

#[cfg(test)]
mod tests {

    use crate::{
        common::{Coin, GenericCoinAmount, LokiAmount, PoolCoin},
        side_chain::MemorySideChain,
        utils::test_utils::{create_fake_stake_quote, create_fake_witness},
    };

    use super::*;

    fn init() -> MemorySideChain {
        let mut chain = MemorySideChain::new();

        let quote = create_fake_stake_quote(PoolCoin::ETH);

        let loki_amount = LokiAmount::from_decimal_string("10");

        let eth_amount = GenericCoinAmount::from_decimal_string(Coin::ETH, "10");

        let witness = create_fake_witness(&quote, loki_amount, Coin::LOKI);

        chain.add_block(vec![witness.into()]).expect("adding block");

        let witness = create_fake_witness(&quote, eth_amount, Coin::ETH);

        chain.add_block(vec![witness.into()]).expect("adding block");

        chain
    }

    #[tokio::test]
    async fn check_get_witness_txs() {
        let chain = init();
        let chain = Arc::new(Mutex::new(chain));

        let res = get_witness_txs(chain).await.expect("result should be OK");

        assert_eq!(res.witness_txs.len(), 2);
    }
}
