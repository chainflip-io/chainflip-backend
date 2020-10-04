use crate::{
    common::{
        liquidity_provider::{Liquidity, LiquidityProvider, MemoryLiquidityProvider},
        GenericCoinAmount, LokiAmount, PoolCoin,
    },
    side_chain::{ISideChain, SideChainTx},
    transactions::{
        OutputSentTx, OutputTx, PoolChangeTx, QuoteTx, StakeQuoteTx, StakeTx, UnstakeRequestTx,
        WitnessTx,
    },
    vault::transactions::{
        portions::{adjust_portions_after_stake, StakeContribution},
        TransactionProvider,
    },
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

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
    pub inner: WitnessTx,
    /// Whether the transaction has been used to fulfill
    /// some quote transaction
    pub used: bool,
}

impl WitnessTxWrapper {
    /// Construct from internal parts
    pub fn new(inner: WitnessTx, used: bool) -> Self {
        WitnessTxWrapper { inner, used }
    }
}

/// Staker Identity
pub type StakerId = String;

/// Integer value used to indicate the how much of the pool's
/// value is associated with a given staker id.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
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
    quote_txs: Vec<FulfilledTxWrapper<QuoteTx>>,
    stake_quote_txs: Vec<FulfilledTxWrapper<StakeQuoteTx>>,
    unstake_request_txs: Vec<UnstakeRequestTx>,
    stake_txs: Vec<StakeTx>,
    witness_txs: Vec<WitnessTxWrapper>,
    output_txs: Vec<FulfilledTxWrapper<OutputTx>>,
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
            stake_txs: vec![],
            witness_txs: vec![],
            output_txs: vec![],
            liquidity: MemoryLiquidityProvider::new(),
            next_block_idx: 0,
            staker_portions: HashMap::new(),
        };

        MemoryTransactionsProvider { side_chain, state }
    }
}

/// How much of each coin a given staker owns
/// in coin amounts
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
    fn process_stake_tx(&mut self, tx: StakeTx) {
        // Find quote and mark it as fulfilled
        if let Some(quote_info) = self
            .stake_quote_txs
            .iter_mut()
            .find(|quote_info| quote_info.inner.id == tx.quote_tx)
        {
            quote_info.fulfilled = true;
        }

        // Find witness transacitons and mark them as used:
        for wtx_id in &tx.witness_txs {
            if let Some(witness_info) = self.witness_txs.iter_mut().find(|w| &w.inner.id == wtx_id)
            {
                witness_info.used = true;
            }
        }

        // TODO: we need to have the associated PoolChangeTx at this point, but StakeTx
        // only has a "uuid reference" to a transactions that we haven't processed yet...
        // What's worse, we've made the assumption that StakeTx gets processed first,
        // Because we want to see what the liquidity is like before the contribution was made.

        let contribution = StakeContribution::new(
            tx.staker_id.clone(),
            tx.loki_amount,
            GenericCoinAmount::from_atomic(tx.pool.get_coin(), tx.other_amount),
        );

        adjust_portions_after_stake(
            &mut self.staker_portions,
            &mut self.liquidity.get_pools(),
            &contribution,
        );

        self.stake_txs.push(tx)
    }

    fn process_pool_change_tx(&mut self, tx: PoolChangeTx) {
        if let Err(err) = self.liquidity.update_liquidity(&tx) {
            error!("Failed to process pool change tx {:?}: {}", tx, err);
            panic!(err);
        }
    }

    fn process_unstake_request_tx(&mut self, tx: UnstakeRequestTx) {
        self.unstake_request_txs.push(tx);
    }

    fn process_output_tx(&mut self, tx: OutputTx) {
        // Find quote and mark it as fulfilled only if it's not a refund
        if let Some(quote_info) = self.quote_txs.iter_mut().find(|quote_info| {
            quote_info.inner.id == tx.quote_tx && quote_info.inner.output == tx.coin
        }) {
            quote_info.fulfilled = true;
        }

        // Find witness txs and mark them as fulfilled
        let witnesses = self
            .witness_txs
            .iter_mut()
            .filter(|witness| tx.witness_txs.contains(&witness.inner.id));

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

    fn process_output_sent_tx(&mut self, tx: OutputSentTx) {
        // Find output txs and mark them as fulfilled
        let outputs = self
            .output_txs
            .iter_mut()
            .filter(|output| tx.output_txs.contains(&output.inner.id));

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

            for tx in block.clone().txs {
                match tx {
                    SideChainTx::QuoteTx(tx) => {
                        // Quote transactions always come before their
                        // corresponding "outcome" tx, so they start unfulfilled
                        let tx = FulfilledTxWrapper::new(tx, false);

                        self.state.quote_txs.push(tx);
                    }
                    SideChainTx::StakeQuoteTx(tx) => {
                        // (same as above)
                        let tx = FulfilledTxWrapper::new(tx, false);

                        self.state.stake_quote_txs.push(tx)
                    }
                    SideChainTx::WitnessTx(tx) => {
                        // We assume that witness transactions arrive unused
                        let tx = WitnessTxWrapper {
                            inner: tx,
                            used: false,
                        };

                        self.state.witness_txs.push(tx);
                    }
                    SideChainTx::PoolChangeTx(tx) => self.state.process_pool_change_tx(tx),
                    SideChainTx::StakeTx(tx) => self.state.process_stake_tx(tx),
                    SideChainTx::OutputTx(tx) => self.state.process_output_tx(tx),
                    SideChainTx::UnstakeRequestTx(tx) => self.state.process_unstake_request_tx(tx),
                    SideChainTx::OutputSentTx(tx) => self.state.process_output_sent_tx(tx),
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
                if let SideChainTx::WitnessTx(tx) = tx {
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

    fn get_quote_txs(&self) -> &[FulfilledTxWrapper<QuoteTx>] {
        &self.state.quote_txs
    }

    fn get_stake_quote_txs(&self) -> &[FulfilledTxWrapper<StakeQuoteTx>] {
        &self.state.stake_quote_txs
    }

    fn get_witness_txs(&self) -> &[WitnessTxWrapper] {
        &self.state.witness_txs
    }

    fn get_output_txs(&self) -> &[FulfilledTxWrapper<OutputTx>] {
        &self.state.output_txs
    }

    fn get_unstake_request_txs(&self) -> &[UnstakeRequestTx] {
        &self.state.unstake_request_txs
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
    use crate::{
        common::Coin, common::Timestamp, common::WalletAddress, side_chain::MemorySideChain,
    };
    use crate::{transactions::PoolChangeTx, utils::test_utils::create_fake_quote_tx_eth_loki};

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

            let quote = create_fake_quote_tx_eth_loki();
            let witness = WitnessTx::new(
                Timestamp::now(),
                quote.id,
                "0".to_owned(),
                0,
                1,
                100,
                Coin::ETH,
                None,
            );

            side_chain
                .add_block(vec![quote.into(), witness.into()])
                .unwrap();
        }

        provider.sync();

        assert_eq!(provider.state.next_block_idx, 1);
        assert_eq!(provider.get_quote_txs().len(), 1);
        assert_eq!(provider.get_witness_txs().len(), 1);

        provider
            .add_transactions(vec![create_fake_quote_tx_eth_loki().into()])
            .unwrap();

        assert_eq!(provider.state.next_block_idx, 2);
        assert_eq!(provider.get_quote_txs().len(), 2);
    }

    #[test]
    fn test_provider_does_not_add_duplicates() {
        let mut provider = setup();

        let quote = create_fake_quote_tx_eth_loki();
        let witness = WitnessTx::new(
            Timestamp::now(),
            quote.id,
            "0".to_owned(),
            0,
            1,
            100,
            Coin::ETH,
            None,
        );

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
            let change_tx = PoolChangeTx::new(coin, -100, -100);

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
                    PoolChangeTx::new(coin, 100, 100).into(),
                    PoolChangeTx::new(coin, -50, 100).into(),
                ])
                .unwrap();
        }

        assert!(provider.get_liquidity(coin).is_none());

        provider.sync();

        let liquidity = provider
            .get_liquidity(coin)
            .expect("Expected liquidity to exist");

        assert_eq!(liquidity.depth, 200);
        assert_eq!(liquidity.loki_depth, 50);
    }

    #[test]
    fn test_provider_fulfills_quote_and_witness_on_output_tx() {
        let mut provider = setup();

        let quote = create_fake_quote_tx_eth_loki();
        let witness = WitnessTx::new(
            Timestamp::now(),
            quote.id,
            "0".to_owned(),
            0,
            1,
            100,
            Coin::ETH,
            None,
        );

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
        let output = OutputTx::new(
            Timestamp::now(),
            quote.id,
            vec![witness.id],
            vec![],
            quote.output,
            quote.output_address,
            100,
        )
        .unwrap();

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

        let quote = create_fake_quote_tx_eth_loki();
        let witness = WitnessTx::new(
            Timestamp::now(),
            quote.id,
            "0".to_owned(),
            0,
            1,
            100,
            Coin::ETH,
            None,
        );

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
        let output = OutputTx::new(
            Timestamp::now(),
            quote.id,
            vec![witness.id],
            vec![],
            quote.input,
            quote.return_address.unwrap(),
            100,
        )
        .unwrap();

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

        let output_tx = OutputTx {
            id: uuid::Uuid::new_v4(),
            timestamp: Timestamp::now(),
            quote_tx: uuid::Uuid::new_v4(),
            witness_txs: vec![],
            pool_change_txs: vec![],
            coin: Coin::LOKI,
            address: WalletAddress::new("address"),
            amount: 100,
        };

        let mut another_tx = output_tx.clone();
        another_tx.id = uuid::Uuid::new_v4();

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

        let output_sent_tx = OutputSentTx {
            id: uuid::Uuid::new_v4(),
            timestamp: Timestamp::now(),
            output_txs: vec![output_tx.id, another_tx.id],
            coin: Coin::LOKI,
            address: WalletAddress::new("address"),
            amount: 100,
            fee: 100,
            transaction_id: "".to_owned(),
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
