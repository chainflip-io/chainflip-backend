mod logic;
mod refund;

use std::sync::Arc;

use parking_lot::RwLock;

use crate::{
    common::liquidity_provider::LiquidityProvider,
    common::liquidity_provider::MemoryLiquidityProvider, common::Timestamp,
    constants::SWAP_QUOTE_HARD_EXPIRE, side_chain::SideChainTx, transactions::QuoteTx,
    transactions::WitnessTx, vault::transactions::memory_provider::FulfilledTxWrapper,
    vault::transactions::memory_provider::WitnessTxWrapper,
    vault::transactions::TransactionProvider,
};

#[derive(Debug)]
struct Swap {
    quote: FulfilledTxWrapper<QuoteTx>,
    witnesses: Vec<WitnessTx>,
}

fn get_swaps<T: TransactionProvider>(provider: &T) -> Vec<Swap> {
    let now = Timestamp::now();
    let is_hard_expired = |quote: &QuoteTx| now.0 - quote.timestamp.0 >= SWAP_QUOTE_HARD_EXPIRE;

    let quotes: Vec<&FulfilledTxWrapper<QuoteTx>> = provider
        .get_quote_txs()
        .iter()
        .filter(|tx| !is_hard_expired(&tx.inner))
        .collect();

    let witnesses: Vec<&WitnessTxWrapper> = provider
        .get_witness_txs()
        .iter()
        .filter(|tx| !tx.used)
        .collect();

    let mut swaps: Vec<Swap> = vec![];

    for quote in quotes {
        let witnesses: Vec<WitnessTx> = witnesses
            .iter()
            .filter(|tx| tx.inner.quote_id == quote.inner.id && tx.inner.coin == quote.inner.input)
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

fn process<L: LiquidityProvider>(provider: &L, swaps: &[Swap]) -> Vec<SideChainTx> {
    if swaps.is_empty() {
        return vec![];
    }

    let mut liquidity = MemoryLiquidityProvider::new();
    liquidity.populate(provider);

    let mut transactions: Vec<SideChainTx> = vec![];

    for swap in swaps.iter() {
        match logic::process_swap(&liquidity, &swap.quote, &swap.witnesses) {
            Ok(result) => {
                // Update liquidity
                for tx in result.pool_changes {
                    liquidity
                        .update_liquidity(&tx)
                        .expect("Failed to update liquidity in a swap!");

                    transactions.push(SideChainTx::PoolChangeTx(tx));
                }

                transactions.push(SideChainTx::OutputTx(result.output));
            }
            // On an error we can just log and try again later
            Err(err) => error!("Failed to process swap {:?}. {}", swap, err),
        };
    }

    transactions
}

/// Process all pending swaps
pub fn process_swaps<T: TransactionProvider>(provider: &mut Arc<RwLock<T>>) {
    provider.write().sync();

    let swaps = get_swaps(&*provider.read());

    let transactions = process(&*provider.read(), &swaps);

    if transactions.len() > 0 {
        provider
            .write()
            .add_transactions(transactions)
            .expect("Failed to add processed swap transactions");
    }
}

#[cfg(test)]
mod test {
    use std::sync::{Arc, Mutex};

    use uuid::Uuid;

    use crate::{
        common::{Coin, GenericCoinAmount, Liquidity, PoolCoin, WalletAddress},
        side_chain::{ISideChain, MemorySideChain},
        transactions::{OutputTx, PoolChangeTx},
        utils::test_utils::create_fake_quote_tx_eth_loki,
        vault::transactions::MemoryTransactionsProvider,
    };

    use super::*;

    struct Runner {
        side_chain: Arc<Mutex<MemorySideChain>>,
        provider: Arc<RwLock<MemoryTransactionsProvider<MemorySideChain>>>,
    }

    impl Runner {
        fn new() -> Self {
            let side_chain = MemorySideChain::new();
            let side_chain = Arc::new(Mutex::new(side_chain));
            let provider = MemoryTransactionsProvider::new_protected(side_chain.clone());

            Self {
                side_chain,
                provider,
            }
        }

        fn sync_provider(&mut self) {
            self.provider.write().sync();
        }
    }

    fn get_witness(quote: &QuoteTx, amount: u128) -> WitnessTx {
        WitnessTx::new(
            Timestamp::now(),
            quote.id,
            Uuid::new_v4().to_string(),
            0,
            0,
            amount,
            quote.input,
        )
    }

    fn to_atomic(coin: Coin, amount: &str) -> u128 {
        GenericCoinAmount::from_decimal_string(coin, amount).to_atomic()
    }

    #[test]
    fn get_swaps_returns_pending_swaps() {
        let mut runner = Runner::new();

        let quote_with_no_witnesses = create_fake_quote_tx_eth_loki();
        let quote_with_witnesses = create_fake_quote_tx_eth_loki();

        let first_witness = get_witness(&quote_with_witnesses, 100);
        let second_witness = get_witness(&quote_with_witnesses, 200);

        runner
            .side_chain
            .lock()
            .unwrap()
            .add_block(vec![
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

        let mut quote = create_fake_quote_tx_eth_loki();
        quote.timestamp = Timestamp(0);

        let witness = get_witness(&quote, 100);

        runner
            .side_chain
            .lock()
            .unwrap()
            .add_block(vec![quote.into(), witness.into()])
            .unwrap();

        runner.sync_provider();

        let swaps = get_swaps(&*runner.provider.read());
        assert_eq!(swaps.len(), 0);
    }

    #[test]
    fn get_swaps_does_not_return_invalid_witness_transactions() {
        let mut runner = Runner::new();

        let quote = create_fake_quote_tx_eth_loki();
        let mut witness = get_witness(&quote, 100);
        witness.coin = quote.output; // Witness coin must match quote input

        runner
            .side_chain
            .lock()
            .unwrap()
            .add_block(vec![quote.into(), witness.into()])
            .unwrap();

        runner.sync_provider();

        let swaps = get_swaps(&*runner.provider.read());
        assert_eq!(swaps.len(), 0);
    }

    #[test]
    fn get_swaps_does_not_return_processed_witnesses() {
        let mut runner = Runner::new();

        let quote = create_fake_quote_tx_eth_loki();
        let unused = get_witness(&quote, 100);
        let used = get_witness(&quote, 150);

        let output = OutputTx::new(
            Timestamp::now(),
            quote.id,
            vec![used.id],
            vec![],
            quote.output,
            quote.output_address.clone(),
            200,
        )
        .expect("Expected valid output tx");

        runner
            .side_chain
            .lock()
            .unwrap()
            .add_block(vec![
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

        let quote = create_fake_quote_tx_eth_loki();
        let witness = get_witness(&quote, 100);

        // Refund should occur here because we have no liquidity
        let swap = Swap {
            quote: FulfilledTxWrapper {
                inner: quote.clone(),
                fulfilled: false,
            },
            witnesses: vec![witness],
        };

        let txs = process(&provider, &[swap]);
        assert_eq!(txs.len(), 1);

        if let SideChainTx::OutputTx(output) = txs.first().unwrap() {
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

        let quote = create_fake_quote_tx_eth_loki();
        assert_eq!(quote.input, Coin::ETH);
        assert_eq!(quote.output, Coin::LOKI);

        let witness = get_witness(&quote, to_atomic(Coin::ETH, "2500.0"));

        let swap = Swap {
            quote: FulfilledTxWrapper {
                inner: quote,
                fulfilled: false,
            },
            witnesses: vec![witness],
        };

        let txs = process(&provider, &[swap]);
        assert_eq!(txs.len(), 2);

        match txs.first().unwrap() {
            SideChainTx::PoolChangeTx(_) => {}
            tx @ _ => panic!("Expected to find pool change transaction. Found: {:?}", tx),
        }

        match txs.last().unwrap() {
            SideChainTx::OutputTx(_) => {}
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

        let quote = create_fake_quote_tx_eth_loki();
        assert_eq!(quote.input, Coin::ETH);
        assert_eq!(quote.output, Coin::LOKI);

        let another = create_fake_quote_tx_eth_loki();
        assert_eq!(quote.input, Coin::ETH);
        assert_eq!(quote.output, Coin::LOKI);

        let mut btc_quote = create_fake_quote_tx_eth_loki();
        btc_quote.input = Coin::BTC;
        btc_quote.input_address = WalletAddress::new("1FPR9qMV6nikLxKP1MnZG6jh4viqFUs7wV");

        let swaps = vec![
            Swap {
                quote: FulfilledTxWrapper {
                    inner: quote.clone(),
                    fulfilled: false,
                },
                witnesses: vec![get_witness(&quote, to_atomic(Coin::ETH, "2500.0"))],
            },
            Swap {
                quote: FulfilledTxWrapper {
                    inner: another.clone(),
                    fulfilled: false,
                },
                witnesses: vec![get_witness(&another, to_atomic(Coin::ETH, "2500.0"))],
            },
            Swap {
                quote: FulfilledTxWrapper {
                    inner: btc_quote.clone(),
                    fulfilled: false,
                },
                witnesses: vec![get_witness(&btc_quote, to_atomic(Coin::BTC, "2500.0"))],
            },
        ];

        let txs = process(&provider, &swaps);
        assert_eq!(txs.len(), 6);

        let expected_first_amount = 3199500000000;
        let expected_second_amount = 2332902777777;
        let expected_third_amount = 3199500000000;

        if let SideChainTx::OutputTx(first_output) = txs.get(1).unwrap() {
            assert_eq!(first_output.coin, Coin::LOKI);
            assert_eq!(first_output.amount, expected_first_amount);
        } else {
            panic!("Expected to get an output transaction");
        }

        if let SideChainTx::OutputTx(second_output) = txs.get(3).unwrap() {
            assert_eq!(second_output.coin, Coin::LOKI);
            assert_ne!(
                second_output.amount, expected_first_amount,
                "Expected liquidity to update between swap process"
            );
            assert_eq!(second_output.amount, expected_second_amount);
        } else {
            panic!("Expected to get an output transaction");
        }

        if let SideChainTx::OutputTx(third_output) = txs.get(5).unwrap() {
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
    fn process_swaps_correctly_updates_side_chain() {
        let mut runner = Runner::new();

        let quote = create_fake_quote_tx_eth_loki();
        assert_eq!(quote.input, Coin::ETH);
        assert_eq!(quote.output, Coin::LOKI);

        let witness = get_witness(&quote, to_atomic(Coin::ETH, "2500.0"));

        let initial_eth_depth = to_atomic(Coin::ETH, "10000.0");
        let initial_loki_depth = to_atomic(Coin::LOKI, "20000.0");

        let initial_pool = PoolChangeTx::new(
            PoolCoin::ETH,
            initial_loki_depth as i128,
            initial_eth_depth as i128,
        );
        runner
            .side_chain
            .lock()
            .unwrap()
            .add_block(vec![initial_pool.into(), quote.into(), witness.into()])
            .unwrap();

        runner.sync_provider();

        // Pre conditions
        assert_eq!(runner.provider.read().get_quote_txs().len(), 1);
        assert_eq!(runner.provider.read().get_witness_txs().len(), 1);
        assert_eq!(runner.provider.read().get_output_txs().len(), 0);
        assert_eq!(
            runner.provider.read().get_liquidity(PoolCoin::ETH),
            Some(Liquidity::new(initial_eth_depth, initial_loki_depth))
        );

        process_swaps(&mut runner.provider);

        let new_eth_depth = to_atomic(Coin::ETH, "12500.0");
        let new_loki_depth = to_atomic(Coin::LOKI, "16800.5");

        // Post conditions
        assert_eq!(runner.provider.read().get_quote_txs().len(), 1);
        assert_eq!(runner.provider.read().get_witness_txs().len(), 1);
        assert_eq!(runner.provider.read().get_output_txs().len(), 1);
        assert_eq!(
            runner.provider.read().get_liquidity(PoolCoin::ETH),
            Some(Liquidity::new(new_eth_depth, new_loki_depth))
        );
    }
}
