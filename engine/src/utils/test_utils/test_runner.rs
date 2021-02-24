use super::{data::TestData, store::MemoryKVS};
use crate::{
    common::*,
    local_store::{ILocalStore, LocalEvent, MemoryLocalStore},
    vault::{
        processor::{CoinProcessor, ProcessorEvent, SideChainProcessor},
        transactions::{
            memory_provider::{FulfilledWrapper, Portion},
            MemoryTransactionsProvider, TransactionProvider,
        },
    },
};
use chainflip_common::types::{chain::*, coin::Coin, unique_id::GetUniqueId, Network};
use parking_lot::RwLock;
use std::{
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

    /// Confirms witnesses when called. This is used to emulate the
    /// witness confirmer
    pub fn confirm_witnesses(&mut self, witnesses: Vec<Witness>) {
        let mut provider = self.provider.write();
        for witness in witnesses {
            provider.confirm_witness(witness.unique_id()).unwrap();
        }
    }

    /// A helper function that adds a deposit quote and the corresponding witnesses
    /// necessary for the deposit to be registered
    pub fn add_witnessed_deposit_quote(
        &mut self,
        staker_id: &StakerId,
        oxen_amount: OxenAmount,
        other_amount: GenericCoinAmount,
    ) -> DepositQuote {
        let deposit_quote =
            TestData::deposit_quote_for_id(staker_id.to_owned(), other_amount.coin_type());
        let wtx_oxen = TestData::witness(
            deposit_quote.unique_id(),
            oxen_amount.to_atomic(),
            Coin::OXEN,
        );
        let wtx_oxen_id = wtx_oxen.unique_id();
        let wtx_eth = TestData::witness(
            deposit_quote.unique_id(),
            other_amount.to_atomic(),
            other_amount.coin_type(),
        );
        let wtx_eth_id = wtx_eth.unique_id();

        self.add_local_events([
            wtx_oxen.into(),
            wtx_eth.into(),
            deposit_quote.clone().into(),
        ]);

        let mut provider = self.provider.write();
        // confirm the witnesses - emulating witness_confirmer
        provider.confirm_witness(wtx_oxen_id).unwrap();
        provider.confirm_witness(wtx_eth_id).unwrap();

        println!(
            "Witnesses after confirming: {:#?}",
            provider.get_witnesses()
        );

        deposit_quote
    }

    /// Convenience method to find outputs associated with withdraws
    pub fn get_outputs_for_withdraw_request(&self, request: &WithdrawRequest) -> EthDepositOutputs {
        let sent_outputs = self.sent_outputs.lock().unwrap();

        let outputs: Vec<_> = sent_outputs
            .iter()
            .filter(|output| output.parent_id() == request.unique_id())
            .cloned()
            .collect();

        let oxen_output = outputs
            .iter()
            .find(|x| x.coin == Coin::OXEN)
            .expect("Oxen output should exist")
            .clone();
        let eth_output = outputs
            .iter()
            .find(|x| x.coin == Coin::ETH)
            .expect("Eth output should exist")
            .clone();

        EthDepositOutputs {
            oxen_output,
            eth_output,
        }
    }

    /// Convenience method to check liquidity amounts in ETH pool
    pub fn check_eth_liquidity(&mut self, oxen_atomic: u128, eth_atomic: u128) {
        self.provider.write().sync();

        let liquidity = self
            .provider
            .read()
            .get_liquidity(PoolCoin::ETH)
            .expect("liquidity should exist");

        assert_eq!(liquidity.base_depth, oxen_atomic);
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
        println!("Getting portions for staker_id: {:#?}", staker_id);
        let provider = self.provider.read();
        let all_pools = provider.get_portions();
        let pool = all_pools
            .get(&pool)
            .ok_or(format!("Pool should have portions: {}", pool))?;

        println!("Pool portions: {:#?}", pool);
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

    /// get deposit quotes
    pub fn get_deposit_quotes(&self) -> Vec<FulfilledWrapper<DepositQuote>> {
        self.provider.read().get_deposit_quotes().to_vec()
    }
}

/// A helper struct that represents the two outputs that
/// should be generated when unstaking from oxen/eth pool
pub struct EthDepositOutputs {
    /// Oxen output
    pub oxen_output: Output,
    /// Ethereum output
    pub eth_output: Output,
}

fn spin_until_last_seen(receiver: &crossbeam_channel::Receiver<ProcessorEvent>, last_seen: u64) {
    // Long timeout just to make sure a failing test
    let timeout = Duration::from_secs(10);
    println!("Spin until last seen, last_seen: {}", last_seen);
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
