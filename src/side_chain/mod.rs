mod memory_side_chain;
mod persistent_side_chain;

mod substrate_node;

pub use substrate_node::{FakeStateChainNode, IStateChainNode, StateChainNode};

use chainflip_common::types::chain::*;

use serde::{Deserialize, Serialize};

pub use memory_side_chain::MemorySideChain;
pub use persistent_side_chain::PeristentSideChain;

/// Side chain transaction type
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(tag = "type", content = "info")]
pub enum SideChainTx {
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

impl From<DepositQuote> for SideChainTx {
    fn from(tx: DepositQuote) -> Self {
        SideChainTx::DepositQuote(tx)
    }
}

impl From<Deposit> for SideChainTx {
    fn from(tx: Deposit) -> Self {
        SideChainTx::Deposit(tx)
    }
}

impl From<OutputSent> for SideChainTx {
    fn from(tx: OutputSent) -> Self {
        SideChainTx::OutputSent(tx)
    }
}

impl From<Output> for SideChainTx {
    fn from(tx: Output) -> Self {
        SideChainTx::Output(tx)
    }
}

impl From<PoolChange> for SideChainTx {
    fn from(tx: PoolChange) -> Self {
        SideChainTx::PoolChange(tx)
    }
}

impl From<SwapQuote> for SideChainTx {
    fn from(tx: SwapQuote) -> Self {
        SideChainTx::SwapQuote(tx)
    }
}

impl From<WithdrawRequest> for SideChainTx {
    fn from(tx: WithdrawRequest) -> Self {
        SideChainTx::WithdrawRequest(tx)
    }
}

impl From<Withdraw> for SideChainTx {
    fn from(tx: Withdraw) -> Self {
        SideChainTx::Withdraw(tx)
    }
}

impl From<Witness> for SideChainTx {
    fn from(tx: Witness) -> Self {
        SideChainTx::Witness(tx)
    }
}

/// Side chain block type
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SideChainBlock {
    /// Block index on the sidechain.
    pub id: u32,
    /// The list of transactions associated with the current block.
    pub transactions: Vec<SideChainTx>,
}

/// Interface that must be provided by any "side chain" implementation
pub trait ISideChain {
    /// Create a block from transactions (tsx) and add it onto the side chain
    fn add_block(&mut self, txs: Vec<SideChainTx>) -> Result<(), String>;

    // TODO: change the sigature the return a reference instead
    /// Get block by index if exists
    fn get_block(&self, block_idx: u32) -> Option<&SideChainBlock>;

    /// Get the total number of blocks
    fn total_blocks(&self) -> u32;
}
