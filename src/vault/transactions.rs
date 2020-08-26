use crate::{
    side_chain::{ISideChain, SideChainTx},
    transactions::{QuoteTx, WitnessTx},
};
use std::sync::{Arc, Mutex};

/// An interface for providing transactions
pub trait TransactionProvider {
    /// Sync new transactions
    fn sync(&mut self);

    /// Add transactions
    fn add_transactions(&mut self, txs: Vec<SideChainTx>) -> Result<(), String>;

    /// Get all the quote transactions
    fn get_quote_txs(&self) -> &[QuoteTx];

    /// Get all the witness transactions
    fn get_witness_txs(&self) -> &[WitnessTx];
}

/// An in-memory transaction provider
pub(crate) struct MemoryTransactionsProvider<S: ISideChain> {
    side_chain: Arc<Mutex<S>>,
    quote_txs: Vec<QuoteTx>,
    witness_txs: Vec<WitnessTx>,
    next_block_idx: u32,
}

impl<S: ISideChain> MemoryTransactionsProvider<S> {
    /// Create an in-memory transaction provider
    pub fn new(side_chain: Arc<Mutex<S>>) -> Self {
        MemoryTransactionsProvider {
            side_chain: side_chain,
            quote_txs: vec![],
            witness_txs: vec![],
            next_block_idx: 0,
        }
    }
}

impl<S: ISideChain> TransactionProvider for MemoryTransactionsProvider<S> {
    fn sync(&mut self) {
        let side_chain = self.side_chain.lock().unwrap();
        while let Some(block) = side_chain.get_block(self.next_block_idx) {
            for tx in block.clone().txs {
                match tx {
                    SideChainTx::QuoteTx(tx) => self.quote_txs.push(tx),
                    SideChainTx::WitnessTx(tx) => self.witness_txs.push(tx),
                    _ => continue,
                }
            }
            self.next_block_idx += 1;
        }
    }

    fn add_transactions(&mut self, txs: Vec<SideChainTx>) -> Result<(), String> {
        // Filter out any duplicate transactions
        let valid_txs: Vec<SideChainTx> = txs
            .into_iter()
            .filter(|tx| {
                if let SideChainTx::WitnessTx(tx) = tx {
                    return !self.witness_txs.iter().any(|witness| tx == witness);
                }

                true
            })
            .collect();

        if valid_txs.len() > 0 {
            self.side_chain.lock().unwrap().add_block(valid_txs)?;
        }

        self.sync();
        Ok(())
    }

    fn get_quote_txs(&self) -> &[QuoteTx] {
        &self.quote_txs
    }

    fn get_witness_txs(&self) -> &[WitnessTx] {
        &self.witness_txs
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::side_chain::FakeSideChain;
    use crate::utils::test_utils::create_fake_quote_tx;

    fn setup() -> MemoryTransactionsProvider<FakeSideChain> {
        let side_chain = Arc::new(Mutex::new(FakeSideChain::new()));
        MemoryTransactionsProvider::new(side_chain)
    }

    #[test]
    fn test_provider() {
        let mut provider = setup();

        assert!(provider.get_quote_txs().is_empty());
        assert!(provider.get_witness_txs().is_empty());

        // Add some random blocks
        {
            let mut side_chain = provider.side_chain.lock().unwrap();

            let quote = create_fake_quote_tx();
            let witness = WitnessTx {
                quote_id: quote.id,
                transaction_id: "0".to_owned(),
                transaction_block_number: 0,
                transaction_index: 1,
                amount: 100,
                sender: None,
            };

            side_chain
                .add_block(vec![
                    SideChainTx::QuoteTx(quote),
                    SideChainTx::WitnessTx(witness),
                ])
                .unwrap();
        }

        provider.sync();

        assert_eq!(provider.next_block_idx, 1);
        assert_eq!(provider.get_quote_txs().len(), 1);
        assert_eq!(provider.get_witness_txs().len(), 1);

        provider
            .add_transactions(vec![SideChainTx::QuoteTx(create_fake_quote_tx())])
            .unwrap();

        assert_eq!(provider.next_block_idx, 2);
        assert_eq!(provider.get_quote_txs().len(), 2);
    }

    #[test]
    fn test_provider_does_not_add_duplicates() {
        let mut provider = setup();

        let quote = create_fake_quote_tx();
        let witness = WitnessTx {
            quote_id: quote.id,
            transaction_id: "0".to_owned(),
            transaction_block_number: 0,
            transaction_index: 1,
            amount: 100,
            sender: None,
        };

        {
            let mut side_chain = provider.side_chain.lock().unwrap();

            side_chain
                .add_block(vec![
                    SideChainTx::QuoteTx(quote),
                    SideChainTx::WitnessTx(witness.clone()),
                ])
                .unwrap();
        }

        provider.sync();

        assert_eq!(provider.get_witness_txs().len(), 1);
        assert_eq!(provider.next_block_idx, 1);

        provider
            .add_transactions(vec![SideChainTx::WitnessTx(witness)])
            .unwrap();

        assert_eq!(provider.get_witness_txs().len(), 1);
        assert_eq!(provider.next_block_idx, 1);
    }
}
