use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use parking_lot::RwLock;

use crate::{
    common::*,
    side_chain::{MemorySideChain, ISideChain, SideChainTx},
    transactions::*,
    vault::processor::{SideChainProcessor, CoinProcessor, ProcessorEvent},
    vault::transactions::{TransactionProvider, MemoryTransactionsProvider},
};

use super::{create_fake_witness, fake_txs::create_fake_stake_quote_for_id, store::MemoryKVS};

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

/// A wrapper around sidechain/provider used in tests with handy
/// shortcuts for staking etc.
pub struct TestRunner {
    /// Sidechain
    pub chain: Arc<Mutex<MemorySideChain>>,
    /// Receiving end of the channel used to shut down the server
    pub receiver: crossbeam_channel::Receiver<ProcessorEvent>,
    /// State provider
    pub provider: Arc<RwLock<MemoryTransactionsProvider<MemorySideChain>>>,
    /// Sent outputs are recorded here
    pub sent_outputs: Arc<Mutex<Vec<OutputTx>>>,
}

impl TestRunner {
    /// Create an instance
    pub fn new() -> Self {
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

    /// Add `block` to the blockchain
    pub fn add_block<T>(&mut self, block: T)
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
    pub fn add_witnessed_stake_tx(
        &mut self,
        staker_id: &StakerId,
        loki_amount: LokiAmount,
        other_amount: GenericCoinAmount,
    ) -> StakeQuoteTx {
        let stake_tx = create_fake_stake_quote_for_id(
            staker_id.to_owned(),
            PoolCoin::from(other_amount.coin_type()).unwrap(),
        );
        let wtx_loki = create_fake_witness(&stake_tx, loki_amount, Coin::LOKI);
        let wtx_eth = create_fake_witness(&stake_tx, other_amount, other_amount.coin_type());

        self.add_block([stake_tx.clone().into()]);
        self.add_block([wtx_loki.into(), wtx_eth.into()]);

        stake_tx
    }

    /// Convenience method to find outputs associated with `tx` unstake
    pub fn get_outputs_for_unstake(&self, tx: &UnstakeRequestTx) -> EthStakeOutputs {
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

    /// Convenience method to check liquidity amounts in ETH pool
    pub fn check_eth_liquidity(&mut self, loki_atomic: u128, eth_atomic: u128) {
        self.provider.write().sync();

        let liquidity = self
            .provider
            .read()
            .get_liquidity(PoolCoin::ETH)
            .expect("liquidity should exist");

        assert_eq!(liquidity.loki_depth, loki_atomic);
        assert_eq!(liquidity.depth, eth_atomic);
    }

    /// Sync processor
    pub fn sync(&mut self) {
        let total_blocks = self.chain.lock().unwrap().total_blocks();

        if total_blocks > 0 {
            let last_block = total_blocks.checked_sub(1).unwrap();
            spin_until_block(&self.receiver, last_block);
        }
    }
}

/// A helper struct that represents the two outputs that
/// should be generated when unstaking from loki/eth pool
pub struct EthStakeOutputs {
    /// Loki output
    pub loki_output: OutputTx,
    /// Ethereum output
    pub eth_output: OutputTx,
}

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
