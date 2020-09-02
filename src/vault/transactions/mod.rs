use crate::{
    common::{coins::PoolCoin, Coin},
    side_chain::SideChainTx,
    transactions::{QuoteTx, WitnessTx},
};

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
    /// Sync new transactions
    fn sync(&mut self);

    /// Add transactions
    fn add_transactions(&mut self, txs: Vec<SideChainTx>) -> Result<(), String>;

    /// Get all the quote transactions
    fn get_quote_txs(&self) -> &[QuoteTx];

    /// Get all the witness transactions
    fn get_witness_txs(&self) -> &[WitnessTx];

    /// Get the liquidity for a given pool
    fn get_liquidity(&self, pool: PoolCoin) -> Option<Liquidity>;
}

/// Memory transaction provider
pub mod memory_provider;
pub use memory_provider::MemoryTransactionsProvider;
