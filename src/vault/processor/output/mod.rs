use itertools::Itertools;

use crate::{
    common::Coin, side_chain::SideChainTx, transactions::OutputTx,
    vault::blockchain_connection::ethereum::EthereumClient,
    vault::transactions::memory_provider::FulfilledTxWrapper,
    vault::transactions::TransactionProvider,
};

mod coin_processor;
mod senders;

use coin_processor::CoinProcessor;
pub use coin_processor::OutputCoinProcessor;

/// Process all pending outputs
pub fn process_outputs<T: TransactionProvider, E: EthereumClient>(
    provider: &mut T,
    coin_processor: &OutputCoinProcessor<E>,
) {
    provider.sync();

    process(provider, coin_processor);
}

fn get_grouped(outputs: &[FulfilledTxWrapper<OutputTx>]) -> Vec<(Coin, Vec<OutputTx>)> {
    let groups = outputs
        .iter()
        .filter(|tx| !tx.fulfilled)
        .group_by(|tx| tx.inner.coin);

    groups
        .into_iter()
        .map(|(coin, group)| (coin, group.map(|tx| tx.inner.clone()).collect_vec()))
        .collect()
}

fn process<T: TransactionProvider, C: CoinProcessor>(provider: &mut T, coin_processor: &C) {
    // Get outputs and group them by their coin types
    let outputs = provider.get_output_txs();
    let groups = get_grouped(outputs);

    for (coin, outputs) in groups {
        let sent_txs = coin_processor.process(coin, &outputs);
        if sent_txs.len() > 0 {
            let txs = sent_txs.into_iter().map_into::<SideChainTx>().collect_vec();
            provider.add_transactions(txs).unwrap()
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

    impl CoinProcessor for TestCoinProcessor {
        fn process(&self, coin: Coin, _outputs: &[OutputTx]) -> Vec<OutputSentTx> {
            self.map.get(&coin).cloned().unwrap_or(vec![])
        }
    }

    fn get_output_tx(coin: Coin) -> OutputTx {
        OutputTx {
            id: uuid::Uuid::new_v4(),
            timestamp: Timestamp::now(),
            quote_tx: uuid::Uuid::new_v4(),
            witness_txs: vec![],
            pool_change_txs: vec![],
            coin,
            address: WalletAddress::new("address"),
            amount: 100,
        }
    }

    #[test]
    fn groups_outputs_correctly() {
        let loki_output = get_output_tx(Coin::LOKI);
        let eth_output = get_output_tx(Coin::ETH);
        let second_eth_output = get_output_tx(Coin::ETH);
        let fulfilled_output = get_output_tx(Coin::LOKI);

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
                inner: second_eth_output.clone(),
                fulfilled: false,
            },
            FulfilledTxWrapper {
                inner: fulfilled_output,
                fulfilled: true,
            },
        ];
        let grouped = get_grouped(&txs);
        assert_eq!(
            grouped,
            vec![
                (Coin::LOKI, vec![loki_output]),
                (Coin::ETH, vec![eth_output, second_eth_output])
            ]
        );
    }

    #[test]
    fn process_stores_output_sent_txs() {
        let mut chain = MemorySideChain::new();
        let output_tx = get_output_tx(Coin::LOKI);
        chain.add_block(vec![output_tx.clone().into()]).unwrap();

        let chain = Arc::new(Mutex::new(chain));
        let mut provider = MemoryTransactionsProvider::new(chain);
        provider.sync();

        // Pre-condition: Output is not fulfilled
        assert_eq!(provider.get_output_txs().len(), 1);
        let current_output_tx = provider.get_output_txs().first().unwrap();
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

        process(&mut provider, &coin_processor);

        assert_eq!(provider.get_output_txs().len(), 1);
        let current_output_tx = provider.get_output_txs().first().unwrap();
        assert_eq!(current_output_tx.fulfilled, true);
    }

    #[test]
    fn process_with_no_sent_output_tx() {
        let mut chain = MemorySideChain::new();
        let output_tx = get_output_tx(Coin::LOKI);
        chain.add_block(vec![output_tx.clone().into()]).unwrap();

        let chain = Arc::new(Mutex::new(chain));
        let mut provider = MemoryTransactionsProvider::new(chain);
        provider.sync();

        // Pre-condition: Output is not fulfilled
        assert_eq!(provider.get_output_txs().len(), 1);
        let current_output_tx = provider.get_output_txs().first().unwrap();
        assert_eq!(current_output_tx.fulfilled, false);

        let mut coin_processor = TestCoinProcessor::new();
        coin_processor.set_txs(Coin::LOKI, vec![]);

        process(&mut provider, &coin_processor);

        assert_eq!(provider.get_output_txs().len(), 1);
        let current_output_tx = provider.get_output_txs().first().unwrap();
        assert_eq!(current_output_tx.fulfilled, false);
    }
}
