use crate::{
    local_store::LocalEvent, vault::transactions::memory_provider::FulfilledWrapper,
    vault::transactions::TransactionProvider,
};
use chainflip_common::types::{chain::Output, coin::Coin};
use itertools::Itertools;
use parking_lot::RwLock;
use std::{collections::HashMap, sync::Arc};

mod coin_processor;
mod senders;

pub use coin_processor::{CoinProcessor, OutputCoinProcessor};

pub use senders::{btc::BtcOutputSender, ethereum::EthOutputSender, loki_sender::LokiSender};

/// Process all pending outputs
pub async fn process_outputs<T: TransactionProvider + Sync, C: CoinProcessor>(
    provider: &mut Arc<RwLock<T>>,
    coin_processor: &C,
) {
    provider.write().sync();

    process(provider, coin_processor).await;
}

fn group_by_coins(outputs: &[FulfilledWrapper<Output>]) -> HashMap<Coin, Vec<Output>> {
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
        let outputs = provider_lock.get_outputs();
        group_by_coins(outputs)
    };

    let futs = groups
        .iter()
        .map(|(coin, outputs)| coin_processor.process(*coin, outputs));

    let txs = futures::future::join_all(futs)
        .await
        .into_iter()
        .map(|txs| txs.into_iter().map_into::<LocalEvent>().collect_vec())
        .flatten()
        .collect_vec();

    match provider.write().add_local_events(txs) {
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
    use super::*;
    use crate::{
        local_store::{ILocalStore, MemoryLocalStore},
        utils::test_utils::data::TestData,
        vault::transactions::MemoryTransactionsProvider,
    };
    use chainflip_common::types::{chain::OutputSent, unique_id::GetUniqueId};
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
    };

    struct TestCoinProcessor {
        map: HashMap<Coin, Vec<OutputSent>>,
    }

    impl TestCoinProcessor {
        fn new() -> Self {
            Self {
                map: HashMap::new(),
            }
        }

        fn set_txs(&mut self, coin: Coin, txs: Vec<OutputSent>) {
            self.map.insert(coin, txs);
        }
    }

    #[async_trait]
    impl CoinProcessor for TestCoinProcessor {
        async fn process(&self, coin: Coin, _outputs: &[Output]) -> Vec<OutputSent> {
            self.map.get(&coin).cloned().unwrap_or(vec![])
        }
    }

    #[test]
    fn groups_outputs_by_coins_correctly() {
        let loki_output = TestData::output(Coin::LOKI, 100);
        let second_loki_output = TestData::output(Coin::LOKI, 100);
        let eth_output = TestData::output(Coin::ETH, 100);
        let second_eth_output = TestData::output(Coin::ETH, 100);
        let fulfilled_output = TestData::output(Coin::LOKI, 100);

        let txs = vec![
            FulfilledWrapper {
                inner: loki_output.clone(),
                fulfilled: false,
            },
            FulfilledWrapper {
                inner: eth_output.clone(),
                fulfilled: false,
            },
            FulfilledWrapper {
                inner: second_loki_output.clone(),
                fulfilled: false,
            },
            FulfilledWrapper {
                inner: second_eth_output.clone(),
                fulfilled: false,
            },
            FulfilledWrapper {
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
        let mut store = MemoryLocalStore::new();
        let output_tx = TestData::output(Coin::LOKI, 100);
        store.add_events(vec![output_tx.clone().into()]).unwrap();

        let store = Arc::new(Mutex::new(store));
        let mut provider = MemoryTransactionsProvider::new_protected(store);
        provider.write().sync();

        // Pre-condition: Output is not fulfilled
        assert_eq!(provider.read().get_outputs().len(), 1);
        let current_output_tx = provider.read().get_outputs().first().unwrap().clone();
        assert_eq!(current_output_tx.fulfilled, false);

        let output_sent_tx = OutputSent {
            outputs: vec![output_tx.unique_id()],
            coin: Coin::LOKI,
            address: "address".into(),
            amount: 100,
            fee: 100,
            transaction_id: "".into(),
            event_number: None,
        };

        let mut coin_processor = TestCoinProcessor::new();
        coin_processor.set_txs(Coin::LOKI, vec![output_sent_tx]);

        process(&mut provider, &coin_processor).await;

        assert_eq!(provider.read().get_outputs().len(), 1);
        let current_output_tx = provider.read().get_outputs().first().unwrap().clone();
        assert_eq!(current_output_tx.fulfilled, true);
    }

    #[tokio::test]
    async fn process_with_no_sent_output_tx() {
        let mut store = MemoryLocalStore::new();
        let output_tx = TestData::output(Coin::LOKI, 100);
        store.add_events(vec![output_tx.clone().into()]).unwrap();

        let store = Arc::new(Mutex::new(store));
        let mut provider = MemoryTransactionsProvider::new_protected(store);
        provider.write().sync();

        // Pre-condition: Output is not fulfilled
        assert_eq!(provider.read().get_outputs().len(), 1);
        let current_output_tx = provider.read().get_outputs().first().unwrap().clone();
        assert_eq!(current_output_tx.fulfilled, false);

        let mut coin_processor = TestCoinProcessor::new();
        coin_processor.set_txs(Coin::LOKI, vec![]);

        process(&mut provider, &coin_processor).await;

        assert_eq!(provider.read().get_outputs().len(), 1);
        let current_output_tx = provider.read().get_outputs().first().unwrap().clone();
        assert_eq!(current_output_tx.fulfilled, false);
    }
}
