use crate::{
    common::ethereum::Transaction as EtherumTransaction,
    side_chain::SideChainTx,
    transactions::{QuoteTx, WitnessTx},
    vault::{blockchain_connection::ethereum::EthereumClient, transactions::TransactionProvider},
};
use std::sync::Arc;

/// A ethereum transaction witness
pub struct EthereumWitness<T, C>
where
    T: TransactionProvider,
    C: EthereumClient,
{
    transaction_provider: Arc<T>,
    client: Arc<C>,
    next_ethereum_block: u64,
}

impl<T, C> EthereumWitness<T, C>
where
    T: TransactionProvider + 'static,
    C: EthereumClient + 'static,
{
    /// Create a new ethereum chain witness
    pub fn new(client: Arc<C>, transaction_provider: Arc<T>) -> Self {
        EthereumWitness {
            client,
            transaction_provider,
            next_ethereum_block: 0, // TODO: Maybe load this in from somewhere so that we don't rescan the whole eth chain
        }
    }

    /// Start witnessing the ethereum chain.
    ///
    /// This will block the thread it is called on.
    pub async fn start(mut self) {
        loop {
            self.poll_next_main_chain().await;

            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    async fn poll_next_main_chain(&mut self) {
        if let Some(transactions) = self.client.get_transactions(self.next_ethereum_block).await {
            self.transaction_provider.sync();
            let quotes = self.transaction_provider.get_quote_txs();

            for tx in transactions {
                if let Some(recipient) = tx.to.as_ref() {
                    let recipient = recipient.to_string();
                    if let Some(quote) = quotes
                        .iter()
                        .find(|quote| quote.input_address.0 == recipient)
                    {
                        self.publish_witness_tx(quote, &tx);
                    }
                }
            }

            self.next_ethereum_block = self.next_ethereum_block + 1;
        }
    }

    /// Publish witness tx for `quote`
    fn publish_witness_tx(&self, quote: &QuoteTx, transaction: &EtherumTransaction) {
        // Ensure that a witness transaction doesn't exist with the given transaction id and quote id
        let hash = transaction.hash.to_string();
        if self
            .transaction_provider
            .get_witness_txs()
            .iter()
            .find(|tx| tx.quote_id == quote.id && tx.transaction_id == hash)
            .is_some()
        {
            return;
        }

        debug!("Publishing witness transaction for quote: {:?}", &quote);

        let tx = WitnessTx {
            quote_id: quote.id,
            transaction_id: hash,
            transaction_block_number: transaction.block_number,
            transaction_index: transaction.index,
            amount: transaction.value,
            sender: Some(transaction.from.to_string()),
        };

        let tx = SideChainTx::WitnessTx(tx);

        self.transaction_provider
            .add_transactions(vec![tx])
            .expect("Could not publish witness tx");
    }
}
