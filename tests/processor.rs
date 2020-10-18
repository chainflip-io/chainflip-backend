use blockswap::{
    common::{Coin, GenericCoinAmount, LokiAmount, PoolCoin},
    side_chain::{ISideChain, MemorySideChain, SideChainTx},
    transactions::{OutputSentTx, OutputTx, UnstakeRequestTx},
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

#[cfg(test)]
mod tests {

    struct TestRunner {
        chain: Arc<Mutex<MemorySideChain>>,
        receiver: crossbeam_channel::Receiver<ProcessorEvent>,
        provider: Arc<RwLock<MemoryTransactionsProvider<MemorySideChain>>>,
        sent_outputs: Arc<Mutex<Vec<OutputTx>>>,
    }

    /// A helper struct that represents the two outputs that
    /// should be generated when unstaking from loki/eth pool
    struct EthStakeOutputs {
        loki_output: OutputTx,
        eth_output: OutputTx,
    }

    impl TestRunner {
        fn new() -> Self {
            let chain = MemorySideChain::new();
            let chain = Arc::new(Mutex::new(chain));

            let provider = MemoryTransactionsProvider::new(chain.clone());
            let provider = Arc::new(RwLock::new(provider));

            let (sender, sent_outputs) = FakeCoinSender::new();

            let processor = SideChainProcessor::new(Arc::clone(&provider), MemoryKVS::new(), sender);

            // Create a channel to receive processor events through
            let (sender, receiver) = crossbeam_channel::unbounded::<ProcessorEvent>();

            processor.start(Some(sender));

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

            drop(chain);

            self.sync();
        }

        /// A helper function that adds a stake quote and the corresponding witness transactions
        /// necessary for the stake to be registered
        fn add_witnessed_stake_tx(
            &mut self,
            staker_id: &str,
            loki_amount: LokiAmount,
            other_amount: GenericCoinAmount,
        ) -> StakeQuoteTx {
            let stake_tx = create_fake_stake_quote_for_id(staker_id, loki_amount, other_amount);
            let wtx_loki = create_fake_witness(&stake_tx, loki_amount, Coin::LOKI);
            let wtx_eth = create_fake_witness(&stake_tx, other_amount, other_amount.coin_type());

            self.add_block([stake_tx.clone().into()]);
            self.add_block([wtx_loki.into(), wtx_eth.into()]);

            stake_tx
        }

        fn get_outputs_for_unstake(&self, tx: &UnstakeRequestTx) -> EthStakeOutputs {
            let sent_outputs = self.sent_outputs.lock().unwrap();

            let outputs: Vec<_> = sent_outputs
                .iter()
                .filter(|output| output.quote_tx == tx.id)
                .cloned()
                .collect();

            let loki_output = outputs
                .iter()
                .find(|x| x.coin == Coin::LOKI)
                .expect("Loki output should exist")
                .clone();
            let eth_output = outputs
                .iter()
                .find(|x| x.coin == Coin::ETH)
                .expect("Eth output should exist")
                .clone();

            EthStakeOutputs {
                loki_output,
                eth_output,
            }
        }

        fn check_eth_liquidity(&mut self, loki_atomic: u128, eth_atomic: u128) {
            self.provider.write().sync();

            let liquidity = self
                .provider
                .read().get_liquidity(PoolCoin::ETH)
                .expect("liquidity should exist");

            assert_eq!(liquidity.loki_depth, loki_atomic);
            assert_eq!(liquidity.depth, eth_atomic);
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

    use super::*;
    use blockswap::{
        common::liquidity_provider::LiquidityProvider, common::WalletAddress,
        transactions::StakeQuoteTx, utils::test_utils::fake_txs::create_fake_stake_quote_for_id,
    };
    use test_utils::*;

    fn create_unstake_tx(tx: &StakeQuoteTx) -> UnstakeRequestTx {
        let loki_address = WalletAddress::new("T6SMsepawgrKXeFmQroAbuTQMqLWyMxiVUgZ6APCRFgxQAUQ1AkEtHxAgDMZJJG9HMJeTeDsqWiuCMsNahScC7ZS2StC9kHhY");
        let other_address = WalletAddress::new("0x70e7db0678460c5e53f1ffc9221d1c692111dcc5");

        UnstakeRequestTx::new(
            tx.coin_type,
            tx.staker_id.clone(),
            loki_address,
            other_address,
        )
    }

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

        check_liquidity(&mut *runner.provider.write(), coin_type, loki_amount, coin_amount);

        runner.add_block([stake_tx.clone().into()]);

        // Check that the balance has not changed
        check_liquidity(&mut *runner.provider.write(), coin_type, loki_amount, coin_amount);
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

        check_liquidity(&mut *runner.provider.write(), coin_type, loki_amount, coin_amount);

        // 2. Add another stake with another staker id

        let stake_tx = create_fake_stake_quote(loki_amount, coin_amount);
        let wtx_loki = create_fake_witness(&stake_tx, loki_amount, Coin::LOKI);
        let wtx_eth = create_fake_witness(&stake_tx, coin_amount, coin_type);

        runner.add_block([stake_tx.clone().into()]);
        runner.add_block([wtx_loki.into(), wtx_eth.into()]);
    }

    #[test]
    fn sole_staker_can_unstake_all() {
        test_utils::logging::init();

        let mut runner = TestRunner::new();

        let loki_amount = LokiAmount::from_decimal_string("1.0");
        let eth_amount = GenericCoinAmount::from_decimal_string(Coin::ETH, "2.0");

        let stake_tx = runner.add_witnessed_stake_tx("Alice", loki_amount, eth_amount);

        // Check that the liquidity is non-zero before unstaking
        runner.check_eth_liquidity(loki_amount.to_atomic(), eth_amount.to_atomic());

        let unstake_tx = create_unstake_tx(&stake_tx);

        runner.add_block([unstake_tx.clone().into()]);

        // Check that outputs have been payed out
        let outputs = runner.get_outputs_for_unstake(&unstake_tx);

        assert_eq!(outputs.loki_output.amount, loki_amount.to_atomic());
        assert_eq!(outputs.eth_output.amount, eth_amount.to_atomic());

        // Check that liquidity is 0 after unstaking. (Is this even a valid state???)
        runner.check_eth_liquidity(0, 0);
    }

    #[test]
    fn half_staker_can_unstake_half() {
        test_utils::logging::init();

        let mut runner = TestRunner::new();

        let loki_amount = LokiAmount::from_decimal_string("1.0");
        let eth_amount = GenericCoinAmount::from_decimal_string(Coin::ETH, "2.0");

        let _ = runner.add_witnessed_stake_tx("Alice", loki_amount, eth_amount);
        let stake2 = runner.add_witnessed_stake_tx("Bob", loki_amount, eth_amount);

        // Check that liquidity is the sum of two stakes
        runner.check_eth_liquidity(loki_amount.to_atomic() * 2, eth_amount.to_atomic() * 2);

        let unstake_tx = create_unstake_tx(&stake2);
        runner.add_block([unstake_tx.clone().into()]);

        // Check that outputs have been payed out
        let outputs = runner.get_outputs_for_unstake(&unstake_tx);

        assert_eq!(outputs.loki_output.amount, loki_amount.to_atomic());
        assert_eq!(outputs.eth_output.amount, eth_amount.to_atomic());

        // Check that liquidity halved
        runner.check_eth_liquidity(loki_amount.to_atomic(), eth_amount.to_atomic());
    }

    #[test]
    fn non_staker_cannot_unstake() {
        test_utils::logging::init();

        let mut runner = TestRunner::new();

        let loki_amount = LokiAmount::from_decimal_string("1.0");
        let eth_amount = GenericCoinAmount::from_decimal_string(Coin::ETH, "2.0");

        let _ = runner.add_witnessed_stake_tx("Alice", loki_amount, eth_amount);

        // Bob creates a stake quote tx, but never pays the amounts:
        let stake = create_fake_stake_quote_for_id("Bob", loki_amount, eth_amount);
        runner.add_block([stake.clone().into()]);

        // Bob tries to unstake:
        let unstake_tx = create_unstake_tx(&stake);
        runner.add_block([unstake_tx.clone().into()]);

        // Check that no outputs are created:
        let sent_outputs = runner.sent_outputs.lock().unwrap();

        let outputs = sent_outputs
            .iter()
            .filter(|output| output.quote_tx == unstake_tx.id)
            .count();

        assert_eq!(outputs, 0);
    }
}
