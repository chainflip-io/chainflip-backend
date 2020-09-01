//! Witness has the following responsibilities:
//! - It is subscribed to the side chain for *quote transactions*
//! - It monitors foreign blockchains for *incoming transactions*

// Events: Lokid transaction, Ether transaction, Swap transaction from Side Chain

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use crossbeam_channel::Receiver;

use crate::side_chain::{ISideChain, SideChainTx};
use crate::transactions::{QuoteTx, WitnessTx};
use crate::vault::blockchain_connection::{Payment, Payments};

use crate::common::coins::{Coin, CoinAmount};
use uuid::Uuid;

/// Witness Mock
pub struct LokiWitness<T>
where
    T: ISideChain + Send,
{
    // TODO: need to make sure we keep track of which quotes have been witnessed
    /// Outstanding quotes (make sure this stays synced)
    quotes: HashSet<QuoteTx>,
    loki_connection: Receiver<Payments>,
    side_chain: Arc<Mutex<T>>,
    // We should save this to a DB (maybe not, because when we restart, we might want to rescan the db for all quotes?)
    next_block_idx: u32, // block from the side chain
}

impl<T> LokiWitness<T>
where
    T: ISideChain + Send + 'static,
{
    /// Create Loki witness
    pub fn new(bc: Receiver<Payments>, side_chain: Arc<Mutex<T>>) -> LokiWitness<T> {
        let next_block_idx = 0;

        LokiWitness {
            quotes: HashSet::new(),
            loki_connection: bc,
            side_chain,
            next_block_idx,
        }
    }

    fn poll_side_chain(&mut self) {
        let side_chain = self.side_chain.lock().unwrap();

        while let Some(block) = side_chain.get_block(self.next_block_idx) {
            for tx in &block.txs {
                if let SideChainTx::QuoteTx(tx) = tx {
                    debug!("Registered quote tx: {:?}", tx.id);
                    self.quotes.insert(tx.clone());
                }
            }

            self.next_block_idx = self.next_block_idx + 1;
        }
    }

    fn poll_main_chain(&mut self) {
        loop {
            match self.loki_connection.try_recv() {
                Ok(payments) => {
                    debug!(
                        "Received payments from loki wallet (count: {})",
                        payments.len()
                    );

                    for p in &payments {
                        debug!(
                            "     [{}] unlock: {}, amount: {}",
                            p.block_height, p.unlock_time, p.amount
                        );
                    }
                    self.process_main_chain_payments(payments);
                }
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    error!("Failed to receive message: Disconnected");
                    // Something must have gone wrong if the channel is closed,
                    // so it is bette to abort the program here
                    panic!("Loki connection has been severed");
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

    /// Check whether `tx` matches any outstanding qoute and take the value from the set
    fn find_and_take_matching_quote(&mut self, payment: &Payment) -> Option<QuoteTx> {
        debug!(
            "Looking for a matching quote for payment id: {} amount: {}",
            payment.payment_id, payment.amount
        );

        let res = self
            .quotes
            .iter()
            .find(|quote| payment.payment_id.to_str()[0..16] == quote.input_address_id);

        match res {
            Some(ref quote) => {
                // Annoyngly I have to clone here, because otherwise I'm holding a reference
                // into an object that I'm trying to modify at the same time...
                let quote = (*quote).clone();

                self.quotes.take(&quote)
            }
            None => None,
        }
    }

    /// Publish witness tx for `quote`
    fn publish_witness_tx(&self, quote: &QuoteTx, payment: &Payment) {
        debug!("Publishing witness transaction for quote: {:?}", &quote);

        let mut side_chain = self.side_chain.lock().unwrap();

        let tx = WitnessTx {
            id: Uuid::new_v4(),
            quote_id: quote.id,
            transaction_id: "0".to_owned(),
            transaction_block_number: 0,
            transaction_index: 0,
            amount: payment.amount.to_atomic(),
            coin_type: Coin::LOKI,
            sender: None,
        };

        debug!("Adding witness tx: {:?}", &tx);

        let tx = SideChainTx::WitnessTx(tx);

        side_chain
            .add_block(vec![tx])
            .expect("Could not publish witness tx");

        // Do we remove the quote here?
    }

    /// Stuff to do whenever we receive a new block from
    /// a foreign chain
    fn process_main_chain_payments(&mut self, payments: Payments) {
        for payment in &payments {
            if let Some(quote) = self.find_and_take_matching_quote(payment) {
                debug!("Found a matching transaction!");

                self.publish_witness_tx(&quote, payment);
            }
        }
    }
}
