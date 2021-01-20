mod memory_side_chain;
mod persistent_side_chain;

use chainflip_common::types::chain::*;

use serde::{Deserialize, Serialize};

pub use memory_side_chain::MemorySideChain;
pub use persistent_side_chain::PeristentSideChain;

/// Side chain transaction type
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(tag = "type", content = "info")]
pub enum LocalEvent {
    /// Deposit quote
    DepositQuote(DepositQuote),
    /// Deposit
    Deposit(Deposit),
    /// Output sent
    OutputSent(OutputSent),
    /// Output
    Output(Output),
    /// Pool change
    PoolChange(PoolChange),
    /// Swap quote
    SwapQuote(SwapQuote),
    /// Withdraw request
    WithdrawRequest(WithdrawRequest),
    /// Withdraw
    Withdraw(Withdraw),
    /// Witness
    Witness(Witness),
}

impl From<DepositQuote> for LocalEvent {
    fn from(tx: DepositQuote) -> Self {
        LocalEvent::DepositQuote(tx)
    }
}

impl From<Deposit> for LocalEvent {
    fn from(tx: Deposit) -> Self {
        LocalEvent::Deposit(tx)
    }
}

impl From<OutputSent> for LocalEvent {
    fn from(tx: OutputSent) -> Self {
        LocalEvent::OutputSent(tx)
    }
}

impl From<Output> for LocalEvent {
    fn from(tx: Output) -> Self {
        LocalEvent::Output(tx)
    }
}

impl From<PoolChange> for LocalEvent {
    fn from(tx: PoolChange) -> Self {
        LocalEvent::PoolChange(tx)
    }
}

impl From<SwapQuote> for LocalEvent {
    fn from(tx: SwapQuote) -> Self {
        LocalEvent::SwapQuote(tx)
    }
}

impl From<WithdrawRequest> for LocalEvent {
    fn from(tx: WithdrawRequest) -> Self {
        LocalEvent::WithdrawRequest(tx)
    }
}

impl From<Withdraw> for LocalEvent {
    fn from(tx: Withdraw) -> Self {
        LocalEvent::Withdraw(tx)
    }
}

impl From<Witness> for LocalEvent {
    fn from(tx: Witness) -> Self {
        LocalEvent::Witness(tx)
    }
}

/// Side chain block type
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SideChainBlock {
    /// Block index on the sidechain.
    pub id: u32,
    /// The list of transactions associated with the current block.
    pub transactions: Vec<LocalEvent>,
}

/// Interface that must be provided by any "side chain" implementation
pub trait ISideChain {
    /// Create a block from transactions (tsx) and add it onto the side chain
    fn add_block(&mut self, txs: Vec<LocalEvent>) -> Result<(), String>;

    // TODO: change the sigature the return a reference instead
    /// Get block by index if exists
    fn get_block(&self, block_idx: u32) -> Option<&SideChainBlock>;

    /// Get the total number of blocks
    fn total_blocks(&self) -> u32;
}
