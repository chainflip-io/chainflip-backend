mod persistent_side_chain;

use crate::transactions::{QuoteTx, WitnessTx};

use serde::{Deserialize, Serialize};

pub use persistent_side_chain::PeristentSideChain;

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub enum SideChainTx {
    QuoteTx(QuoteTx),
    WitnessTx(WitnessTx),
}

#[derive(Clone, Deserialize, Serialize)]
pub struct SideChainBlock {
    pub number: u32,
    pub txs: Vec<SideChainTx>,
}

pub struct SideChain {
    // For now store tx in memory:
    blocks: Vec<SideChainBlock>,
}

pub trait ISideChain {
    /// Add transaciton onto the side chain
    fn add_tx(&mut self, tx: SideChainTx) -> Result<(), String>;

    // TODO: change the sigature the return a reference instead
    /// Get block by index if exists
    fn get_block(&self, block_idx: u32) -> Option<&SideChainBlock>;

    /// Get the index of the lastest block (aka "block height")
    fn last_block(&self) -> Option<&SideChainBlock>;
}

impl SideChain {
    pub fn new() -> SideChain {
        SideChain { blocks: vec![] }
    }

    /// Check whether the transaction exists
    pub fn check_tx(&self, quote_tx: &QuoteTx) -> bool {
        for block in &self.blocks {
            for tx in &block.txs {
                if let SideChainTx::WitnessTx(tx) = tx {
                    if tx.quote_id == quote_tx.id {
                        return true;
                    }
                }
            }
        }

        false
    }

    pub fn get_witness_txs(&self) -> Vec<WitnessTx> {
        let mut quotes: Vec<WitnessTx> = vec![];

        for block in &self.blocks {
            for tx in &block.txs {
                match tx {
                    SideChainTx::WitnessTx(tx) => {
                        quotes.push(tx.clone());
                    }
                    SideChainTx::QuoteTx(_tx) => {
                        // skip
                    }
                }
            }
        }

        quotes
    }
}

impl ISideChain for SideChain {
    fn add_tx(&mut self, tx: SideChainTx) -> Result<(), String> {
        // For now all transactions live in their own block
        let block = SideChainBlock {
            number: self.blocks.len() as u32,
            txs: vec![tx],
        };
        self.blocks.push(block);
        Ok(())
    }

    fn get_block(&self, block_idx: u32) -> Option<&SideChainBlock> {
        self.blocks.get(block_idx as usize)
    }

    fn last_block(&self) -> Option<&SideChainBlock> {
        self.blocks.last()
    }
}
