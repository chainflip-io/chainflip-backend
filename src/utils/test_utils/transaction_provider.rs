use crate::vault::transactions::{Liquidity, TransactionProvider};
use crate::{
    common::Coin,
    side_chain::SideChainTx,
    transactions::{QuoteTx, StakeQuoteTx, WitnessTx},
};
use std::collections::HashMap;

/// A test transaction provider
#[derive(Debug)]
pub struct TestTransactionProvider {
    quote_txs: Vec<QuoteTx>,
    stake_quote_txs: Vec<StakeQuoteTx>,
    witness_txs: Vec<WitnessTx>,
    pools: HashMap<Coin, Liquidity>,
}

impl TestTransactionProvider {
    /// Create a new test transaction provider
    pub fn new() -> Self {
        TestTransactionProvider {
            quote_txs: vec![],
            stake_quote_txs: vec![],
            witness_txs: vec![],
            pools: HashMap::new(),
        }
    }
}

impl TransactionProvider for TestTransactionProvider {
    fn sync(&mut self) -> u32 {
        0
    }

    fn add_transactions(&mut self, txs: Vec<SideChainTx>) -> Result<(), String> {
        for tx in txs {
            match tx {
                SideChainTx::QuoteTx(tx) => self.quote_txs.push(tx),
                SideChainTx::WitnessTx(tx) => self.witness_txs.push(tx),
                SideChainTx::PoolChangeTx(tx) => {
                    let mut liquidity = self
                        .pools
                        .get(&tx.coin.get_coin())
                        .cloned()
                        .unwrap_or(Liquidity::new());

                    let depth = liquidity.depth as i128 + tx.depth_change;
                    let loki_depth = liquidity.loki_depth as i128 + tx.loki_depth_change;
                    if depth < 0 || loki_depth < 0 {
                        error!(
                            "Negative liquidity depth found for tx: {:?}. Current: {:?}",
                            tx, liquidity
                        );
                        panic!("Negative liquidity depth found");
                    }

                    liquidity.depth = depth as u128;
                    liquidity.loki_depth = loki_depth as u128;
                    self.pools.insert(tx.coin.get_coin(), liquidity);
                }
                _ => continue,
            }
        }
        Ok(())
    }

    fn get_quote_txs(&self) -> &[QuoteTx] {
        &self.quote_txs
    }

    fn get_stake_quote_txs(&self) -> &[StakeQuoteTx] {
        &self.stake_quote_txs
    }

    fn get_witness_txs(&self) -> &[WitnessTx] {
        &self.witness_txs
    }

    fn get_liquidity(
        &self,
        pool: crate::common::coins::PoolCoin,
    ) -> Option<crate::vault::transactions::Liquidity> {
        self.pools.get(&pool.get_coin()).cloned()
    }
}
