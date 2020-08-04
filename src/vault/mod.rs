
pub mod witness;
pub mod blockchain_connection;

use crate::transactions::{QuoteTx, WitnessTx};

use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub enum SideChainTx {
    QuoteTx(QuoteTx),
    WitnessTx(WitnessTx),
}

pub struct SideChainBlock {
    pub txs: Vec<SideChainTx>,
}

pub struct SideChain {
    // For now store tx in memory:
    blocks: Vec<SideChainBlock>,
}

pub struct Vault {
    side_chain: Arc<Mutex<SideChain>>,
}

pub trait ISideChain {

    fn add_tx(&mut self, tx: SideChainTx);

    fn get_block(&self, block_idx: u64) -> Option<&SideChainBlock>;

}

impl SideChain {

    pub fn new() -> SideChain {
        SideChain {
            blocks: vec![]
        }
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
                    },
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

    fn add_tx(&mut self, tx: SideChainTx) {
        // For now all transactions live in their own block
        let block = SideChainBlock{ txs: vec![tx] };
        self.blocks.push(block);
    }

    fn get_block(&self, block_idx: u64) -> Option<&SideChainBlock> {
        self.blocks.get(block_idx as usize)
    }

}

impl Vault {


    pub fn new(side_chain: Arc<Mutex<SideChain>>) -> Vault {

        Vault { side_chain }

    }


}