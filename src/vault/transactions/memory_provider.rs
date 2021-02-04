use crate::{
    common::{
        liquidity_provider::{Liquidity, LiquidityProvider, MemoryLiquidityProvider},
        GenericCoinAmount, LokiAmount, PoolCoin, StakerId,
    },
    local_store::{ILocalStore, LocalEvent},
    vault::transactions::{
        portions::{adjust_portions_after_deposit, DepositContribution},
        TransactionProvider,
    },
};
use chainflip_common::types::{chain::*, unique_id::GetUniqueId};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fmt,
    str::FromStr,
    sync::{Arc, Mutex},
};

use super::portions::{adjust_portions_after_withdraw, Withdrawal};

/// Transaction plus a boolean flag
#[derive(Debug, Clone, PartialEq)]
pub struct FulfilledWrapper<Q: PartialEq> {
    /// The actual transaction
    pub inner: Q,
    /// Whether the transaction has been fulfilled (i.e. there
    /// is a matching "outcome" tx on the side chain)
    pub fulfilled: bool,
}

impl<Q: PartialEq> FulfilledWrapper<Q> {
    /// Constructor
    pub fn new(inner: Q, fulfilled: bool) -> FulfilledWrapper<Q> {
        FulfilledWrapper { inner, fulfilled }
    }
}

/// Defines the processing stage the witness is in
#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub enum WitnessStatus {
    /// When it has been locally witnessed
    AwaitingConfirmation,
    /// When it has been confirmed by the network, i.e. it's ready for processing
    Confirmed,
    /// After it has been processed. No further action should be taken on this witness
    Processed,
}

impl fmt::Display for WitnessStatus {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl FromStr for WitnessStatus {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            // can come back from the database as Null
            "Null" => Ok(WitnessStatus::AwaitingConfirmation),
            "AwaitingConfirmation" => Ok(WitnessStatus::AwaitingConfirmation),
            "Confirmed" => Ok(WitnessStatus::Confirmed),
            "Processed" => Ok(WitnessStatus::Processed),
            _ => Err(()),
        }
    }
}

/// Witness plus a boolean flag
#[derive(Debug)]
pub struct StatusWitnessWrapper {
    /// The actual transaction
    pub inner: Witness,
    /// Whether the transaction has been used to fulfill some quote
    pub status: WitnessStatus,
}

impl StatusWitnessWrapper {
    /// Construct from internal parts
    pub fn new(inner: Witness, status: WitnessStatus) -> Self {
        StatusWitnessWrapper { inner, status }
    }

    /// Is the witness status Confirmed
    pub fn is_confirmed(&self) -> bool {
        self.status == WitnessStatus::Confirmed
    }
}

/// Integer value used to indicate the how much of the pool's
/// value is associated with a given staker id.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
pub struct Portion(pub u64);

impl Portion {
    /// Value representing 100% ownership
    pub const MAX: Portion = Portion(10_000_000_000u64);

    /// Add checking for overflow
    pub fn checked_add(self, rhs: Portion) -> Option<Portion> {
        let sum = self.0 + rhs.0;

        if sum <= Portion::MAX.0 {
            Some(Portion(sum))
        } else {
            None
        }
    }

    /// Subtract checking for underflow
    pub fn checked_sub(self, rhs: Portion) -> Option<Portion> {
        self.0.checked_sub(rhs.0).map(|x| Portion(x))
    }
}

impl std::ops::Add for Portion {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self(self.0.checked_add(other.0).expect(""))
    }
}

/// Portions in one pool
pub type PoolPortions = HashMap<StakerId, Portion>;
/// Portions in all pools
pub type VaultPortions = HashMap<PoolCoin, PoolPortions>;

/// All state that TransactionProvider will keep in memory
struct MemoryState {
    swap_quotes: Vec<FulfilledWrapper<SwapQuote>>,
    deposit_quotes: Vec<FulfilledWrapper<DepositQuote>>,
    withdraw_requests: Vec<FulfilledWrapper<WithdrawRequest>>,
    withdraws: Vec<Withdraw>,
    deposits: Vec<Deposit>,
    witnesses: Vec<StatusWitnessWrapper>,
    outputs: Vec<FulfilledWrapper<Output>>,
    liquidity: MemoryLiquidityProvider,
    next_event: u64,
    staker_portions: VaultPortions,
}

/// An in-memory transaction provider
pub struct MemoryTransactionsProvider<L: ILocalStore> {
    local_store: Arc<Mutex<L>>,
    state: MemoryState,
}

impl<L: ILocalStore> MemoryTransactionsProvider<L> {
    /// Create an in-memory transaction provider
    pub fn new(local_store: Arc<Mutex<L>>) -> Self {
        let state = MemoryState {
            swap_quotes: vec![],
            deposit_quotes: vec![],
            withdraw_requests: vec![],
            withdraws: vec![],
            deposits: vec![],
            witnesses: vec![],
            outputs: vec![],
            liquidity: MemoryLiquidityProvider::new(),
            next_event: 0,
            staker_portions: HashMap::new(),
        };

        MemoryTransactionsProvider { local_store, state }
    }

    /// Helper constructor to return a wrapped (thread safe) instance
    pub fn new_protected(local_store: Arc<Mutex<L>>) -> Arc<RwLock<Self>> {
        let p = Self::new(local_store);
        Arc::new(RwLock::new(p))
    }
}

/// How much of each coin a given staker owns
/// in coin amounts
#[derive(Debug, Clone)]
pub struct StakerOwnership {
    /// Staker identity
    pub staker_id: StakerId,
    /// Into which pool the contribution is made
    pub pool_type: PoolCoin,
    /// Contribution in Loki
    pub loki: LokiAmount,
    /// Contribution in the other coin
    pub other: GenericCoinAmount,
}

impl MemoryState {
    fn process_deposit(&mut self, tx: Deposit) {
        println!("Processing deposit");
        // Find quote and mark it as fulfilled
        if let Some(quote_info) = self
            .deposit_quotes
            .iter_mut()
            .find(|quote_info| quote_info.inner.unique_id() == tx.quote)
        {
            quote_info.fulfilled = true;
        }

        // println!("The witness statuses for the deposit: {:#?}", &tx.witnesses.iter().map(|w| w.status));
        // Find witnesses and mark them as used:
        for wtx_id in &tx.witnesses {
            if let Some(witness_info) = self
                .witnesses
                .iter_mut()
                .find(|w| &w.inner.unique_id() == wtx_id)
            {
                println!(
                    "Witness status before marking processed: {:#?}",
                    witness_info.status
                );
                witness_info.status = WitnessStatus::Processed;
            }
        }

        // TODO: we need to have the associated PoolChange at this point, but Deposit
        // only has a "uuid reference" to a transactions that we haven't processed yet...
        // What's worse, we've made the assumption that Deposit gets processed first,
        // Because we want to see what the liquidity is like before the contribution was made.

        let contribution = DepositContribution::new(
            StakerId::from_bytes(&tx.staker_id).unwrap(),
            LokiAmount::from_atomic(tx.base_amount),
            GenericCoinAmount::from_atomic(tx.pool, tx.other_amount),
        );

        adjust_portions_after_deposit(
            &mut self.staker_portions,
            &mut self.liquidity.get_pools(),
            &contribution,
        );

        self.deposits.push(tx)
    }

    fn process_pool_change(&mut self, tx: PoolChange) {
        debug!("Processing a pool change tx: {:?}", tx);
        if let Err(err) = self.liquidity.update_liquidity(&tx) {
            error!("Failed to process pool change tx {:?}: {}", tx, err);
            panic!(err);
        }
    }

    fn process_withdraw_request(&mut self, tx: WithdrawRequest) {
        let tx = FulfilledWrapper::new(tx, false);
        self.withdraw_requests.push(tx);
    }

    fn process_withdraw(&mut self, tx: Withdraw) {
        // We must be able to find the request or else we won't be
        // able to adjust portions which might result in double withdraw

        // Find quote and mark it as fulfilled
        let wrapped_withdraw_request = match self
            .withdraw_requests
            .iter_mut()
            .find(|w_withdraw_req| w_withdraw_req.inner.unique_id() == tx.withdraw_request)
        {
            Some(w_withdraw_req) => {
                w_withdraw_req.fulfilled = true;
                w_withdraw_req
            }
            None => panic!(
                "No withdraw request found that matches withdraw request id: {}",
                tx.withdraw_request
            ),
        };

        let withdraw_req = &wrapped_withdraw_request.inner;

        let staker_id = StakerId::from_bytes(&withdraw_req.staker_id).unwrap();
        let pool = PoolCoin::from(*&withdraw_req.pool).unwrap();
        let fraction = *&withdraw_req.fraction;

        let withdrawal = Withdrawal {
            staker_id,
            fraction,
            pool,
        };

        let liquidity = self
            .liquidity
            .get_liquidity(pool)
            .expect("Liquidity must exist for withdrawn coin");

        adjust_portions_after_withdraw(&mut self.staker_portions, &liquidity, withdrawal);

        self.withdraws.push(tx);
    }

    fn process_output_tx(&mut self, tx: Output) {
        // Find quote and mark it as fulfilled only if it's not a refund
        if let Some(quote_info) = self.swap_quotes.iter_mut().find(|quote_info| {
            quote_info.inner.unique_id() == tx.parent_id() && quote_info.inner.output == tx.coin
        }) {
            quote_info.fulfilled = true;
        }

        // Find witnesses and mark them as fulfilled
        let witnesses = self.witnesses.iter_mut().filter(|witness| {
            witness.is_confirmed() && tx.witnesses.contains(&witness.inner.unique_id())
        });

        for witness in witnesses {
            witness.status = WitnessStatus::Processed;
        }

        // Add output tx
        let wrapper = FulfilledWrapper {
            inner: tx,
            fulfilled: false,
        };

        self.outputs.push(wrapper);
    }

    fn process_output_sent_tx(&mut self, tx: OutputSent) {
        // Find output txs and mark them as fulfilled

        // can this be made `.find()` and without the second loop? there should only be one output?
        let outputs = self
            .outputs
            .iter_mut()
            .filter(|output| tx.outputs.contains(&output.inner.unique_id()));

        for output in outputs {
            output.fulfilled = true;
        }
    }

    fn confirm_witness_mem(&mut self, witness_id: u64) {
        let witness = self
            .witnesses
            .iter_mut()
            .find(|e| e.inner.unique_id() == witness_id);

        match witness {
            Some(w) => w.status = WitnessStatus::Confirmed,
            None => {
                println!("Witness does not exist");
                debug!("Witness does not exist");
            }
        }
    }
}

impl<L: ILocalStore> TransactionProvider for MemoryTransactionsProvider<L> {
    // Here we fetch events from the database and put them into memory
    // The core assumption here is that events are not in intermediate stages of processing
    // This is particularly relevant wrt witnesses. We now have the ability to store
    // `status` on events in the db, and retrieve this. Status is currently only updated in memory
    // thus if a witness was `Confirmed` but before it become processed the program crashed, then
    // on restart that witness would be loaded back in as `AwaitingConfirmation`
    // We should change this, it requires a bit of a restructure.
    fn sync(&mut self) -> u64 {
        let local_store = self.local_store.lock().unwrap();
        for evt in local_store.get_events(self.state.next_event) {
            match evt {
                LocalEvent::Witness(evt) => {
                    self.state.witnesses.push(StatusWitnessWrapper::new(
                        evt,
                        WitnessStatus::AwaitingConfirmation,
                    ));
                }
                LocalEvent::SwapQuote(evt) => {
                    // Quotes always come before their corresponding "outcome", so they start unfulfilled
                    let evt = FulfilledWrapper::new(evt, false);
                    self.state.swap_quotes.push(evt);
                }
                LocalEvent::DepositQuote(evt) => {
                    // (same as above)
                    let evt = FulfilledWrapper::new(evt, false);

                    self.state.deposit_quotes.push(evt)
                }
                LocalEvent::PoolChange(evt) => self.state.process_pool_change(evt),
                LocalEvent::Deposit(evt) => self.state.process_deposit(evt),
                LocalEvent::Output(evt) => self.state.process_output_tx(evt),
                LocalEvent::WithdrawRequest(evt) => self.state.process_withdraw_request(evt),
                LocalEvent::Withdraw(evt) => self.state.process_withdraw(evt),
                LocalEvent::OutputSent(evt) => self.state.process_output_sent_tx(evt),
            }
            self.state.next_event += 1
        }

        self.state.next_event
    }

    fn add_local_events(&mut self, events: Vec<LocalEvent>) -> Result<(), String> {
        let valid_events: Vec<_> = events
            .into_iter()
            .filter(|event| {
                if let LocalEvent::Witness(tx) = event {
                    return !self
                        .state
                        .witnesses
                        .iter()
                        .any(|witness| tx == &witness.inner);
                }

                true
            })
            .collect();

        if valid_events.len() > 0 {
            self.local_store.lock().unwrap().add_events(valid_events)?;
        }

        self.sync();
        Ok(())
    }

    fn confirm_witness(&mut self, witness_id: u64) -> Result<(), String> {
        // let mut local_store = self.local_store.lock().unwrap();
        // local_store.set_witness_status(witness_id, WitnessStatus::Confirmed)?;
        // update the in mem version of status
        self.state.confirm_witness_mem(witness_id);
        Ok(())
    }

    fn get_swap_quotes(&self) -> &[FulfilledWrapper<SwapQuote>] {
        &self.state.swap_quotes
    }

    fn get_deposit_quotes(&self) -> &[FulfilledWrapper<DepositQuote>] {
        &self.state.deposit_quotes
    }

    fn get_witnesses(&self) -> &[StatusWitnessWrapper] {
        &self.state.witnesses
    }

    fn get_outputs(&self) -> &[FulfilledWrapper<Output>] {
        &self.state.outputs
    }

    fn get_withdraw_requests(&self) -> &[FulfilledWrapper<WithdrawRequest>] {
        &self.state.withdraw_requests
    }

    fn get_portions(&self) -> &VaultPortions {
        &self.state.staker_portions
    }
}

impl<L: ILocalStore> LiquidityProvider for MemoryTransactionsProvider<L> {
    fn get_liquidity(&self, pool: PoolCoin) -> Option<Liquidity> {
        self.state.liquidity.get_liquidity(pool)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{local_store::MemoryLocalStore, utils::test_utils::data::TestData};
    use chainflip_common::types::coin::Coin;

    fn setup() -> MemoryTransactionsProvider<MemoryLocalStore> {
        let local_store = Arc::new(Mutex::new(MemoryLocalStore::new()));
        MemoryTransactionsProvider::new(local_store)
    }

    #[test]
    fn test_provider() {
        let mut provider = setup();

        assert!(provider.get_swap_quotes().is_empty());
        assert!(provider.get_witnesses().is_empty());

        // Add some random events
        {
            let mut local_store = provider.local_store.lock().unwrap();

            let quote = TestData::swap_quote(Coin::ETH, Coin::LOKI);
            let witness = TestData::witness(quote.unique_id(), 100, Coin::ETH);

            local_store
                .add_events(vec![quote.into(), witness.into()])
                .unwrap();
        }

        provider.sync();

        assert_eq!(provider.state.next_event, 2);
        assert_eq!(provider.get_swap_quotes().len(), 1);
        assert_eq!(provider.get_witnesses().len(), 1);

        provider
            .add_local_events(vec![TestData::swap_quote(Coin::ETH, Coin::BTC).into()])
            .unwrap();

        assert_eq!(provider.state.next_event, 3);
        assert_eq!(provider.get_swap_quotes().len(), 2);
    }

    #[test]
    fn test_provider_does_not_add_duplicates() {
        let mut provider = setup();

        let quote = TestData::swap_quote(Coin::ETH, Coin::LOKI);
        let witness = TestData::witness(quote.unique_id(), 100, Coin::ETH);

        {
            let mut local_store = provider.local_store.lock().unwrap();
            local_store
                .add_events(vec![quote.into(), witness.clone().into()])
                .unwrap();
        }

        provider.sync();

        assert_eq!(provider.get_witnesses().len(), 1);
        assert_eq!(provider.state.next_event, 2);

        provider.add_local_events(vec![witness.into()]).unwrap();

        provider.sync();

        assert_eq!(provider.get_witnesses().len(), 1);
        assert_eq!(provider.state.next_event, 2);
    }

    #[test]
    #[should_panic(expected = "Negative liquidity depth found")]
    fn test_provider_panics_on_negative_liquidity() {
        let coin = PoolCoin::from(Coin::ETH).expect("Expected valid pool coin");
        let mut provider = setup();
        {
            let change_tx = TestData::pool_change(coin.get_coin(), -100, -100);
            let mut local_store = provider.local_store.lock().unwrap();

            local_store.add_events(vec![change_tx.into()]).unwrap();
        }

        // Pre condition check
        assert!(provider.get_liquidity(coin).is_none());

        provider.sync();
    }

    #[test]
    fn test_provider_tallies_liquidity() {
        let coin = PoolCoin::from(Coin::ETH).expect("Expected valid pool coin");
        let mut provider = setup();
        {
            let mut local_store = provider.local_store.lock().unwrap();

            local_store
                .add_events(vec![
                    TestData::pool_change(coin.get_coin(), 100, 100).into(),
                    TestData::pool_change(coin.get_coin(), 100, -50).into(),
                ])
                .unwrap();
        }

        assert!(provider.get_liquidity(coin).is_none());

        provider.sync();

        let liquidity = provider
            .get_liquidity(coin)
            .expect("Expected liquidity to exist");

        assert_eq!(liquidity.depth, 200);
        assert_eq!(liquidity.base_depth, 50);
    }

    #[test]
    fn test_provider_fulfills_quote_and_witness_on_output_tx() {
        let mut provider = setup();

        let quote = TestData::swap_quote(Coin::ETH, Coin::LOKI);
        let witness = TestData::witness(quote.unique_id(), 100, Coin::ETH);
        let witness_id = witness.unique_id();

        provider
            .local_store
            .lock()
            .unwrap()
            .add_events(vec![quote.clone().into(), witness.clone().into()])
            .unwrap();

        provider.sync();

        provider.confirm_witness(witness_id).unwrap();

        assert_eq!(provider.get_swap_quotes().first().unwrap().fulfilled, false);
        assert_eq!(
            provider.get_witnesses().first().unwrap().status,
            WitnessStatus::Confirmed
        );

        // Swap
        let mut output = TestData::output(quote.output, 100);
        output.parent = OutputParent::SwapQuote(quote.unique_id());
        output.witnesses = vec![witness_id];
        output.address = quote.output_address.clone();

        provider
            .local_store
            .lock()
            .unwrap()
            .add_events(vec![output.into()])
            .unwrap();

        provider.sync();

        assert_eq!(provider.get_swap_quotes().first().unwrap().fulfilled, true);
        assert_eq!(
            provider.get_witnesses().first().unwrap().status,
            WitnessStatus::Processed
        );
    }

    #[test]
    fn test_provider_does_not_fulfill_quote_on_refunded_output_tx() {
        let mut provider = setup();

        let quote = TestData::swap_quote(Coin::ETH, Coin::LOKI);
        let witness = TestData::witness(quote.unique_id(), 100, Coin::ETH);
        let witness_id = witness.unique_id();

        provider
            .local_store
            .lock()
            .unwrap()
            .add_events(vec![quote.clone().into(), witness.clone().into()])
            .unwrap();

        provider.sync();

        // confirm the witness before starting to process - emulating witness_confirmer
        provider.confirm_witness(witness_id).unwrap();

        assert_eq!(provider.get_swap_quotes().first().unwrap().fulfilled, false);
        assert_eq!(
            provider.get_witnesses().first().unwrap().status,
            WitnessStatus::Confirmed
        );

        // Refund
        let mut output = TestData::output(quote.input, 100);
        output.parent = OutputParent::SwapQuote(quote.unique_id());
        output.witnesses = vec![witness_id];
        output.address = quote.return_address.unwrap().clone();

        provider
            .local_store
            .lock()
            .unwrap()
            .add_events(vec![output.into()])
            .unwrap();

        provider.sync();

        assert_eq!(provider.get_swap_quotes().first().unwrap().fulfilled, false);
        assert_eq!(
            provider.get_witnesses().first().unwrap().status,
            WitnessStatus::Processed
        );
    }

    #[test]
    fn test_provider_fulfills_output_txs_on_output_sent_tx() {
        let mut provider = setup();

        let output_tx = TestData::output(Coin::LOKI, 100);

        let output_tx2 = TestData::output(Coin::ETH, 102);

        provider
            .local_store
            .lock()
            .unwrap()
            .add_events(vec![output_tx.clone().into(), output_tx2.clone().into()])
            .unwrap();

        provider.sync();

        let expected = vec![
            FulfilledWrapper::new(output_tx.clone(), false),
            FulfilledWrapper::new(output_tx2.clone(), false),
        ];

        assert_eq!(provider.get_outputs().to_vec(), expected);

        let output_sent_tx = OutputSent {
            outputs: vec![output_tx.unique_id(), output_tx2.unique_id()],
            coin: Coin::LOKI,
            address: "address".into(),
            amount: 100,
            fee: 100,
            transaction_id: "".into(),
            event_number: None,
        };

        provider
            .local_store
            .lock()
            .unwrap()
            .add_events(vec![output_sent_tx.clone().into()])
            .unwrap();

        provider.sync();

        // provider.confirm_witness(witness)

        let expected = vec![
            FulfilledWrapper::new(output_tx.clone(), true),
            FulfilledWrapper::new(output_tx2.clone(), true),
        ];

        assert_eq!(provider.get_outputs().to_vec(), expected);
    }
}
