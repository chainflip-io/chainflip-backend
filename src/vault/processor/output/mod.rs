use std::{collections::HashMap, sync::Arc};

use itertools::Itertools;
use parking_lot::RwLock;

use crate::{
    common::Coin, side_chain::SideChainTx, transactions::OutputTx,
    vault::transactions::memory_provider::FulfilledTxWrapper,
    vault::transactions::TransactionProvider,
};

mod coin_processor;
mod senders;

pub use coin_processor::{CoinProcessor, OutputCoinProcessor};

pub use senders::loki_sender::LokiSender;

/// Process all pending outputs
pub async fn process_outputs<T: TransactionProvider + Sync, C: CoinProcessor>(
    provider: &mut Arc<RwLock<T>>,
    coin_processor: &C,
) {
    provider.write().sync();

    process(provider, coin_processor).await;
}

fn group_by_coins(outputs: &[FulfilledTxWrapper<OutputTx>]) -> HashMap<Coin, Vec<OutputTx>> {
    outputs
        .iter()
        .filter(|tx| !tx.fulfilled)
        .map(|tx| (tx.inner.coin, tx.inner.clone()))
        .into_group_map()
}

async fn process<T: TransactionProvider + Sync, C: CoinProcessor>(
    provider: &mut Arc<RwLock<T>>,
    coin_processor: &C,
) {
    // Get outputs and group them by their coin types
    let groups = {
        let provider_lock = provider.read();
        let outputs = provider_lock.get_output_txs();
        group_by_coins(outputs)
    };

    let futs = groups
        .iter()
        .map(|(coin, outputs)| coin_processor.process(*coin, outputs));

    let txs = futures::future::join_all(futs)
        .await
        .into_iter()
        .map(|txs| txs.into_iter().map_into::<SideChainTx>().collect_vec())
        .flatten()
        .collect_vec();

    match provider.write().add_transactions(txs) {
        Ok(_) => (),
        Err(err) => {
            error!("Could not save output sent txs: {}", err);
            // TODO: investigate how we could recover from this error
            panic!();
        }
    }
}

#[cfg(test)]
mod test {
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
    };

    use crate::{
        common::{Timestamp, WalletAddress},
        side_chain::ISideChain,
        side_chain::MemorySideChain,
        transactions::OutputSentTx,
        utils::test_utils::create_fake_output_tx,
        vault::transactions::MemoryTransactionsProvider,
    };

    use super::*;

    struct TestCoinProcessor {
        map: HashMap<Coin, Vec<OutputSentTx>>,
    }

    impl TestCoinProcessor {
        fn new() -> Self {
            Self {
                map: HashMap::new(),
            }
        }

        fn set_txs(&mut self, coin: Coin, txs: Vec<OutputSentTx>) {
            self.map.insert(coin, txs);
        }
    }

    #[async_trait]
    impl CoinProcessor for TestCoinProcessor {
        async fn process(&self, coin: Coin, _outputs: &[OutputTx]) -> Vec<OutputSentTx> {
            self.map.get(&coin).cloned().unwrap_or(vec![])
        }
    }

    #[test]
    fn groups_outputs_by_coins_correctly() {
        let loki_output = create_fake_output_tx(Coin::LOKI);
        let second_loki_output = create_fake_output_tx(Coin::LOKI);
        let eth_output = create_fake_output_tx(Coin::ETH);
        let second_eth_output = create_fake_output_tx(Coin::ETH);
        let fulfilled_output = create_fake_output_tx(Coin::LOKI);

        let txs = vec![
            FulfilledTxWrapper {
                inner: loki_output.clone(),
                fulfilled: false,
            },
            FulfilledTxWrapper {
                inner: eth_output.clone(),
                fulfilled: false,
            },
            FulfilledTxWrapper {
                inner: second_loki_output.clone(),
                fulfilled: false,
            },
            FulfilledTxWrapper {
                inner: second_eth_output.clone(),
                fulfilled: false,
            },
            FulfilledTxWrapper {
                inner: fulfilled_output,
                fulfilled: true,
            },
        ];
        let grouped = group_by_coins(&txs);
        assert_eq!(
            grouped.get(&Coin::LOKI).unwrap(),
            &[loki_output, second_loki_output]
        );
        assert_eq!(
            grouped.get(&Coin::ETH).unwrap(),
            &[eth_output, second_eth_output]
        );
    }

    #[tokio::test]
    async fn process_stores_output_sent_txs() {
        let mut chain = MemorySideChain::new();
        let output_tx = create_fake_output_tx(Coin::LOKI);
        chain.add_block(vec![output_tx.clone().into()]).unwrap();

        let chain = Arc::new(Mutex::new(chain));
        let mut provider = MemoryTransactionsProvider::new_protected(chain);
        provider.write().sync();

        // Pre-condition: Output is not fulfilled
        assert_eq!(provider.read().get_output_txs().len(), 1);
        let current_output_tx = provider.read().get_output_txs().first().unwrap().clone();
        assert_eq!(current_output_tx.fulfilled, false);

        let output_sent_tx = OutputSentTx {
            id: uuid::Uuid::new_v4(),
            timestamp: Timestamp::now(),
            output_txs: vec![output_tx.id],
            coin: Coin::LOKI,
            address: WalletAddress::new("address"),
            amount: 100,
            fee: 100,
            transaction_id: "".to_owned(),
        };

        let mut coin_processor = TestCoinProcessor::new();
        coin_processor.set_txs(Coin::LOKI, vec![output_sent_tx]);

        process(&mut provider, &coin_processor).await;

        assert_eq!(provider.read().get_output_txs().len(), 1);
        let current_output_tx = provider.read().get_output_txs().first().unwrap().clone();
        assert_eq!(current_output_tx.fulfilled, true);
    }

    #[tokio::test]
    async fn process_with_no_sent_output_tx() {
        let mut chain = MemorySideChain::new();
        let output_tx = create_fake_output_tx(Coin::LOKI);
        chain.add_block(vec![output_tx.clone().into()]).unwrap();

        let chain = Arc::new(Mutex::new(chain));
        let mut provider = MemoryTransactionsProvider::new_protected(chain);
        provider.write().sync();

        // Pre-condition: Output is not fulfilled
        assert_eq!(provider.read().get_output_txs().len(), 1);
        let current_output_tx = provider.read().get_output_txs().first().unwrap().clone();
        assert_eq!(current_output_tx.fulfilled, false);

        let mut coin_processor = TestCoinProcessor::new();
        coin_processor.set_txs(Coin::LOKI, vec![]);

        process(&mut provider, &coin_processor).await;

        assert_eq!(provider.read().get_output_txs().len(), 1);
        let current_output_tx = provider.read().get_output_txs().first().unwrap().clone();
        assert_eq!(current_output_tx.fulfilled, false);
    }
}
