use crate::{
    common::{store::KeyValueStore, Coin},
    side_chain::SideChainTx,
    transactions::WitnessTx,
    vault::{blockchain_connection::ethereum::EthereumClient, transactions::TransactionProvider},
};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

/// The block that we should start scanning from if we're starting the witness from scratch.
/// There's no reason to scan from a block before blockswap launch.
const START_BLOCK: u64 = 10746801;

/// The db key for fetching and storing the next eth block
const NEXT_ETH_BLOCK_KEY: &'static str = "next_eth_block";

/// A ethereum transaction witness
pub struct EthereumWitness<T, C, S>
where
    T: TransactionProvider,
    C: EthereumClient,
    S: KeyValueStore,
{
    transaction_provider: Arc<Mutex<T>>,
    client: Arc<C>,
    store: Arc<Mutex<S>>,
    next_ethereum_block: u64,
}

impl<T, C, S> EthereumWitness<T, C, S>
where
    T: TransactionProvider + Send + 'static,
    C: EthereumClient + Send + Sync + 'static,
    S: KeyValueStore + Send + 'static,
{
    /// Create a new ethereum chain witness
    pub fn new(client: Arc<C>, transaction_provider: Arc<Mutex<T>>, store: Arc<Mutex<S>>) -> Self {
        let next_ethereum_block = match store.lock().unwrap().get_data::<u64>(NEXT_ETH_BLOCK_KEY) {
            Some(next_block) => next_block,
            None => {
                warn!(
                    "Last block record not found for Eth witness, using default: {}",
                    START_BLOCK
                );
                START_BLOCK
            }
        };

        EthereumWitness {
            client,
            transaction_provider,
            store,
            next_ethereum_block,
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
                        id: Uuid::new_v4(),
                        quote_id: quote.id,
                        transaction_id: transaction.hash.to_string(),
                        transaction_block_number: transaction.block_number,
                        transaction_index: transaction.index,
                        amount: transaction.value,
                        coin_type: Coin::ETH,
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
            self.store
                .lock()
                .unwrap()
                .set_data(NEXT_ETH_BLOCK_KEY, Some(self.next_ethereum_block))
                .expect("Failed to store next ethereum block");
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
        transactions::QuoteTx,
        utils::test_utils::{store::MemoryKVS, transaction_provider::TestTransactionProvider},
    };
    use rand::Rng;

    struct TestObjects {
        client: Arc<TestEthereumClient>,
        provider: Arc<Mutex<TestTransactionProvider>>,
        store: Arc<Mutex<MemoryKVS>>,
        witness: EthereumWitness<TestTransactionProvider, TestEthereumClient, MemoryKVS>,
    }

    fn setup() -> TestObjects {
        let client = Arc::new(TestEthereumClient::new());
        let provider = Arc::new(Mutex::new(TestTransactionProvider::new()));
        let store = Arc::new(Mutex::new(MemoryKVS::new()));
        let witness = EthereumWitness::new(client.clone(), provider.clone(), store.clone());

        TestObjects {
            client,
            provider,
            store,
            witness,
        }
    }

    fn get_quote(id: u64, input: Coin, input_address: &str) -> QuoteTx {
        QuoteTx {
            id: Uuid::new_v4(),
            timestamp: Timestamp::now(),
            input,
            output: Coin::BTC,
            input_address: WalletAddress::new(input_address),
            input_address_id: "".to_owned(),
            return_address: WalletAddress::new("return"),
        }
    }

    fn generate_eth_address() -> Address {
        Address(rand::thread_rng().gen::<[u8; 20]>())
    }

    #[tokio::test]
    async fn adds_witness_transaction_correctly() {
        let params = setup();
        let client = params.client;
        let provider = params.provider;
        let mut witness = params.witness;

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

        // Check that transactions were added correctly
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

    #[tokio::test]
    async fn adds_witness_saves_next_ethereum_block() {
        let params = setup();
        let client = params.client;
        let store = params.store;
        let mut witness = params.witness;

        // Create the main chain transactions
        let eth_transaction = Transaction {
            hash: Hash(rand::thread_rng().gen::<[u8; 32]>()),
            index: 0,
            block_number: 0,
            from: generate_eth_address(),
            to: Some(generate_eth_address()),
            value: 100,
        };

        client.add_block(vec![eth_transaction]);

        // Pre conditions
        assert_eq!(witness.next_ethereum_block, START_BLOCK);
        assert!(store
            .lock()
            .unwrap()
            .get_data::<u64>(NEXT_ETH_BLOCK_KEY)
            .is_none());

        // Poll!
        witness.poll_next_main_chain().await;

        // Check that we correctly store the next ethereum block
        let next_block_key = store.lock().unwrap().get_data::<u64>(NEXT_ETH_BLOCK_KEY);
        assert_eq!(next_block_key, Some(witness.next_ethereum_block));
        assert_ne!(next_block_key, Some(START_BLOCK));
    }
}
