use crate::{
    side_chain::SideChainTx,
    transactions::WitnessTx,
    vault::{blockchain_connection::ethereum::EthereumClient, transactions::TransactionProvider},
};
use std::sync::{Arc, Mutex};

/// A ethereum transaction witness
pub struct EthereumWitness<T, C>
where
    T: TransactionProvider,
    C: EthereumClient,
{
    transaction_provider: Arc<Mutex<T>>,
    client: Arc<C>,
    next_ethereum_block: u64,
}

impl<T, C> EthereumWitness<T, C>
where
    T: TransactionProvider + 'static,
    C: EthereumClient + 'static,
{
    /// Create a new ethereum chain witness
    pub fn new(client: Arc<C>, transaction_provider: Arc<Mutex<T>>) -> Self {
        EthereumWitness {
            client,
            transaction_provider,
            next_ethereum_block: 0, // TODO: Maybe load this in from somewhere so that we don't rescan the whole eth chain
        }
    }

    async fn event_loop(&mut self) {
        loop {
            self.poll_next_main_chain().await;

            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    /// Start witnessing the ethereum chain on a new thread
    pub async fn start(mut self) {
        std::thread::spawn(move || {
            let mut rt = tokio::runtime::Runtime::new().unwrap();

            rt.block_on(async {
                self.event_loop().await;
            });
        })
    }

    async fn poll_next_main_chain(&mut self) {
        if let Some(transactions) = self.client.get_transactions(self.next_ethereum_block).await {
            let mut provider = self.transaction_provider.lock().unwrap();

            provider.sync();
            let quotes = provider.get_quote_txs();

            let mut witness_txs: Vec<WitnessTx> = vec![];

            for transaction in transactions {
                if let Some(recipient) = transaction.to.as_ref() {
                    let recipient = recipient.to_string();
                    let quote = quotes
                        .iter()
                        .find(|quote| quote.input_address.0 == recipient);

                    if !quote.is_none() {
                        continue;
                    }

                    let quote = quote.unwrap();

                    debug!("Publishing witness transaction for quote: {:?}", &quote);

                    let tx = WitnessTx {
                        quote_id: quote.id,
                        transaction_id: transaction.hash.to_string(),
                        transaction_block_number: transaction.block_number,
                        transaction_index: transaction.index,
                        amount: transaction.value,
                        sender: Some(transaction.from.to_string()),
                    };

                    witness_txs.push(tx);
                }
            }

            if witness_txs.len() > 0 {
                let side_chain_txs = witness_txs
                    .into_iter()
                    .map(SideChainTx::WitnessTx)
                    .collect();

                provider
                    .add_transactions(side_chain_txs)
                    .expect("Could not publish witness txs");
            }

            self.next_ethereum_block = self.next_ethereum_block + 1;
        }
    }
}
