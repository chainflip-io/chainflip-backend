//! Witness has the following responsibilities:
//! - It is subscribed to the side chain for *quote transactions*
//! - It monitors foreign blockchains for *incoming transactions*

// Events: Lokid transaction, Ether transaction, Swap transaction from Side Chain

use std::sync::Arc;

use crossbeam_channel::Receiver;
use parking_lot::RwLock;
use uuid::Uuid;

use crate::vault::blockchain_connection::{Payment, Payments};
use crate::{common::Timestamp, side_chain::SideChainTx};
use crate::{transactions::WitnessTx, vault::transactions::TransactionProvider};

use crate::common::Coin;

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

            std::thread::sleep(std::time::Duration::from_secs(10));
        }
    }

    /// Start the loki witness
    pub fn start(self) {
        std::thread::spawn(move || {
            self.event_loop();
        });
    }

    /// Publish witness tx for `quote_id`
    fn publish_witness_tx(&self, quote_id: Uuid, payment: &Payment) {
        debug!("Publishing witness transaction for quote: {}", &quote_id);

        let tx = WitnessTx::new(
            Timestamp::now(),
            quote_id,
            "0".to_owned(),
            0,
            0,
            payment.amount.to_atomic(),
            Coin::LOKI,
        );

        debug!("Adding witness tx: {:?}", &tx);
    }

    /// Stuff to do whenever we receive a new block from
    /// a foreign chain
    fn process_main_chain_payments(&mut self, payments: Payments) {
        self.transaction_provider.write().sync();

        let provider = self.transaction_provider.read();
        let swaps = provider.get_quote_txs();
        let stakes = provider.get_stake_quote_txs();

        let mut witness_txs: Vec<SideChainTx> = vec![];

        for payment in &payments {
            let swap_quote = swaps
                .iter()
                .find(|quote| {
                    quote.inner.input == Coin::LOKI
                        && quote.inner.input_address_id == payment.payment_id.to_str()[0..16]
                })
                .map(|quote| quote.inner.id);

            let stake_quote = stakes
                .iter()
                .find(|quote| quote.inner.loki_input_address_id == payment.payment_id)
                .map(|quote| quote.inner.id);

            if let Some(quote_id) = swap_quote.or(stake_quote) {
                debug!("Publishing witness transaction for quote: {}", &quote_id);

                let tx = WitnessTx::new(
                    Timestamp::now(),
                    quote_id,
                    "0".to_owned(),
                    0,
                    0,
                    payment.amount.to_atomic(),
                    Coin::LOKI,
                );

                witness_txs.push(tx.into());
            }
        }

        drop(provider);

        if witness_txs.len() > 0 {
            self.transaction_provider
                .write()
                .add_transactions(witness_txs)
                .expect("Could not publish witness tx");
        }
    }
}
