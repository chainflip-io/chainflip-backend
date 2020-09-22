use crate::{
    common::liquidity_provider::LiquidityProvider,
    common::Coin,
    side_chain::SideChainTx,
    transactions::OutputTx,
    transactions::{QuoteTx, StakeQuoteTx},
    utils::price::{self, OutputCalculation},
};
use memory_provider::{FulfilledTxWrapper, WitnessTxWrapper};

/// An interface for providing transactions
pub trait TransactionProvider: LiquidityProvider {
    /// Sync new transactions and return the index of the first unprocessed block
    fn sync(&mut self) -> u32;

    /// Add transactions
    fn add_transactions(&mut self, txs: Vec<SideChainTx>) -> Result<(), String>;

    /// Get all swap quote transactions
    fn get_quote_txs(&self) -> &[FulfilledTxWrapper<QuoteTx>];

    /// Get all stake quote transactions
    fn get_stake_quote_txs(&self) -> &[FulfilledTxWrapper<StakeQuoteTx>];

    /// Get all the witness transactions
    fn get_witness_txs(&self) -> &[WitnessTxWrapper];

    /// Get all the output transactions
    fn get_output_txs(&self) -> &[FulfilledTxWrapper<OutputTx>];
}

/// Memory transaction provider
pub mod memory_provider;
/// Helper functions to do portion-based calculations
/// (probably should be a child module of memory_provider,
/// but don't want to move too much code around)
pub mod portions;
pub use memory_provider::MemoryTransactionsProvider;
