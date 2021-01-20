use chainflip_common::types::{chain::Witness, UUIDv4};

use super::{ISideChain, SideChainBlock, LocalEvent};

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
    pub fn check_tx(&self, quote_id: UUIDv4) -> bool {
        for block in &self.blocks {
            for tx in &block.transactions {
                if let LocalEvent::Witness(tx) = tx {
                    if tx.quote == quote_id {
                        return true;
                    }
                }
            }
        }

        false
    }

    /// Convenience method for getting all witnesses
    pub fn get_witness_txs(&self) -> Vec<Witness> {
        let mut quotes: Vec<Witness> = vec![];

        for block in &self.blocks {
            for tx in &block.transactions {
                match tx {
                    LocalEvent::Witness(tx) => {
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
    fn add_block(&mut self, txs: Vec<LocalEvent>) -> Result<(), String> {
        // For now all transactions live in their own block
        let block = SideChainBlock {
            id: self.blocks.len() as u32,
            transactions: txs,
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
