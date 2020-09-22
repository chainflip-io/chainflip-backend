use std::sync::{Arc, Mutex};

use crossbeam_channel::Receiver;

use crate::common::{Block, Coin, Timestamp};
use crate::side_chain::{ISideChain, SideChainTx};
use crate::transactions::{CoinTx, QuoteTx, WitnessTx};
use uuid::Uuid;

/// Witness Fake
pub struct FakeWitness<T>
where
    T: ISideChain + Send,
{
    /// Outstanding quotes (make sure this stays synced)
    quotes: Vec<QuoteTx>,
    loki_connection: Receiver<Block>,
    side_chain: Arc<Mutex<T>>,
    // We should save this to a DB (maybe not, because when we restart, we might want to rescan the db for all quotes?)
    next_block_idx: u32, // block from the side chain
}

impl<T> FakeWitness<T>
where
    T: ISideChain + Send + 'static,
{
    pub fn new(bc: Receiver<Block>, side_chain: Arc<Mutex<T>>) -> FakeWitness<T> {
        let next_block_idx = 0;

        FakeWitness {
            quotes: vec![],
            loki_connection: bc,
            side_chain,
            next_block_idx,
        }
    }

    fn poll_side_chain(&mut self) {
        let mut quote_txs = vec![];

        let side_chain = self.side_chain.lock().unwrap();

        while let Some(block) = side_chain.get_block(self.next_block_idx) {
            for tx in &block.txs {
                if let SideChainTx::QuoteTx(tx) = tx {
                    debug!("Registered quote tx: {:?}", tx.id);
                    quote_txs.push(tx.clone());
                }
            }

            self.next_block_idx = self.next_block_idx + 1;
        }

        self.quotes.append(&mut quote_txs);
    }

    fn poll_main_chain(&self) {
        loop {
            match self.loki_connection.try_recv() {
                Ok(block) => {
                    debug!("Received message from loki blockchain: {:?}", block);
                    self.process_main_chain_block(block);
                }
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    error!("Failed to receive message: Disconnected");
                    break;
                }
                Err(crossbeam_channel::TryRecvError::Empty) => {
                    break;
                }
            }
        }
    }

    fn event_loop(mut self) {
        loop {
            // Check the blockchain for quote tx on the side chain
            self.poll_side_chain();

            self.poll_main_chain();

            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    pub fn start(self) {
        std::thread::spawn(move || {
            self.event_loop();
        });
    }

    /// Check whether `tx` matches any outstanding qoute
    fn find_quote(&self, tx: &CoinTx) -> Option<&QuoteTx> {
        self.quotes
            .iter()
            .find(|quote| tx.deposit_address == quote.input_address)
    }

    /// Publish witness tx for `quote`
    fn publish_witness_tx(&self, quote: &QuoteTx) {
        debug!("Publishing witness transaction for quote: {:?}", &quote);

        let mut side_chain = self.side_chain.lock().unwrap();

        let tx = WitnessTx {
            id: Uuid::new_v4(),
            timestamp: Timestamp::now(),
            quote_id: quote.id,
            transaction_id: "0".to_owned(),
            transaction_block_number: 0,
            transaction_index: 0,
            amount: 0,
            coin: Coin::LOKI,
            sender: None,
        };

        let tx = SideChainTx::WitnessTx(tx);

        side_chain
            .add_block(vec![tx])
            .expect("Could not publish witness tx");

        // Do we remove the quote here?
    }

    /// Stuff to do whenever we receive a new block from
    /// a foreign chain
    fn process_main_chain_block(&self, block: Block) {
        for tx in &block.txs {
            if let Some(quote) = self.find_quote(tx) {
                self.publish_witness_tx(quote);
            }
        }
    }
}
