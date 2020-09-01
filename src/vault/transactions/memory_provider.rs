use super::{Liquidity, TransactionProvider};
use crate::{
    common::{coins::PoolCoin, Coin},
    side_chain::{ISideChain, SideChainTx},
    transactions::{QuoteTx, WitnessTx},
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

/// An in-memory transaction provider
pub struct MemoryTransactionsProvider<S: ISideChain> {
    side_chain: Arc<Mutex<S>>,
    quote_txs: Vec<QuoteTx>,
    witness_txs: Vec<WitnessTx>,
    pools: HashMap<Coin, Liquidity>,
    next_block_idx: u32,
}

impl<S: ISideChain> MemoryTransactionsProvider<S> {
    /// Create an in-memory transaction provider
    pub fn new(side_chain: Arc<Mutex<S>>) -> Self {
        MemoryTransactionsProvider {
            side_chain: side_chain,
            quote_txs: vec![],
            witness_txs: vec![],
            pools: HashMap::new(),
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

    fn get_liquidity(&self, pool: PoolCoin) -> Option<Liquidity> {
        self.pools.get(&pool.get_coin()).cloned()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::side_chain::FakeSideChain;
    use crate::{transactions::PoolChangeTx, utils::test_utils::create_fake_quote_tx};
    use uuid::Uuid;

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
                id: Uuid::new_v4(),
                quote_id: quote.id,
                transaction_id: "0".to_owned(),
                transaction_block_number: 0,
                transaction_index: 1,
                amount: 100,
                coin_type: Coin::ETH,
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
            id: Uuid::new_v4(),
            quote_id: quote.id,
            transaction_id: "0".to_owned(),
            transaction_block_number: 0,
            transaction_index: 1,
            amount: 100,
            coin_type: Coin::ETH,
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

    #[test]
    #[should_panic(expected = "Negative liquidity depth found")]
    fn test_provider_panics_on_negative_liquidity() {
        let coin = PoolCoin::from(Coin::ETH).expect("Expected valid pool coin");
        let mut provider = setup();
        {
            let change_tx = PoolChangeTx {
                id: Uuid::new_v4(),
                coin,
                depth_change: -100,
                loki_depth_change: -100,
            };

            let mut side_chain = provider.side_chain.lock().unwrap();

            side_chain
                .add_block(vec![SideChainTx::PoolChangeTx(change_tx)])
                .unwrap();
        }

        // Pre condition check
        assert!(provider.get_liquidity(coin).is_none());

        provider.sync();
    }

    #[test]
    fn test_provider_tallies_liquidity() {
        let coin = PoolCoin::from(Coin::ETH).expect("Expected valid pool coin");
        let mut provider = setup();
        {
            let mut side_chain = provider.side_chain.lock().unwrap();

            side_chain
                .add_block(vec![
                    SideChainTx::PoolChangeTx(PoolChangeTx {
                        id: Uuid::new_v4(),
                        coin,
                        depth_change: 100,
                        loki_depth_change: 100,
                    }),
                    SideChainTx::PoolChangeTx(PoolChangeTx {
                        id: Uuid::new_v4(),
                        coin,
                        depth_change: 100,
                        loki_depth_change: -50,
                    }),
                ])
                .unwrap();
        }

        assert!(provider.get_liquidity(coin).is_none());

        provider.sync();

        let liquidity = provider
            .get_liquidity(coin)
            .expect("Expected liquidity to exist");

        assert_eq!(liquidity.depth, 200);
        assert_eq!(liquidity.loki_depth, 50);
    }
}
