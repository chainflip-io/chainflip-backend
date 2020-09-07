use crate::{
    common::{coins::PoolCoin, Coin},
    side_chain::SideChainTx,
    transactions::{QuoteTx, StakeQuoteTx, WitnessTx},
    utils::price::{self, OutputCalculation},
};
use memory_provider::QuoteTxWrapper;

/// A simple representation of a pool liquidity
#[derive(Debug, Copy, Clone)]
pub struct Liquidity {
    /// The depth of the coin staked against LOKI in the pool
    pub depth: u128,
    /// The depth of LOKI in the pool
    pub loki_depth: u128,
}

impl Liquidity {
    /// Create a new liquidity
    pub fn new() -> Self {
        Liquidity {
            depth: 0,
            loki_depth: 0,
        }
    }
}

/// An interface for providing transactions
pub trait TransactionProvider {
    /// Sync new transactions and return the index of the first unprocessed block
    fn sync(&mut self) -> u32;

    /// Add transactions
    fn add_transactions(&mut self, txs: Vec<SideChainTx>) -> Result<(), String>;

    /// Get all swap quote transactions
    fn get_quote_txs(&self) -> &[QuoteTxWrapper<QuoteTx>];

    /// Get all stake quote transactions
    fn get_stake_quote_txs(&self) -> &[QuoteTxWrapper<StakeQuoteTx>];

    /// Get all the witness transactions
    fn get_witness_txs(&self) -> &[WitnessTx];

    /// Get the liquidity for a given pool
    fn get_liquidity(&self, pool: PoolCoin) -> Option<Liquidity>;

    /// Get the output amount.
    ///
    /// If `input` or `output` is `LOKI` then only 1 output is returned.
    ///
    /// If `input` or `output` is *NOT* `LOKI` then 2 outputs are returned: `[(input, LOKI, fee), (LOKI, output, fee)]`
    fn get_output_amount(
        &self,
        input: Coin,
        input_amount: u128,
        output: Coin,
    ) -> Result<OutputCalculation, &'static str>
    where
        Self: Sized,
    {
        price::get_output(self, input, input_amount, output)
    }
}

/// Memory transaction provider
pub mod memory_provider;
pub use memory_provider::MemoryTransactionsProvider;
