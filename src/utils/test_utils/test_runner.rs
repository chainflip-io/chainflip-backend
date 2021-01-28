use super::{data::TestData, store::MemoryKVS};
use crate::{
    common::*,
    local_store::{ILocalStore, LocalEvent, MemoryLocalStore},
    vault::{
        processor::{CoinProcessor, ProcessorEvent, SideChainProcessor},
        transactions::{memory_provider::Portion, MemoryTransactionsProvider, TransactionProvider},
    },
};
use chainflip_common::types::{chain::*, coin::Coin, Network};
use parking_lot::RwLock;
use std::{
    hint::unreachable_unchecked,
    sync::{Arc, Mutex},
    time::Duration,
};

struct FakeCoinSender {
    /// Store processed outputs here
    processed_txs: Arc<Mutex<Vec<Output>>>,
}

impl FakeCoinSender {
    fn new() -> (Self, Arc<Mutex<Vec<Output>>>) {
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
    async fn process(&self, _coin: Coin, outputs: &[Output]) -> Vec<OutputSent> {
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
    /// Local store
    pub store: Arc<Mutex<MemoryLocalStore>>,
    /// Receiving end of the channel used to shut down the server
    pub receiver: crossbeam_channel::Receiver<ProcessorEvent>,
    /// State provider
    pub provider: Arc<RwLock<MemoryTransactionsProvider<MemoryLocalStore>>>,
    /// Sent outputs are recorded here
    pub sent_outputs: Arc<Mutex<Vec<Output>>>,
}

impl TestRunner {
    /// Create an instance
    pub fn new() -> Self {
        let store = MemoryLocalStore::new();
        let store = Arc::new(Mutex::new(store));
        let provider = MemoryTransactionsProvider::new(store.clone());
        let provider = Arc::new(RwLock::new(provider));

        let (sender, sent_outputs) = FakeCoinSender::new();

        let processor = SideChainProcessor::new(
            Arc::clone(&provider),
            MemoryKVS::new(),
            sender,
            Network::Testnet,
        );

        // Create a channel to receive processor events through
        let (sender, receiver) = crossbeam_channel::unbounded::<ProcessorEvent>();

        processor.start(Some(sender));

        TestRunner {
            store,
            receiver,
            provider,
            sent_outputs,
        }
    }

    /// Add a bunch of local events to the store, for retrieval later on, when consensus is reached
    pub fn add_local_events<L>(&mut self, events: L)
    where
        L: Into<Vec<LocalEvent>>,
    {
        let mut local_store = self.store.lock().unwrap();

        local_store
            .add_events(events.into())
            .expect("Could not add events");

        drop(local_store);

        self.sync();
    }

    /// A helper function that adds a deposit quote and the corresponding witnesses
    /// necessary for the deposit to be registered
    pub fn add_witnessed_deposit_quote(
        &mut self,
        staker_id: &StakerId,
        loki_amount: LokiAmount,
        other_amount: GenericCoinAmount,
    ) -> DepositQuote {
        let deposit_quote =
            TestData::deposit_quote_for_id(staker_id.to_owned(), other_amount.coin_type());
        let wtx_loki = TestData::witness(deposit_quote.id, loki_amount.to_atomic(), Coin::LOKI);
        let wtx_eth = TestData::witness(
            deposit_quote.id,
            other_amount.to_atomic(),
            other_amount.coin_type(),
        );

        println!("adding deposit quote and witnesses");

        self.add_local_events([
            wtx_loki.into(),
            wtx_eth.into(),
            deposit_quote.clone().into(),
        ]);

        println!("added deposit quote and witnesses");

        deposit_quote
    }

    /// Convenience method to find outputs associated with withdraws
    pub fn get_outputs_for_withdraw_request(&self, request: &WithdrawRequest) -> EthDepositOutputs {
        let sent_outputs = self.sent_outputs.lock().unwrap();

        let outputs: Vec<_> = sent_outputs
            .iter()
            .filter(|output| output.parent_id() == request.id)
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

        EthDepositOutputs {
            loki_output,
            eth_output,
        }
    }

    /// Convenience method to check liquidity amounts in ETH pool
    pub fn check_eth_liquidity(&mut self, loki_atomic: u128, eth_atomic: u128) {
        self.provider.write().sync();
        let provider = self.provider.read();
        let ws = provider.get_witnesses();
        let ws: Vec<Witness> = ws.iter().map(|w| w.inner.clone()).collect();
        println!("Getting the witnesses: {:#?}", ws);
        let dqs = provider.get_deposit_quotes();
        println!("Get the deposit quotes: {:#?}", dqs);

        let liquidity = self
            .provider
            .read()
            .get_liquidity(PoolCoin::ETH)
            .expect("liquidity should exist");

        assert_eq!(liquidity.base_depth, loki_atomic);
        assert_eq!(liquidity.depth, eth_atomic);
    }

    /// Convenience method to add a signed withdraw request for `staker_id`
    pub fn add_withdraw_request_for(&mut self, staker: &Staker, pool: PoolCoin) {
        let tx = TestData::withdraw_request_for_staker(staker, pool.get_coin());

        self.add_local_events([tx.into()])
    }

    /// Convenience method to get portions for `staker_id` in `pool`
    pub fn get_portions_for(
        &self,
        staker_id: &StakerId,
        pool: PoolCoin,
    ) -> Result<Portion, String> {
        let provider = self.provider.read();
        let all_pools = provider.get_portions();
        let pool = all_pools
            .get(&pool)
            .ok_or(format!("Pool should have portions: {}", pool))?;

        let portions = pool
            .get(&staker_id)
            .ok_or("No portions for this staker id")?;

        Ok(*portions)
    }

    /// Sync processor
    pub fn sync(&mut self) {
        let last_seen = self.store.lock().unwrap().total_events();
        spin_until_last_seen(&self.receiver, last_seen);
    }
}

/// A helper struct that represents the two outputs that
/// should be generated when unstaking from loki/eth pool
pub struct EthDepositOutputs {
    /// Loki output
    pub loki_output: Output,
    /// Ethereum output
    pub eth_output: Output,
}

fn spin_until_last_seen(receiver: &crossbeam_channel::Receiver<ProcessorEvent>, last_seen: u64) {
    // Long timeout just to make sure a failing test
    let timeout = Duration::from_secs(10);

    loop {
        match receiver.recv_timeout(timeout) {
            Ok(event) => {
                info!("--- received event: {:?}", event);
                let ProcessorEvent::EVENT(seen) = event;
                if seen >= last_seen {
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
