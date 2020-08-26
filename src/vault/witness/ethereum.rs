use crate::{
    common::coins::Coin,
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
    T: TransactionProvider + Send + 'static,
    C: EthereumClient + Send + Sync + 'static,
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
        });
    }

    async fn poll_next_main_chain(&mut self) {
        while let Some(transactions) = self.client.get_transactions(self.next_ethereum_block).await
        {
            let mut provider = self.transaction_provider.lock().unwrap();

            provider.sync();
            let quotes = provider.get_quote_txs();

            let mut witness_txs: Vec<WitnessTx> = vec![];

            for transaction in transactions {
                if let Some(recipient) = transaction.to.as_ref() {
                    let recipient = recipient.to_string();
                    let quote = quotes.iter().find(|quote| {
                        quote.input == Coin::ETH && quote.input_address.0 == recipient
                    });

                    if quote.is_none() {
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

#[cfg(test)]
mod test {
    use super::*;
    use crate::utils::test_utils::ethereum::TestEthereumClient;
    use crate::{
        common::{
            ethereum::{Address, Hash, Transaction},
            Timestamp, WalletAddress,
        },
        transactions::{QuoteId, QuoteTx},
        utils::test_utils::transaction_provider::TestTransactionProvider,
    };
    use rand::Rng;

    struct TestObjects {
        client: Arc<TestEthereumClient>,
        provider: Arc<Mutex<TestTransactionProvider>>,
        witness: EthereumWitness<TestTransactionProvider, TestEthereumClient>,
    }

    fn setup() -> TestObjects {
        let client = Arc::new(TestEthereumClient::new());
        let provider = Arc::new(Mutex::new(TestTransactionProvider::new()));
        let witness = EthereumWitness::new(client.clone(), provider.clone());

        TestObjects {
            client,
            provider,
            witness,
        }
    }

    fn get_quote(id: u64, input: Coin, input_address: &str) -> QuoteTx {
        QuoteTx {
            id: QuoteId::new(id),
            timestamp: Timestamp::now(),
            input,
            output: Coin::BTC,
            input_address: WalletAddress::new(input_address),
            return_address: WalletAddress::new("return"),
        }
    }

    fn generate_eth_address() -> Address {
        Address(rand::thread_rng().gen::<[u8; 20]>())
    }

    #[tokio::test]
    async fn adds_witness_transaction_correctly() {
        let TestObjects {
            client,
            provider,
            mut witness,
        } = setup();

        let input_address = generate_eth_address();

        // Add a quote so we can witness it
        let eth_quote = get_quote(0, Coin::ETH, &input_address.to_string()[..]);
        let btc_quote = get_quote(1, Coin::BTC, &input_address.to_string()[..]);

        {
            let mut provider = provider.lock().unwrap();
            provider
                .add_transactions(vec![
                    SideChainTx::QuoteTx(eth_quote.clone()),
                    SideChainTx::QuoteTx(btc_quote),
                ])
                .unwrap();

            assert_eq!(provider.get_quote_txs().len(), 2);
            assert_eq!(provider.get_witness_txs().len(), 0);
        }

        // Create the main chain transactions
        let eth_transaction = Transaction {
            hash: Hash(rand::thread_rng().gen::<[u8; 32]>()),
            index: 0,
            block_number: 0,
            from: generate_eth_address(),
            to: Some(input_address),
            value: 100,
        };

        let another_eth_transaction = Transaction {
            hash: Hash(rand::thread_rng().gen::<[u8; 32]>()),
            index: 1,
            block_number: 0,
            from: generate_eth_address(),
            to: Some(generate_eth_address()),
            value: 100,
        };

        client.add_block(vec![eth_transaction.clone(), another_eth_transaction]);

        // Poll!
        witness.poll_next_main_chain().await;

        let provider = provider.lock().unwrap();

        assert_eq!(provider.get_quote_txs().len(), 2);
        assert_eq!(
            provider.get_witness_txs().len(),
            1,
            "Expected a witness transaction to be added"
        );

        let witness_tx = provider
            .get_witness_txs()
            .first()
            .expect("Expected witness tx to exist");

        assert_eq!(witness_tx.quote_id, eth_quote.id);
        assert_eq!(witness_tx.transaction_id, eth_transaction.hash.to_string());
        assert_eq!(witness_tx.transaction_index, eth_transaction.index);
        assert_eq!(
            witness_tx.transaction_block_number,
            eth_transaction.block_number
        );
        assert_eq!(witness_tx.amount, eth_transaction.value);
        assert_eq!(witness_tx.sender, Some(eth_transaction.from.to_string()));
    }
}
