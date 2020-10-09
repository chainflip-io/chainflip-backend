use blockswap::{
    common::{Coin, GenericCoinAmount, LokiAmount, PoolCoin},
    side_chain::{ISideChain, MemorySideChain, SideChainTx},
    transactions::{OutputSentTx, OutputTx},
    utils::test_utils::{self, store::MemoryKVS},
    vault::{
        processor::{CoinProcessor, ProcessorEvent, SideChainProcessor},
        transactions::{MemoryTransactionsProvider, TransactionProvider},
    },
};

use async_trait::async_trait;
use parking_lot::RwLock;

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use log::{error, info};

fn spin_until_block(receiver: &crossbeam_channel::Receiver<ProcessorEvent>, target_idx: u32) {
    // Long timeout just to make sure a failing test
    let timeout = Duration::from_secs(10);

    loop {
        match receiver.recv_timeout(timeout) {
            Ok(event) => {
                info!("--- received event: {:?}", event);
                let ProcessorEvent::BLOCK(idx) = event;
                if idx >= target_idx {
                    break;
                }
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                error!("Channel timeout on receive");
                break;
            }
            Err(err) => {
                panic!("Unexpected channel error: {}", err);
            }
        }
    }
}

fn check_liquidity<T>(
    tx_provider: &mut T,
    coin_type: Coin,
    loki_amount: LokiAmount,
    coin_amount: GenericCoinAmount,
) where
    T: TransactionProvider,
{
    tx_provider.sync();

    let liquidity = tx_provider
        .get_liquidity(PoolCoin::from(coin_type).unwrap())
        .unwrap();

    // Check that a pool with the right amount was created
    assert_eq!(liquidity.loki_depth, loki_amount.to_atomic());
    assert_eq!(liquidity.depth, coin_amount.to_atomic());
}

struct FakeCoinSender {
    /// Store processed outputs here
    processed_txs: Arc<Mutex<Vec<OutputTx>>>,
}

impl FakeCoinSender {
    fn new() -> (Self, Arc<Mutex<Vec<OutputTx>>>) {
        let processed = Arc::new(Mutex::new(vec![]));

        (
            Self {
                processed_txs: Arc::clone(&processed),
            },
            processed,
        )
    }
}

#[async_trait]
impl CoinProcessor for FakeCoinSender {
    async fn process(&self, _coin: Coin, outputs: &[OutputTx]) -> Vec<OutputSentTx> {
        self.processed_txs
            .lock()
            .unwrap()
            .append(&mut outputs.to_owned());
        vec![]
    }
}

struct TestRunner {
    chain: Arc<Mutex<MemorySideChain>>,
    receiver: crossbeam_channel::Receiver<ProcessorEvent>,
    provider: MemoryTransactionsProvider<MemorySideChain>,
    sent_outputs: Arc<Mutex<Vec<OutputTx>>>,
}

impl TestRunner {
    fn new() -> Self {
        let chain = MemorySideChain::new();
        let chain = Arc::new(Mutex::new(chain));

        let provider = MemoryTransactionsProvider::new(chain.clone());
        let provider = Arc::new(RwLock::new(provider));

        let (sender, sent_outputs) = FakeCoinSender::new();

        let processor = SideChainProcessor::new(provider, MemoryKVS::new(), sender);

        // Create a channel to receive processor events through
        let (sender, receiver) = crossbeam_channel::unbounded::<ProcessorEvent>();

        processor.start(Some(sender));

        // We are not super concerned about keeping 2 tx providers around, because
        // we don't want to require thread safety in production, and having another
        // instance it is cheap enough for tests
        let provider = MemoryTransactionsProvider::new(chain.clone());

        TestRunner {
            chain,
            receiver,
            provider,
            sent_outputs,
        }
    }

    fn add_block<T>(&mut self, block: T)
    where
        T: Into<Vec<SideChainTx>>,
    {
        let mut chain = self.chain.lock().unwrap();

        chain
            .add_block(block.into())
            .expect("Could not add transactions");
    }

    /// Sync processor
    fn sync(&mut self) {
        let total_blocks = self.chain.lock().unwrap().total_blocks();

        if total_blocks > 0 {
            let last_block = total_blocks.checked_sub(1).unwrap();
            spin_until_block(&self.receiver, last_block);
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use test_utils::*;

    #[test]
    fn witnessed_staked_changes_pool_liquidity() {
        let mut runner = TestRunner::new();

        let coin_type = Coin::ETH;
        let loki_amount = LokiAmount::from_decimal_string("1.0");
        let coin_amount = GenericCoinAmount::from_decimal_string(coin_type, "2.0");

        let stake_tx = create_fake_stake_quote(loki_amount, coin_amount);
        let wtx_loki = create_fake_witness(&stake_tx, loki_amount, Coin::LOKI);
        let wtx_eth = create_fake_witness(&stake_tx, coin_amount, coin_type);

        runner.add_block([stake_tx.clone().into()]);
        runner.add_block([wtx_loki.into(), wtx_eth.into()]);

        runner.sync();

        check_liquidity(&mut runner.provider, coin_type, loki_amount, coin_amount);

        runner.add_block([stake_tx.clone().into()]);

        runner.sync();

        // Check that the balance has not changed
        check_liquidity(&mut runner.provider, coin_type, loki_amount, coin_amount);
    }

    #[test]
    fn unstake_transactions() {
        test_utils::logging::init();

        let mut runner = TestRunner::new();

        // 1. Make a Stake TX and make sure it is acknowledged

        let coin_type = Coin::ETH;
        let loki_amount = LokiAmount::from_decimal_string("1.0");
        let coin_amount = GenericCoinAmount::from_decimal_string(coin_type, "2.0");

        let stake_tx = create_fake_stake_quote(loki_amount, coin_amount);
        let wtx_loki = create_fake_witness(&stake_tx, loki_amount, Coin::LOKI);
        let wtx_eth = create_fake_witness(&stake_tx, coin_amount, coin_type);

        // Add blocks with those transactions
        runner.add_block([stake_tx.clone().into()]);
        runner.add_block([wtx_loki.into(), wtx_eth.into()]);

        runner.sync();

        check_liquidity(&mut runner.provider, coin_type, loki_amount, coin_amount);

        // 2. Add an unstake request

        let unstake_tx = create_fake_unstake_request_tx(stake_tx.staker_id);

        runner.add_block([unstake_tx.into()]);

        runner.sync();
    }

    #[test]
    fn multiple_stakes() {
        test_utils::logging::init();

        let mut runner = TestRunner::new();

        // 1. Make a Stake TX and make sure it is acknowledged

        let coin_type = Coin::ETH;
        let loki_amount = LokiAmount::from_decimal_string("1.0");
        let coin_amount = GenericCoinAmount::from_decimal_string(coin_type, "2.0");

        let stake_tx = create_fake_stake_quote(loki_amount, coin_amount);
        let wtx_loki = create_fake_witness(&stake_tx, loki_amount, Coin::LOKI);
        let wtx_eth = create_fake_witness(&stake_tx, coin_amount, coin_type);

        // Add blocks with those transactions
        runner.add_block([stake_tx.clone().into()]);
        runner.add_block([wtx_loki.into(), wtx_eth.into()]);

        runner.sync();

        check_liquidity(&mut runner.provider, coin_type, loki_amount, coin_amount);

        // 2. Add another stake with another staker id

        let stake_tx = create_fake_stake_quote(loki_amount, coin_amount);
        let wtx_loki = create_fake_witness(&stake_tx, loki_amount, Coin::LOKI);
        let wtx_eth = create_fake_witness(&stake_tx, coin_amount, coin_type);

        runner.add_block([stake_tx.clone().into()]);
        runner.add_block([wtx_loki.into(), wtx_eth.into()]);

        runner.sync();
    }
}
