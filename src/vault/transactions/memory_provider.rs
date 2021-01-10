use crate::{
    common::{
        liquidity_provider::{Liquidity, LiquidityProvider, MemoryLiquidityProvider},
        GenericCoinAmount, LokiAmount, PoolCoin, StakerId,
    },
    side_chain::{ISideChain, SideChainTx},
    vault::transactions::{
        portions::{adjust_portions_after_stake, StakeContribution},
        TransactionProvider,
    },
};
use chainflip_common::types::chain::*;
use parking_lot::RwLock;
use serde::Serialize;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use super::portions::{adjust_portions_after_unstake, UnstakeWithdrawal};

/// Transaction plus a boolean flag
#[derive(Debug, Clone, PartialEq)]
pub struct FulfilledTxWrapper<Q: PartialEq> {
    /// The actual transaction
    pub inner: Q,
    /// Whether the transaction has been fulfilled (i.e. there
    /// is a matching "outcome" tx on the side chain)
    pub fulfilled: bool,
}

impl<Q: PartialEq> FulfilledTxWrapper<Q> {
    /// Constructor
    pub fn new(inner: Q, fulfilled: bool) -> FulfilledTxWrapper<Q> {
        FulfilledTxWrapper { inner, fulfilled }
    }
}

/// Witness transaction plus a boolean flag
pub struct WitnessTxWrapper {
    /// The actual transaction
    pub inner: Witness,
    /// Whether the transaction has been used to fulfill
    /// some quote transaction
    pub used: bool,
}

impl WitnessTxWrapper {
    /// Construct from internal parts
    pub fn new(inner: Witness, used: bool) -> Self {
        WitnessTxWrapper { inner, used }
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
    quote_txs: Vec<FulfilledTxWrapper<SwapQuote>>,
    stake_quote_txs: Vec<FulfilledTxWrapper<DepositQuote>>,
    unstake_request_txs: Vec<FulfilledTxWrapper<WithdrawRequest>>,
    unstake_txs: Vec<Withdraw>,
    stake_txs: Vec<Deposit>,
    witness_txs: Vec<WitnessTxWrapper>,
    output_txs: Vec<FulfilledTxWrapper<Output>>,
    liquidity: MemoryLiquidityProvider,
    next_block_idx: u32,
    staker_portions: VaultPortions,
}

/// An in-memory transaction provider
pub struct MemoryTransactionsProvider<S: ISideChain> {
    side_chain: Arc<Mutex<S>>,
    state: MemoryState,
}

impl<S: ISideChain> MemoryTransactionsProvider<S> {
    /// Create an in-memory transaction provider
    pub fn new(side_chain: Arc<Mutex<S>>) -> Self {
        let state = MemoryState {
            quote_txs: vec![],
            stake_quote_txs: vec![],
            unstake_request_txs: vec![],
            unstake_txs: vec![],
            stake_txs: vec![],
            witness_txs: vec![],
            output_txs: vec![],
            liquidity: MemoryLiquidityProvider::new(),
            next_block_idx: 0,
            staker_portions: HashMap::new(),
        };

        MemoryTransactionsProvider { side_chain, state }
    }

    /// Helper constructor to return a wrapped (thread safe) instance
    pub fn new_protected(side_chain: Arc<Mutex<S>>) -> Arc<RwLock<Self>> {
        let p = Self::new(side_chain);
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
    fn process_stake_tx(&mut self, tx: Deposit) {
        // Find quote and mark it as fulfilled
        if let Some(quote_info) = self
            .stake_quote_txs
            .iter_mut()
            .find(|quote_info| quote_info.inner.id == tx.quote)
        {
            quote_info.fulfilled = true;
        }

        // Find witness transacitons and mark them as used:
        for wtx_id in &tx.witnesses {
            if let Some(witness_info) = self.witness_txs.iter_mut().find(|w| &w.inner.id == wtx_id)
            {
                witness_info.used = true;
            }
        }

        // TODO: we need to have the associated PoolChange at this point, but Deposit
        // only has a "uuid reference" to a transactions that we haven't processed yet...
        // What's worse, we've made the assumption that Deposit gets processed first,
        // Because we want to see what the liquidity is like before the contribution was made.

        let contribution = StakeContribution::new(
            StakerId::from_bytes(&tx.staker_id).unwrap(),
            LokiAmount::from_atomic(tx.base_amount),
            GenericCoinAmount::from_atomic(tx.pool, tx.other_amount),
        );

        adjust_portions_after_stake(
            &mut self.staker_portions,
            &mut self.liquidity.get_pools(),
            &contribution,
        );

        self.stake_txs.push(tx)
    }

    fn process_pool_change_tx(&mut self, tx: PoolChange) {
        debug!("Processing a pool change tx: {:?}", tx);
        if let Err(err) = self.liquidity.update_liquidity(&tx) {
            error!("Failed to process pool change tx {:?}: {}", tx, err);
            panic!(err);
        }
    }

    fn process_unstake_request_tx(&mut self, tx: WithdrawRequest) {
        let tx = FulfilledTxWrapper::new(tx, false);
        self.unstake_request_txs.push(tx);
    }

    fn process_unstake_tx(&mut self, tx: Withdraw) {
        // We must be able to find the request or else we won't be
        // able to adjust portions which might result in double unstake

        // Find quote and mark it as fulfilled
        let wrapped_unstake_request = match self
            .unstake_request_txs
            .iter_mut()
            .find(|w_unstake_req| w_unstake_req.inner.id == tx.withdraw_request)
        {
            Some(w_unstake_req) => {
                w_unstake_req.fulfilled = true;
                w_unstake_req
            }
            None => panic!(
                "No unstake request found that matches unstake transaction request id: {}",
                tx.withdraw_request
            ),
        };

        let unstake_req = &wrapped_unstake_request.inner;

        let staker_id = StakerId::from_bytes(&unstake_req.staker_id).unwrap();
        let pool = PoolCoin::from(*&unstake_req.pool).unwrap();
        let fraction = *&unstake_req.fraction;

        let unstake = UnstakeWithdrawal {
            staker_id,
            fraction,
            pool,
        };

        let liquidity = self
            .liquidity
            .get_liquidity(pool)
            .expect("Liquidity must exist for unstaked coin");

        adjust_portions_after_unstake(&mut self.staker_portions, &liquidity, unstake);

        self.unstake_txs.push(tx);
    }

    fn process_output_tx(&mut self, tx: Output) {
        // Find quote and mark it as fulfilled only if it's not a refund
        if let Some(quote_info) = self.quote_txs.iter_mut().find(|quote_info| {
            quote_info.inner.id == tx.parent_id() && quote_info.inner.output == tx.coin
        }) {
            quote_info.fulfilled = true;
        }

        // Find witness txs and mark them as fulfilled
        let witnesses = self
            .witness_txs
            .iter_mut()
            .filter(|witness| tx.witnesses.contains(&witness.inner.id));

        for witness in witnesses {
            witness.used = true;
        }

        // Add output tx
        let wrapper = FulfilledTxWrapper {
            inner: tx,
            fulfilled: false,
        };

        self.output_txs.push(wrapper);
    }

    fn process_output_sent_tx(&mut self, tx: OutputSent) {
        // Find output txs and mark them as fulfilled
        let outputs = self
            .output_txs
            .iter_mut()
            .filter(|output| tx.outputs.contains(&output.inner.id));

        for output in outputs {
            output.fulfilled = true;
        }
    }
}

impl<S: ISideChain> TransactionProvider for MemoryTransactionsProvider<S> {
    fn sync(&mut self) -> u32 {
        let side_chain = self.side_chain.lock().unwrap();
        while let Some(block) = side_chain.get_block(self.state.next_block_idx) {
            debug!(
                "TX Provider processing block: {}",
                self.state.next_block_idx
            );

            for tx in block.clone().transactions {
                match tx {
                    SideChainTx::SwapQuote(tx) => {
                        // Quote transactions always come before their
                        // corresponding "outcome" tx, so they start unfulfilled
                        let tx = FulfilledTxWrapper::new(tx, false);

                        self.state.quote_txs.push(tx);
                    }
                    SideChainTx::DepositQuote(tx) => {
                        // (same as above)
                        let tx = FulfilledTxWrapper::new(tx, false);

                        self.state.stake_quote_txs.push(tx)
                    }
                    SideChainTx::Witness(tx) => {
                        // We assume that witness transactions arrive unused
                        let tx = WitnessTxWrapper {
                            inner: tx,
                            used: false,
                        };

                        self.state.witness_txs.push(tx);
                    }
                    SideChainTx::PoolChange(tx) => self.state.process_pool_change_tx(tx),
                    SideChainTx::Deposit(tx) => self.state.process_stake_tx(tx),
                    SideChainTx::Output(tx) => self.state.process_output_tx(tx),
                    SideChainTx::WithdrawRequest(tx) => self.state.process_unstake_request_tx(tx),
                    SideChainTx::Withdraw(tx) => self.state.process_unstake_tx(tx),
                    SideChainTx::OutputSent(tx) => self.state.process_output_sent_tx(tx),
                }
            }
            self.state.next_block_idx += 1;
        }

        self.state.next_block_idx
    }

    fn add_transactions(&mut self, txs: Vec<SideChainTx>) -> Result<(), String> {
        // Filter out any duplicate transactions
        let valid_txs: Vec<SideChainTx> = txs
            .into_iter()
            .filter(|tx| {
                if let SideChainTx::Witness(tx) = tx {
                    return !self
                        .state
                        .witness_txs
                        .iter()
                        .any(|witness| tx == &witness.inner);
                }

                true
            })
            .collect();

        if valid_txs.len() > 0 {
            self.side_chain.lock().unwrap().add_block(valid_txs)?;
        }

        self.sync();
        Ok(())
    }

    fn get_quote_txs(&self) -> &[FulfilledTxWrapper<SwapQuote>] {
        &self.state.quote_txs
    }

    fn get_stake_quote_txs(&self) -> &[FulfilledTxWrapper<DepositQuote>] {
        &self.state.stake_quote_txs
    }

    fn get_witness_txs(&self) -> &[WitnessTxWrapper] {
        &self.state.witness_txs
    }

    fn get_output_txs(&self) -> &[FulfilledTxWrapper<Output>] {
        &self.state.output_txs
    }

    fn get_unstake_request_txs(&self) -> &[FulfilledTxWrapper<WithdrawRequest>] {
        &self.state.unstake_request_txs
    }

    fn get_portions(&self) -> &VaultPortions {
        &self.state.staker_portions
    }
}

impl<S: ISideChain> LiquidityProvider for MemoryTransactionsProvider<S> {
    fn get_liquidity(&self, pool: PoolCoin) -> Option<Liquidity> {
        self.state.liquidity.get_liquidity(pool)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{side_chain::MemorySideChain, utils::test_utils::data::TestData};
    use chainflip_common::types::{coin::Coin, Timestamp, UUIDv4};

    fn setup() -> MemoryTransactionsProvider<MemorySideChain> {
        let side_chain = Arc::new(Mutex::new(MemorySideChain::new()));
        MemoryTransactionsProvider::new(side_chain)
    }

    #[test]
    fn test_provider() {
        let mut provider = setup();

        assert!(provider.get_quote_txs().is_empty());
        assert!(provider.get_witness_txs().is_empty());

        // Add some random blocks
        {
            let mut side_chain = provider.side_chain.lock().unwrap();

            let quote = TestData::swap_quote(Coin::ETH, Coin::LOKI);
            let witness = TestData::witness(quote.id, 100, Coin::ETH);

            side_chain
                .add_block(vec![quote.into(), witness.into()])
                .unwrap();
        }

        provider.sync();

        assert_eq!(provider.state.next_block_idx, 1);
        assert_eq!(provider.get_quote_txs().len(), 1);
        assert_eq!(provider.get_witness_txs().len(), 1);

        provider
            .add_transactions(vec![TestData::swap_quote(Coin::ETH, Coin::LOKI).into()])
            .unwrap();

        assert_eq!(provider.state.next_block_idx, 2);
        assert_eq!(provider.get_quote_txs().len(), 2);
    }

    #[test]
    fn test_provider_does_not_add_duplicates() {
        let mut provider = setup();

        let quote = TestData::swap_quote(Coin::ETH, Coin::LOKI);
        let witness = TestData::witness(quote.id, 100, Coin::ETH);

        {
            let mut side_chain = provider.side_chain.lock().unwrap();

            side_chain
                .add_block(vec![quote.into(), witness.clone().into()])
                .unwrap();
        }

        provider.sync();

        assert_eq!(provider.get_witness_txs().len(), 1);
        assert_eq!(provider.state.next_block_idx, 1);

        provider.add_transactions(vec![witness.into()]).unwrap();

        assert_eq!(provider.get_witness_txs().len(), 1);
        assert_eq!(provider.state.next_block_idx, 1);
    }

    #[test]
    #[should_panic(expected = "Negative liquidity depth found")]
    fn test_provider_panics_on_negative_liquidity() {
        let coin = PoolCoin::from(Coin::ETH).expect("Expected valid pool coin");
        let mut provider = setup();
        {
            let change_tx = TestData::pool_change(coin.get_coin(), -100, -100);
            let mut side_chain = provider.side_chain.lock().unwrap();

            side_chain.add_block(vec![change_tx.into()]).unwrap();
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
            let mut side_chain = provider.side_chain.lock().unwrap();

            side_chain
                .add_block(vec![
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
        let witness = TestData::witness(quote.id, 100, Coin::ETH);

        provider
            .side_chain
            .lock()
            .unwrap()
            .add_block(vec![quote.clone().into(), witness.clone().into()])
            .unwrap();

        provider.sync();

        assert_eq!(provider.get_quote_txs().first().unwrap().fulfilled, false);
        assert_eq!(provider.get_witness_txs().first().unwrap().used, false);

        // Swap
        let mut output = TestData::output(quote.output, 100);
        output.parent = OutputParent::SwapQuote(quote.id);
        output.witnesses = vec![witness.id];
        output.address = quote.output_address.clone();

        provider
            .side_chain
            .lock()
            .unwrap()
            .add_block(vec![output.into()])
            .unwrap();

        provider.sync();

        assert_eq!(provider.get_quote_txs().first().unwrap().fulfilled, true);
        assert_eq!(provider.get_witness_txs().first().unwrap().used, true);
    }

    #[test]
    fn test_provider_does_not_fulfill_quote_on_refunded_output_tx() {
        let mut provider = setup();

        let quote = TestData::swap_quote(Coin::ETH, Coin::LOKI);
        let witness = TestData::witness(quote.id, 100, Coin::ETH);

        provider
            .side_chain
            .lock()
            .unwrap()
            .add_block(vec![quote.clone().into(), witness.clone().into()])
            .unwrap();

        provider.sync();

        assert_eq!(provider.get_quote_txs().first().unwrap().fulfilled, false);
        assert_eq!(provider.get_witness_txs().first().unwrap().used, false);

        // Refund
        let mut output = TestData::output(quote.input, 100);
        output.parent = OutputParent::SwapQuote(quote.id);
        output.witnesses = vec![witness.id];
        output.address = quote.return_address.unwrap().clone();

        provider
            .side_chain
            .lock()
            .unwrap()
            .add_block(vec![output.into()])
            .unwrap();

        provider.sync();

        assert_eq!(provider.get_quote_txs().first().unwrap().fulfilled, false);
        assert_eq!(provider.get_witness_txs().first().unwrap().used, true);
    }

    #[test]
    fn test_provider_fulfills_output_txs_on_output_sent_tx() {
        let mut provider = setup();

        let output_tx = TestData::output(Coin::LOKI, 100);

        let mut another_tx = output_tx.clone();
        another_tx.id = UUIDv4::new();

        provider
            .side_chain
            .lock()
            .unwrap()
            .add_block(vec![output_tx.clone().into(), another_tx.clone().into()])
            .unwrap();

        provider.sync();

        let expected = vec![
            FulfilledTxWrapper::new(output_tx.clone(), false),
            FulfilledTxWrapper::new(another_tx.clone(), false),
        ];

        assert_eq!(provider.get_output_txs().to_vec(), expected);

        let output_sent_tx = OutputSent {
            id: UUIDv4::new(),
            timestamp: Timestamp::now(),
            outputs: vec![output_tx.id, another_tx.id],
            coin: Coin::LOKI,
            address: "address".into(),
            amount: 100,
            fee: 100,
            transaction_id: "".into(),
        };

        provider
            .side_chain
            .lock()
            .unwrap()
            .add_block(vec![output_sent_tx.clone().into()])
            .unwrap();

        provider.sync();

        let expected = vec![
            FulfilledTxWrapper::new(output_tx.clone(), true),
            FulfilledTxWrapper::new(another_tx.clone(), true),
        ];

        assert_eq!(provider.get_output_txs().to_vec(), expected);
    }
}
