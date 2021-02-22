use crate::local_store::{ILocalStore, LocalEvent};
use chainflip_common::types::{
    chain::{SwapQuote, Witness},
    coin::Coin,
    unique_id::GetUniqueId,
    Timestamp,
};
use crossbeam_channel::Receiver;
use std::sync::{Arc, Mutex};

/// Describes a transaction on a supported chain
#[derive(Debug)]
pub struct CoinTx {
    /// timestamp of transaction
    pub timestamp: Timestamp,
    /// address coins deposited to
    pub deposit_address: String,
    /// address the coins are returned to if the tx fails / is outdated
    pub return_address: Option<String>,
}

/// A representation of a block on some blockchain
#[derive(Debug)]
pub struct Block {
    /// Transactions that belong to this block
    pub txs: Vec<CoinTx>,
}

/// Witness Fake
pub struct FakeWitness<T>
where
    T: ILocalStore + Send,
{
    /// Outstanding quotes (make sure this stays synced)
    quotes: Vec<SwapQuote>,
    oxen_connection: Receiver<Block>,
    local_store: Arc<Mutex<T>>,

    // We should save this to a DB (maybe not, because when we restart, we might want to rescan the db for all quotes?)
    next_event: u64, // event from local store
}

impl<T> FakeWitness<T>
where
    T: ILocalStore + Send + 'static,
{
    /// Construct from internal components
    pub fn new(bc: Receiver<Block>, local_store: Arc<Mutex<T>>) -> FakeWitness<T> {
        let next_event = 0;

        FakeWitness {
            quotes: vec![],
            oxen_connection: bc,
            local_store,
            next_event,
        }
    }

    fn poll_side_chain(&mut self) {
        let mut quote_txs = vec![];

        let local_store = self.local_store.lock().unwrap();

        for tx in &local_store.get_events(self.next_event) {
            if let LocalEvent::SwapQuote(tx) = tx {
                debug!("Registered swap quote: {:?}", tx.unique_id());
                quote_txs.push(tx.clone());
            }
            self.next_event = self.next_event + 1;
        }

        self.quotes.append(&mut quote_txs);
    }

    /// Returns `true` if we can poll again
    fn poll_main_chain(&self) -> bool {
        loop {
            match self.oxen_connection.try_recv() {
                Ok(block) => {
                    debug!("Received message from oxen blockchain: {:?}", block);
                    self.process_main_chain_block(block);
                }
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    debug!("Blockchain channel is closed");
                    break false;
                }
                Err(crossbeam_channel::TryRecvError::Empty) => {
                    break true;
                }
            }
        }
    }

    fn event_loop(mut self) {
        loop {
            // Check the blockchain for quotes on the side chain
            self.poll_side_chain();

            let connection_alive = self.poll_main_chain();

            if !connection_alive {
                break;
            }

            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    /// Start
    pub fn start(self) {
        std::thread::spawn(move || {
            self.event_loop();
        });
    }

    /// Check whether `tx` matches any outstanding qoute
    fn find_quote(&self, tx: &CoinTx) -> Option<&SwapQuote> {
        self.quotes.iter().find(|quote| {
            tx.deposit_address.to_lowercase() == quote.input_address.to_string().to_lowercase()
        })
    }

    /// Publish witness for `quote`
    fn publish_witness_tx(&self, quote: &SwapQuote) {
        debug!("Publishing witness for quote: {:?}", &quote);

        let mut local_store = self.local_store.lock().unwrap();

        let tx = Witness {
            quote: quote.unique_id(),
            transaction_id: "0".into(),
            transaction_block_number: 0,
            transaction_index: 0,
            amount: 100,
            coin: Coin::OXEN,
            event_number: None,
        };

        let tx = LocalEvent::Witness(tx);

        local_store
            .add_events(vec![tx])
            .expect("Could not publish witness");
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
