mod memory_side_chain;
mod persistent_side_chain;

use crate::transactions::{
    OutputSentTx, OutputTx, PoolChangeTx, QuoteTx, StakeQuoteTx, StakeTx, UnstakeRequestTx,
    WitnessTx,
};

use serde::{Deserialize, Serialize};

pub use memory_side_chain::MemorySideChain;
pub use persistent_side_chain::PeristentSideChain;

/// Side chain transaction type
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(tag = "type", content = "info")]
pub enum SideChainTx {
    /// The quote transaction variant
    QuoteTx(QuoteTx),
    /// The witness transaction variant
    WitnessTx(WitnessTx),
    /// The pool change transaction variant
    PoolChangeTx(PoolChangeTx),
    /// Stake/provisioning quote transaction varian
    StakeQuoteTx(StakeQuoteTx),
    /// Transaction acknowledging pool provisioning
    StakeTx(StakeTx),
    /// The output transaction variant
    OutputTx(OutputTx),
    /// Unstake Reuquest variant
    UnstakeRequestTx(UnstakeRequestTx),
    /// Output sent transaction variant
    OutputSentTx(OutputSentTx),
}

impl From<QuoteTx> for SideChainTx {
    fn from(tx: QuoteTx) -> Self {
        SideChainTx::QuoteTx(tx)
    }
}

impl From<WitnessTx> for SideChainTx {
    fn from(tx: WitnessTx) -> Self {
        SideChainTx::WitnessTx(tx)
    }
}

impl From<PoolChangeTx> for SideChainTx {
    fn from(tx: PoolChangeTx) -> Self {
        SideChainTx::PoolChangeTx(tx)
    }
}

impl From<StakeQuoteTx> for SideChainTx {
    fn from(tx: StakeQuoteTx) -> Self {
        SideChainTx::StakeQuoteTx(tx)
    }
}

impl From<StakeTx> for SideChainTx {
    fn from(tx: StakeTx) -> Self {
        SideChainTx::StakeTx(tx)
    }
}

impl From<OutputTx> for SideChainTx {
    fn from(tx: OutputTx) -> Self {
        SideChainTx::OutputTx(tx)
    }
}

impl From<UnstakeRequestTx> for SideChainTx {
    fn from(tx: UnstakeRequestTx) -> Self {
        SideChainTx::UnstakeRequestTx(tx)
    }
}

impl From<OutputSentTx> for SideChainTx {
    fn from(tx: OutputSentTx) -> Self {
        SideChainTx::OutputSentTx(tx)
    }
}

/// Side chain block type
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SideChainBlock {
    /// Block index on the sidechain.
    pub id: u32,
    /// The list of transactions associated with the current block.
    pub txs: Vec<SideChainTx>,
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
