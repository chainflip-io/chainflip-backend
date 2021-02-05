//! Witness has the following responsibilities:
//! - It is subscribed to the side chain for *quotes*
//! - It monitors foreign blockchains for *incoming transactions*

// Events: Lokid transaction, Ether transaction, Swap transaction from Side Chain

use crate::{
    local_store::LocalEvent, vault::blockchain_connection::Payments,
    vault::transactions::TransactionProvider,
};
use chainflip_common::types::{chain::Witness, coin::Coin, unique_id::GetUniqueId};
use crossbeam_channel::Receiver;
use parking_lot::RwLock;
use std::sync::Arc;

/// Witness Mock
pub struct LokiWitness<T: TransactionProvider> {
    transaction_provider: Arc<RwLock<T>>,
    loki_connection: Receiver<Payments>,
}

impl<T> LokiWitness<T>
where
    T: TransactionProvider + Send + Sync + 'static,
{
    /// Create Loki witness
    pub fn new(bc: Receiver<Payments>, transaction_provider: Arc<RwLock<T>>) -> LokiWitness<T> {
        LokiWitness {
            loki_connection: bc,
            transaction_provider,
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
            self.poll_main_chain();

            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    /// Start the loki witness
    pub fn start(self) {
        std::thread::spawn(move || {
            self.event_loop();
        });
    }

    /// Stuff to do whenever we receive a new block from
    /// a foreign chain
    fn process_main_chain_payments(&mut self, payments: Payments) {
        // We need to read the state to know which quotes we should witness

        // TODO: now that there is a delay between submitting a witness and
        // finding it (finalized) on the chain we need to make sure we don't submit
        // the same witness twice
        self.transaction_provider.write().sync();

        let witness_txs = {
            let provider = self.transaction_provider.read();
            let swaps = provider.get_swap_quotes();
            let deposit_quotes = provider.get_deposit_quotes();
            let mut witness_txs: Vec<LocalEvent> = vec![];

            for payment in &payments {
                let swap_quote = swaps
                    .iter()
                    .find(|quote| {
                        quote.inner.input == Coin::LOKI
                            && quote.inner.input_address_id == payment.payment_id.to_bytes()
                    })
                    .map(|quote| quote.inner.unique_id());

                let deposit_quote = deposit_quotes
                    .iter()
                    .find(|quote| {
                        quote.inner.base_input_address_id == payment.payment_id.to_bytes()
                    })
                    .map(|quote| quote.inner.unique_id());

                if let Some(quote_id) = swap_quote.or(deposit_quote) {
                    debug!("Publishing witnesses for quote: {}", &quote_id);

                    let tx = Witness {
                        quote: quote_id,
                        transaction_id: payment.tx_hash.clone().into(),
                        transaction_block_number: payment.block_height,
                        transaction_index: 0,
                        amount: payment.amount.to_atomic(),
                        coin: Coin::LOKI,
                        event_number: None,
                    };

                    if tx.amount > 0 {
                        witness_txs.push(tx.into());
                    }
                }
            }
            witness_txs
        };

        self.transaction_provider
            .write()
            .add_local_events(witness_txs)
            .expect("Transactions not added");
    }
}
