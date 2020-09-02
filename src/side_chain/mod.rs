mod fake_side_chain;
mod persistent_side_chain;

use crate::transactions::{PoolChangeTx, QuoteTx, StakeQuoteTx, WitnessTx};

use serde::{Deserialize, Serialize};

pub use fake_side_chain::FakeSideChain;
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
}

impl From<QuoteTx> for SideChainTx {
    fn from(tx: QuoteTx) -> Self {
        SideChainTx::QuoteTx(tx)
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
