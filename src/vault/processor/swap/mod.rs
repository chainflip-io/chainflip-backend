use crate::{
    common::liquidity_provider::{LiquidityProvider, MemoryLiquidityProvider},
    constants::SWAP_QUOTE_HARD_EXPIRE,
    local_store::{ILocalStore, LocalEvent},
    vault::transactions::{
        memory_provider::{FulfilledWrapper, StatusWitnessWrapper, WitnessStatus},
        TransactionProvider,
    },
};
use chainflip_common::types::{
    chain::{SwapQuote, Witness},
    unique_id::GetUniqueId,
    Network, Timestamp,
};
use parking_lot::RwLock;
use std::sync::Arc;

mod logic;
mod refund;

#[derive(Debug)]
struct Swap {
    quote: FulfilledWrapper<SwapQuote>,
    witnesses: Vec<Witness>,
}

fn get_swaps<T: TransactionProvider>(provider: &T) -> Vec<Swap> {
    let now = Timestamp::now();
    let is_hard_expired = |quote: &SwapQuote| now.0 - quote.timestamp.0 >= SWAP_QUOTE_HARD_EXPIRE;

    let quotes: Vec<&FulfilledWrapper<SwapQuote>> = provider
        .get_swap_quotes()
        .iter()
        .filter(|tx| !is_hard_expired(&tx.inner))
        .collect();

    let witnesses: Vec<&StatusWitnessWrapper> = provider
        .get_witnesses()
        .iter()
        .filter(|tx| !(tx.status == WitnessStatus::Processed))
        .collect();

    let mut swaps: Vec<Swap> = vec![];

    for quote in quotes {
        let witnesses: Vec<Witness> = witnesses
            .iter()
            .filter(|tx| {
                tx.inner.quote == quote.inner.unique_id() && tx.inner.coin == quote.inner.input
            })
            .map(|tx| tx.inner.clone())
            .collect();

        if witnesses.len() > 0 {
            swaps.push(Swap {
                quote: quote.clone(),
                witnesses,
            })
        }
    }

    swaps
}

fn process<L: LiquidityProvider>(
    provider: &L,
    swaps: &[Swap],
    network: Network,
) -> Vec<LocalEvent> {
    if swaps.is_empty() {
        return vec![];
    }

    let mut liquidity = MemoryLiquidityProvider::new();
    liquidity.populate(provider);

    let mut transactions: Vec<LocalEvent> = vec![];

    for swap in swaps.iter() {
        match logic::process_swap(&liquidity, &swap.quote, &swap.witnesses, network) {
            Ok(result) => {
                // Update liquidity
                for tx in result.pool_changes {
                    liquidity
                        .update_liquidity(&tx)
                        .expect("Failed to update liquidity in a swap!");

                    transactions.push(LocalEvent::PoolChange(tx));
                }

                transactions.push(LocalEvent::Output(result.output));
            }
            // On an error we can just log and try again later
            Err(err) => error!("Failed to process swap {:?}. {}", swap, err),
        };
    }

    transactions
}

/// Process all pending swaps
pub fn process_swaps<T: TransactionProvider>(provider: &mut Arc<RwLock<T>>, network: Network) {
    provider.write().sync();

    let swaps = get_swaps(&*provider.read());

    let events = process(&*provider.read(), &swaps, network);

    if events.len() > 0 {
        provider
            .write()
            .add_local_events(events)
            .expect("Failed to add processed swap events");
    }
}

#[cfg(test)]
mod test {
    use crate::{
        common::{GenericCoinAmount, Liquidity, PoolCoin},
        local_store::{LocalEvent, MemoryLocalStore},
        utils::test_utils::data::TestData,
        vault::transactions::MemoryTransactionsProvider,
    };
    use chainflip_common::types::{
        chain::{Output, OutputParent},
        coin::Coin,
    };
    use std::sync::{Arc, Mutex};

    use super::*;

    struct Runner {
        local_store: Arc<Mutex<MemoryLocalStore>>,
        provider: Arc<RwLock<MemoryTransactionsProvider<MemoryLocalStore>>>,
    }

    impl Runner {
        fn new() -> Self {
            let local_store = MemoryLocalStore::new();
            let local_store = Arc::new(Mutex::new(local_store));
            let provider = MemoryTransactionsProvider::new_protected(local_store.clone());

            Self {
                local_store,
                provider,
            }
        }

        fn sync_provider(&mut self) {
            self.provider.write().sync();
        }
    }

    fn get_witness(quote: &SwapQuote, amount: u128) -> Witness {
        TestData::witness(quote.unique_id(), amount, quote.input)
    }

    fn to_atomic(coin: Coin, amount: &str) -> u128 {
        GenericCoinAmount::from_decimal_string(coin, amount).to_atomic()
    }

    #[test]
    fn get_swaps_returns_pending_swaps() {
        let mut runner = Runner::new();

        let quote_with_no_witnesses = TestData::swap_quote(Coin::ETH, Coin::LOKI);
        let quote_with_witnesses = TestData::swap_quote(Coin::BTC, Coin::LOKI);

        let first_witness = get_witness(&quote_with_witnesses, 100);
        let second_witness = get_witness(&quote_with_witnesses, 200);

        runner
            .local_store
            .lock()
            .unwrap()
            .add_events(vec![
                quote_with_no_witnesses.into(),
                quote_with_witnesses.clone().into(),
                first_witness.clone().into(),
                second_witness.clone().into(),
            ])
            .unwrap();

        runner.sync_provider();

        let swaps = get_swaps(&*runner.provider.read());
        assert_eq!(swaps.len(), 1);

        let swap = swaps.first().unwrap();
        assert_eq!(swap.quote.inner, quote_with_witnesses);
        assert_eq!(swap.witnesses.len(), 2);
        assert_eq!(swap.witnesses, vec![first_witness, second_witness]);
    }

    #[test]
    fn get_swaps_does_not_return_hard_expired_quotes() {
        let mut runner = Runner::new();

        let mut quote = TestData::swap_quote(Coin::ETH, Coin::LOKI);
        quote.timestamp = Timestamp(0);

        let witness = get_witness(&quote, 100);

        runner
            .local_store
            .lock()
            .unwrap()
            .add_events(vec![quote.into(), witness.into()])
            .unwrap();

        runner.sync_provider();

        let swaps = get_swaps(&*runner.provider.read());
        assert_eq!(swaps.len(), 0);
    }

    #[test]
    fn get_swaps_does_not_return_invalid_witness_transactions() {
        let mut runner = Runner::new();

        let quote = TestData::swap_quote(Coin::ETH, Coin::LOKI);
        let mut witness = get_witness(&quote, 100);
        witness.coin = quote.output; // Witness coin must match quote input

        runner
            .local_store
            .lock()
            .unwrap()
            .add_events(vec![quote.into(), witness.into()])
            .unwrap();

        runner.sync_provider();

        let swaps = get_swaps(&*runner.provider.read());
        assert_eq!(swaps.len(), 0);
    }

    #[test]
    fn get_swaps_does_not_return_processed_witnesses() {
        let mut runner = Runner::new();

        let quote = TestData::swap_quote(Coin::ETH, Coin::LOKI);
        let unused = get_witness(&quote, 100);
        let used = get_witness(&quote, 150);

        let output = Output {
            parent: OutputParent::SwapQuote(quote.unique_id()),
            witnesses: vec![used.unique_id()],
            pool_changes: vec![],
            coin: quote.output,
            address: quote.output_address.clone(),
            amount: 200,
            event_number: None,
        };

        runner
            .local_store
            .lock()
            .unwrap()
            .add_events(vec![
                quote.into(),
                unused.clone().into(),
                used.into(),
                output.into(),
            ])
            .unwrap();

        runner.sync_provider();

        let swaps = get_swaps(&*runner.provider.read());
        assert_eq!(swaps.len(), 1);

        let swap = swaps.first().unwrap();
        assert_eq!(swap.witnesses, vec![unused]);
    }

    #[test]
    fn process_returns_refunds() {
        let provider = MemoryLiquidityProvider::new();

        let quote = TestData::swap_quote(Coin::ETH, Coin::LOKI);
        let witness = get_witness(&quote, 100);

        // Refund should occur here because we have no liquidity
        let swap = Swap {
            quote: FulfilledWrapper {
                inner: quote.clone(),
                fulfilled: false,
            },
            witnesses: vec![witness],
        };

        let txs = process(&provider, &[swap], Network::Testnet);
        assert_eq!(txs.len(), 1);

        if let LocalEvent::Output(output) = txs.first().unwrap() {
            assert_eq!(output.coin, quote.input);
            assert_eq!(output.amount, 100);
        } else {
            panic!("Expected to get an output transaction");
        }
    }

    #[test]
    fn process_returns_correct_transactions() {
        let mut provider = MemoryLiquidityProvider::new();
        provider.set_liquidity(
            PoolCoin::ETH,
            Some(Liquidity::new(
                to_atomic(Coin::ETH, "10000.0"),
                to_atomic(Coin::LOKI, "20000.0"),
            )),
        );

        let quote = TestData::swap_quote(Coin::ETH, Coin::LOKI);
        assert_eq!(quote.input, Coin::ETH);
        assert_eq!(quote.output, Coin::LOKI);

        let witness = get_witness(&quote, to_atomic(Coin::ETH, "2500.0"));

        let swap = Swap {
            quote: FulfilledWrapper {
                inner: quote,
                fulfilled: false,
            },
            witnesses: vec![witness],
        };

        let txs = process(&provider, &[swap], Network::Testnet);
        assert_eq!(txs.len(), 2);

        match txs.first().unwrap() {
            LocalEvent::PoolChange(_) => {}
            tx @ _ => panic!("Expected to find pool change transaction. Found: {:?}", tx),
        }

        match txs.last().unwrap() {
            LocalEvent::Output(_) => {}
            tx @ _ => panic!("Expected to find output transaction. Found {:?}", tx),
        }
    }

    #[test]
    fn process_correctly_updates_liquidity() {
        let mut provider = MemoryLiquidityProvider::new();

        let eth_liquidity = Liquidity::new(
            to_atomic(Coin::ETH, "10000.0"),
            to_atomic(Coin::LOKI, "20000.0"),
        );

        let btc_liquidity = Liquidity::new(
            to_atomic(Coin::BTC, "10000.0"),
            to_atomic(Coin::LOKI, "20000.0"),
        );

        provider.set_liquidity(PoolCoin::ETH, Some(eth_liquidity));
        provider.set_liquidity(PoolCoin::BTC, Some(btc_liquidity));

        let quote = TestData::swap_quote(Coin::ETH, Coin::LOKI);
        assert_eq!(quote.input, Coin::ETH);
        assert_eq!(quote.output, Coin::LOKI);

        let another = TestData::swap_quote(Coin::ETH, Coin::LOKI);
        assert_eq!(quote.input, Coin::ETH);
        assert_eq!(quote.output, Coin::LOKI);

        let btc_quote = TestData::swap_quote(Coin::BTC, Coin::LOKI);

        let swaps = vec![
            Swap {
                quote: FulfilledWrapper {
                    inner: quote.clone(),
                    fulfilled: false,
                },
                witnesses: vec![get_witness(&quote, to_atomic(Coin::ETH, "2500.0"))],
            },
            Swap {
                quote: FulfilledWrapper {
                    inner: another.clone(),
                    fulfilled: false,
                },
                witnesses: vec![get_witness(&another, to_atomic(Coin::ETH, "2500.0"))],
            },
            Swap {
                quote: FulfilledWrapper {
                    inner: btc_quote.clone(),
                    fulfilled: false,
                },
                witnesses: vec![get_witness(&btc_quote, to_atomic(Coin::BTC, "2500.0"))],
            },
        ];

        let txs = process(&provider, &swaps, Network::Testnet);
        assert_eq!(txs.len(), 6);

        let expected_first_amount = 3199500000000;
        let expected_second_amount = 2332902777777;
        let expected_third_amount = 3199500000000;

        if let LocalEvent::Output(first_output) = txs.get(1).unwrap() {
            assert_eq!(first_output.coin, Coin::LOKI);
            assert_eq!(first_output.amount, expected_first_amount);
        } else {
            panic!("Expected to get an output transaction");
        }

        if let LocalEvent::Output(second_output) = txs.get(3).unwrap() {
            assert_eq!(second_output.coin, Coin::LOKI);
            assert_ne!(
                second_output.amount, expected_first_amount,
                "Expected liquidity to update between swap process"
            );
            assert_eq!(second_output.amount, expected_second_amount);
        } else {
            panic!("Expected to get an output transaction");
        }

        if let LocalEvent::Output(third_output) = txs.get(5).unwrap() {
            assert_eq!(third_output.coin, Coin::LOKI);
            assert_eq!(
                third_output.amount, expected_third_amount,
                "Expected liquidity to not be affected for BTC quote"
            );
        } else {
            panic!("Expected to get an output transaction");
        }

        // Ensure original liquidity is not affected
        assert_eq!(
            provider.get_liquidity(PoolCoin::ETH),
            Some(eth_liquidity),
            "Original liquidity was affected"
        );
        assert_eq!(
            provider.get_liquidity(PoolCoin::BTC),
            Some(btc_liquidity),
            "Original liquidity was affected"
        )
    }

    #[test]
    fn process_swaps_correctly_updates_local_store() {
        let mut runner = Runner::new();

        let quote = TestData::swap_quote(Coin::ETH, Coin::LOKI);
        assert_eq!(quote.input, Coin::ETH);
        assert_eq!(quote.output, Coin::LOKI);

        let witness = get_witness(&quote, to_atomic(Coin::ETH, "2500.0"));

        let initial_eth_depth = to_atomic(Coin::ETH, "10000.0");
        let initial_loki_depth = to_atomic(Coin::LOKI, "20000.0");

        let initial_pool = TestData::pool_change(
            Coin::ETH,
            initial_eth_depth as i128,
            initial_loki_depth as i128,
        );
        runner
            .local_store
            .lock()
            .unwrap()
            .add_events(vec![initial_pool.into(), quote.into(), witness.into()])
            .unwrap();

        runner.sync_provider();

        // Pre conditions
        assert_eq!(runner.provider.read().get_swap_quotes().len(), 1);
        assert_eq!(runner.provider.read().get_witnesses().len(), 1);
        assert_eq!(runner.provider.read().get_outputs().len(), 0);
        assert_eq!(
            runner.provider.read().get_liquidity(PoolCoin::ETH),
            Some(Liquidity::new(initial_eth_depth, initial_loki_depth))
        );

        process_swaps(&mut runner.provider, Network::Testnet);

        let new_eth_depth = to_atomic(Coin::ETH, "12500.0");
        let new_loki_depth = to_atomic(Coin::LOKI, "16800.5");

        // Post conditions
        assert_eq!(runner.provider.read().get_swap_quotes().len(), 1);
        assert_eq!(runner.provider.read().get_witnesses().len(), 1);
        assert_eq!(runner.provider.read().get_outputs().len(), 1);
        assert_eq!(
            runner.provider.read().get_liquidity(PoolCoin::ETH),
            Some(Liquidity::new(new_eth_depth, new_loki_depth))
        );
    }
}
