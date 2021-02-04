use crate::{common::LiquidityProvider, local_store::LocalEvent};
use chainflip_common::types::chain::{DepositQuote, Output, SwapQuote, WithdrawRequest, Witness};
use memory_provider::{FulfilledWrapper, StatusWitnessWrapper};

/// Memory transaction provider
pub mod memory_provider;
/// Helper functions to do portion-based calculations
/// (probably should be a child module of memory_provider,
/// but don't want to move too much code around)
pub mod portions;
pub use memory_provider::{MemoryTransactionsProvider, VaultPortions};

/// An interface for providing transactions
pub trait TransactionProvider: LiquidityProvider {
    /// Sync new transactions and return the index of the first unprocessed event
    fn sync(&mut self) -> u64;

    /// Add events to local store
    fn add_local_events(&mut self, events: Vec<LocalEvent>) -> Result<(), String>;

    /// confirm a witness
    fn confirm_witness(&mut self, witness: u64) -> Result<(), String>;

    /// Get all swap quotes
    fn get_swap_quotes(&self) -> &[FulfilledWrapper<SwapQuote>];

    /// Get all deposit quotes
    fn get_deposit_quotes(&self) -> &[FulfilledWrapper<DepositQuote>];

    /// Get all the witnesses
    fn get_witnesses(&self) -> &[StatusWitnessWrapper];

    /// Get all the output transactions
    fn get_outputs(&self) -> &[FulfilledWrapper<Output>];

    /// Get all (unfulfilled?) withdraw requests
    fn get_withdraw_requests(&self) -> &[FulfilledWrapper<WithdrawRequest>];

    /// Get vault portions
    fn get_portions(&self) -> &VaultPortions;
}
