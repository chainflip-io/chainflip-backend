use crate::vault::transactions::TransactionProvider;
use crate::{
    side_chain::SideChainTx,
    transactions::{QuoteTx, WitnessTx},
    utils::price,
};

/// A test transaction provider
#[derive(Debug)]
pub struct TestTransactionProvider {
    quote_txs: Vec<QuoteTx>,
    witness_txs: Vec<WitnessTx>,
}

impl TestTransactionProvider {
    /// Create a new test transaction provider
    pub fn new() -> Self {
        TestTransactionProvider {
            quote_txs: vec![],
            witness_txs: vec![],
        }
    }
}

impl TransactionProvider for TestTransactionProvider {
    fn sync(&mut self) {}

    fn add_transactions(&mut self, txs: Vec<SideChainTx>) -> Result<(), String> {
        for tx in txs {
            match tx {
                SideChainTx::QuoteTx(tx) => self.quote_txs.push(tx),
                SideChainTx::WitnessTx(tx) => self.witness_txs.push(tx),
                _ => continue,
            }
        }
        Ok(())
    }

    fn get_quote_txs(&self) -> &[QuoteTx] {
        &self.quote_txs
    }

    fn get_witness_txs(&self) -> &[WitnessTx] {
        &self.witness_txs
    }

    fn get_liquidity(
        &self,
        pool: crate::common::coins::PoolCoin,
    ) -> Option<crate::vault::transactions::Liquidity> {
        None
    }
}
