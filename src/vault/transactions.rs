use crate::{
    side_chain::{ISideChain, SideChainTx},
    transactions::{QuoteTx, WitnessTx},
};
use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc, Mutex,
};

/// An interface for providing transactions
pub trait TransactionProvider {
    /// Sync new transactions
    fn sync(&self);

    /// Add transactions
    fn add_transactions(&self, txs: Vec<SideChainTx>) -> Result<(), String>;

    /// Get all the quote transactions
    fn get_quote_txs(&self) -> Vec<QuoteTx>;

    /// Get all the witness transactions
    fn get_witness_txs(&self) -> Vec<WitnessTx>;
}

/// An in-memory transaction provider
pub(crate) struct MemoryTransactionsProvider<S: ISideChain> {
    side_chain: Arc<Mutex<S>>,
    transactions: Mutex<Vec<SideChainTx>>,
    next_block_idx: AtomicU32,
}

impl<S: ISideChain + 'static> MemoryTransactionsProvider<S> {
    /// Create an in-memory transaction provider
    pub fn new(side_chain: Arc<Mutex<S>>) -> Self {
        MemoryTransactionsProvider {
            side_chain: side_chain.clone(),
            transactions: Mutex::new(vec![]),
            next_block_idx: AtomicU32::new(0),
        }
    }
}

impl<S: ISideChain> TransactionProvider for MemoryTransactionsProvider<S> {
    fn sync(&self) {
        let side_chain = self.side_chain.lock().unwrap();
        let mut new_transactions: Vec<SideChainTx> = vec![];
        let mut next_block_idx = self.next_block_idx.load(Ordering::SeqCst);
        while let Some(block) = side_chain.get_block(next_block_idx) {
            new_transactions.extend(block.txs.clone());
            next_block_idx += 1;
        }

        if new_transactions.len() > 0 {
            self.transactions.lock().unwrap().extend(new_transactions);
        }

        self.next_block_idx.store(next_block_idx, Ordering::SeqCst);
    }

    fn add_transactions(&self, txs: Vec<SideChainTx>) -> Result<(), String> {
        self.side_chain.lock().unwrap().add_block(txs)
    }

    fn get_quote_txs(&self) -> Vec<QuoteTx> {
        self.transactions
            .lock()
            .unwrap()
            .iter()
            .filter_map(|tx| {
                if let SideChainTx::QuoteTx(tx) = tx {
                    Some(tx.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    fn get_witness_txs(&self) -> Vec<WitnessTx> {
        self.transactions
            .lock()
            .unwrap()
            .iter()
            .filter_map(|tx| {
                if let SideChainTx::WitnessTx(tx) = tx {
                    Some(tx.clone())
                } else {
                    None
                }
            })
            .collect()
    }
}
