use crate::{
    common::{coins::Coin, ethereum::Transaction as EtherumTransaction},
    side_chain::{ISideChain, SideChainTx},
    transactions::{QuoteTx, WitnessTx},
    vault::blockchain_connection::ethereum::EthereumClient,
};
use std::sync::{Arc, Mutex};

/// A ethereum transaction witness
pub struct EthereumWitness<S, C>
where
    S: ISideChain,
    C: EthereumClient,
{
    quotes: Vec<QuoteTx>,
    witness_txs: Vec<WitnessTx>,
    side_chain: Arc<Mutex<S>>,
    client: Arc<C>,
    next_ethereum_block: u64,
    next_side_chain_block: u32,
}

impl<S, C> EthereumWitness<S, C>
where
    S: ISideChain + 'static,
    C: EthereumClient + 'static,
{
    /// Create a new ethereum chain witness
    pub fn new(client: Arc<C>, side_chain: Arc<Mutex<S>>) -> Self {
        EthereumWitness {
            client,
            side_chain,
            quotes: vec![],
            witness_txs: vec![],
            next_ethereum_block: 0, // TODO: Maybe load this in from somewhere so that we don't rescan the whole eth chain
            next_side_chain_block: 0,
        }
    }

    /// Start witnessing the ethereum chain.
    ///
    /// This will block the thread it is called on.
    pub async fn start(mut self) {
        loop {
            // Check the blockchain for quote tx on the side chain
            self.poll_side_chain();

            self.poll_next_main_chain_block().await;

            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    fn poll_side_chain(&mut self) {
        let mut quote_txs = vec![];
        let mut witness_txs = vec![];

        let side_chain = self.side_chain.lock().unwrap();

        while let Some(block) = side_chain.get_block(self.next_side_chain_block) {
            for tx in &block.txs {
                match tx {
                    SideChainTx::QuoteTx(tx) => {
                        if tx.input == Coin::ETH {
                            debug!("[Ethereum] Registered quote tx:  {:?}", tx.id);
                            quote_txs.push(tx.clone());
                        }
                    }
                    SideChainTx::WitnessTx(tx) => witness_txs.push(tx.clone()),
                    _ => continue,
                }
            }

            self.next_side_chain_block = self.next_side_chain_block + 1;
        }

        self.quotes.append(&mut quote_txs);
    }

    async fn poll_next_main_chain_block(&mut self) {
        if let Some(transactions) = self.client.get_transactions(self.next_ethereum_block).await {
            self.process_ethereum_transactions(&transactions);

            self.next_ethereum_block = self.next_ethereum_block + 1;
        }
    }

    /// Publish witness tx for `quote`
    fn publish_witness_tx(&self, quote: &QuoteTx, transaction: &EtherumTransaction) {
        // Ensure that a witness transaction doesn't exist with the given transaction id and quote id
        let hash = transaction.hash.to_string();
        if self
            .witness_txs
            .iter()
            .find(|tx| tx.quote_id == quote.id && tx.transaction_id == hash)
            .is_some()
        {
            return;
        }

        debug!("Publishing witness transaction for quote: {:?}", &quote);

        let mut side_chain = self.side_chain.lock().unwrap();

        let tx = WitnessTx {
            quote_id: quote.id,
            transaction_id: hash,
            transaction_block_number: transaction.block_number,
            transaction_index: transaction.index,
            amount: transaction.value,
            sender: Some(transaction.from.to_string()),
        };

        let tx = SideChainTx::WitnessTx(tx);

        side_chain
            .add_block(vec![tx])
            .expect("Could not publish witness tx");
    }

    /// Process ethereum transaction
    fn process_ethereum_transactions(&self, transactions: &Vec<EtherumTransaction>) {
        for tx in transactions {
            // Don't need to process transactions without a recipient
            if tx.to.is_none() {
                continue;
            }

            let recipient = tx.to.as_ref().unwrap().to_string();

            if let Some(quote) = self
                .quotes
                .iter()
                .find(|quote| quote.input_address.0 == recipient)
            {
                self.publish_witness_tx(quote, tx);
            }
        }
    }
}
