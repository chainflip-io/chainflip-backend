use super::{ISideChain, SideChainBlock, SideChainTx};

use crate::transactions::{QuoteTx, WitnessTx};

/// Fake implemenation of ISideChain that stores block in memory
pub struct MemorySideChain {
    // For now store tx in memory:
    blocks: Vec<SideChainBlock>,
}

impl MemorySideChain {
    /// Create an empty (fake) chain
    pub fn new() -> Self {
        MemorySideChain { blocks: vec![] }
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

    /// Convenience method for getting all witness transactions
    pub fn get_witness_txs(&self) -> Vec<WitnessTx> {
        let mut quotes: Vec<WitnessTx> = vec![];

        for block in &self.blocks {
            for tx in &block.txs {
                match tx {
                    SideChainTx::WitnessTx(tx) => {
                        quotes.push(tx.clone());
                    }
                    _ => {
                        // skip
                    }
                }
            }
        }

        quotes
    }
}

impl ISideChain for MemorySideChain {
    fn add_block(&mut self, txs: Vec<SideChainTx>) -> Result<(), String> {
        // For now all transactions live in their own block
        let block = SideChainBlock {
            id: self.blocks.len() as u32,
            txs,
        };

        debug!("Adding block idx: {}", block.id);
        self.blocks.push(block);
        Ok(())
    }

    fn get_block(&self, block_idx: u32) -> Option<&SideChainBlock> {
        self.blocks.get(block_idx as usize)
    }

    fn total_blocks(&self) -> u32 {
        self.blocks.len() as u32
    }
}
