use crate::side_chain::{ISideChain, SideChainTx};
use chainflip_common::types::{
    chain::{SwapQuote, Witness},
    coin::Coin,
    Timestamp, UUIDv4,
};
use crossbeam_channel::Receiver;
use std::sync::{Arc, Mutex};

#[derive(Debug)]
pub struct CoinTx {
    pub id: UUIDv4,
    pub timestamp: Timestamp,
    pub deposit_address: String,
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
    T: ISideChain + Send,
{
    /// Outstanding quotes (make sure this stays synced)
    quotes: Vec<SwapQuote>,
    loki_connection: Receiver<Block>,
    side_chain: Arc<Mutex<T>>,
    // We should save this to a DB (maybe not, because when we restart, we might want to rescan the db for all quotes?)
    next_block_idx: u32, // block from the side chain
}

impl<T> FakeWitness<T>
where
    T: ISideChain + Send + 'static,
{
    /// Construct from internal components
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
            for tx in &block.transactions {
                if let SideChainTx::SwapQuote(tx) = tx {
                    debug!("Registered swap quote: {:?}", tx.id);
                    quote_txs.push(tx.clone());
                }
            }

            self.next_block_idx = self.next_block_idx + 1;
        }

        self.quotes.append(&mut quote_txs);
    }

    /// Returns `true` if we can poll again
    fn poll_main_chain(&self) -> bool {
        loop {
            match self.loki_connection.try_recv() {
                Ok(block) => {
                    debug!("Received message from loki blockchain: {:?}", block);
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

        let mut side_chain = self.side_chain.lock().unwrap();

        let tx = Witness {
            id: UUIDv4::new(),
            timestamp: Timestamp::now(),
            quote: quote.id,
            transaction_id: "0".into(),
            transaction_block_number: 0,
            transaction_index: 0,
            amount: 100,
            coin: Coin::LOKI,
        };

        let tx = SideChainTx::Witness(tx);

        side_chain
            .add_block(vec![tx])
            .expect("Could not publish witness");

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
