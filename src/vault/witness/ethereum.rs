use crate::{
    common::store::KeyValueStore,
    local_store::{LocalEvent},
    vault::{blockchain_connection::ethereum::EthereumClient, transactions::TransactionProvider},
};
use chainflip_common::types::{chain::Witness, coin::Coin, Timestamp, UUIDv4};
use parking_lot::RwLock;
use std::sync::{Arc, Mutex};

/// The block that we should start scanning from if we're starting the witness from scratch.
/// There's no reason to scan from a block before chainflip launch.
const START_BLOCK: u64 = 9079997;

/// The db key for fetching and storing the next eth block
const NEXT_ETH_BLOCK_KEY: &'static str = "next_eth_block";

/// A ethereum transaction witness
pub struct EthereumWitness<T, C, S>
where
    T: TransactionProvider,
    C: EthereumClient,
    S: KeyValueStore,
{
    transaction_provider: Arc<RwLock<T>>,
    client: Arc<C>,
    store: Arc<Mutex<S>>,
    next_ethereum_block: u64,
}

impl<T, C, S> EthereumWitness<T, C, S>
where
    T: TransactionProvider + Send + Sync + 'static,
    C: EthereumClient + Send + Sync + 'static,
    S: KeyValueStore + Send + 'static,
{
    /// Create a new ethereum chain witness
    pub fn new(
        client: Arc<C>,
        transaction_provider: Arc<RwLock<T>>,
        store: Arc<Mutex<S>>,
    ) -> Self {
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

            std::thread::sleep(std::time::Duration::from_secs(10));
        }
    }

    /// Start witnessing the ethereum chain on a new thread
    pub fn start(mut self) {
        std::thread::spawn(move || {
            let mut rt = tokio::runtime::Runtime::new().unwrap();

            rt.block_on(async {
                self.event_loop().await;
            });
        });
    }

    async fn poll_next_main_chain(&mut self) {
        // TODO: Only add witnesses once we have a certain amount of confirmations
        // To facilitate this we'd have to poll blocks up to current_block - num_of_confirmations
        while let Some(transactions) = self.client.get_transactions(self.next_ethereum_block).await
        {
            debug!(
                "Got {} new ETH Transactions for block {}",
                transactions.len(),
                self.next_ethereum_block
            );

            let mut provider = self.transaction_provider.write();

            provider.sync();
            let swaps = provider.get_swap_quotes();
            let deposit_quotes = provider.get_deposit_quotes();

            let mut witness_txs: Vec<LocalEvent> = vec![];

            for transaction in transactions {
                if let Some(recipient) = transaction.to {
                    let recipient = recipient.to_string();
                    let swap_quote = swaps.iter().find(|quote_info| {
                        let quote = &quote_info.inner;
                        quote.input == Coin::ETH
                            && quote.input_address.to_string().to_lowercase()
                                == recipient.to_lowercase()
                    });

                    let deposit_quote = deposit_quotes.iter().find(|quote_info| {
                        let quote = &quote_info.inner;
                        quote.pool == Coin::ETH
                            && quote.coin_input_address.to_string().to_lowercase()
                                == recipient.to_lowercase()
                    });

                    let quote_id = {
                        let swap_id = swap_quote.map(|q| q.inner.id);
                        let deposit_id = deposit_quote.map(|q| q.inner.id);

                        swap_id.or(deposit_id)
                    };

                    if quote_id.is_none() {
                        continue;
                    }

                    let quote_id = quote_id.unwrap();

                    debug!("Publishing witness for quote: {}", &quote_id);

                    let tx = Witness {
                        id: UUIDv4::new(),
                        timestamp: Timestamp::now(),
                        quote: quote_id,
                        transaction_id: transaction.hash.to_string().into(),
                        transaction_block_number: transaction.block_number,
                        transaction_index: transaction.index,
                        amount: transaction.value,
                        coin: Coin::ETH,
                    };

                    if tx.amount > 0 {
                        witness_txs.push(tx.into());
                    }
                }
            }

            drop(provider);

            if witness_txs.len() > 0 {
                self.transaction_provider.write().add_transactions(witness_txs);
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
    use crate::{
        common::ethereum::{Hash, Transaction},
        local_store::MemorySideChain,
        utils::test_utils::{data::TestData, get_transactions_provider, store::MemoryKVS},
        vault::transactions::MemoryTransactionsProvider,
    };
    use crate::{utils::test_utils::ethereum::TestEthereumClient};
    use chainflip_common::types::addresses::EthereumAddress;
    use rand::Rng;

    type TestTransactionsProvider = MemoryTransactionsProvider<MemorySideChain>;

    struct TestObjects {
        client: Arc<TestEthereumClient>,
        provider: Arc<RwLock<TestTransactionsProvider>>,
        store: Arc<Mutex<MemoryKVS>>,
        witness: EthereumWitness<
            TestTransactionsProvider,
            TestEthereumClient,
            MemoryKVS,
        >,
    }

    fn setup() -> TestObjects {
        let client = Arc::new(TestEthereumClient::new());
        let provider = Arc::new(RwLock::new(get_transactions_provider()));
        let store = Arc::new(Mutex::new(MemoryKVS::new()));
        let witness = EthereumWitness::new(client.clone(), provider.clone(), store.clone(), node);

        TestObjects {
            client,
            provider,
            store,
            witness,
        }
    }

    fn generate_eth_address() -> EthereumAddress {
        EthereumAddress(rand::thread_rng().gen::<[u8; 20]>())
    }

    #[tokio::test]
    async fn adds_swap_quote_witness_transaction_correctly() {
        let params = setup();
        let client = params.client;
        let provider = params.provider;
        let mut witness = params.witness;

        let input_address = generate_eth_address();

        // Add a quote so we can witness it
        let mut eth_quote = TestData::swap_quote(Coin::ETH, Coin::LOKI);
        eth_quote.input_address = input_address.to_string().to_lowercase().into();

        let btc_quote = TestData::swap_quote(Coin::BTC, Coin::LOKI);

        {
            let mut provider = provider.write();
            provider
                .add_transactions(vec![eth_quote.clone().into(), btc_quote.into()])
                .unwrap();

            assert_eq!(provider.get_swap_quotes().len(), 2);
            assert_eq!(provider.get_witnesses().len(), 0);
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
        let provider = provider.read();

        assert_eq!(provider.get_swap_quotes().len(), 2);
        assert_eq!(
            provider.get_witnesses().len(),
            1,
            "Expected a witness to be added"
        );

        let witness_tx = &provider
            .get_witnesses()
            .first()
            .expect("Expected witness to exist")
            .inner;

        assert_eq!(witness_tx.quote, eth_quote.id);
        assert_eq!(
            witness_tx.transaction_id.to_string(),
            eth_transaction.hash.to_string()
        );
        assert_eq!(witness_tx.transaction_index, eth_transaction.index);
        assert_eq!(
            witness_tx.transaction_block_number,
            eth_transaction.block_number
        );
        assert_eq!(witness_tx.amount, eth_transaction.value);
    }

    #[tokio::test]
    async fn adds_deposit_quote_witness_transaction_correctly() {
        let params = setup();
        let client = params.client;
        let provider = params.provider;
        let mut witness = params.witness;

        let input_address = generate_eth_address();

        // Add a quote so we can witness it
        let mut eth_deposit_quote = TestData::deposit_quote(Coin::ETH);
        eth_deposit_quote.coin_input_address = input_address.to_string().to_lowercase().into();

        let btc_deposit_quote = TestData::deposit_quote(Coin::BTC);

        {
            let mut provider = provider.write();
            provider
                .add_transactions(vec![
                    eth_deposit_quote.clone().into(),
                    btc_deposit_quote.into(),
                ])
                .unwrap();

            assert_eq!(provider.get_deposit_quotes().len(), 2);
            assert_eq!(provider.get_witnesses().len(), 0);
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
        let provider = provider.read();

        assert_eq!(provider.get_deposit_quotes().len(), 2);
        assert_eq!(
            provider.get_witnesses().len(),
            1,
            "Expected a witness to be added"
        );

        let witness_tx = &provider
            .get_witnesses()
            .first()
            .expect("Expected witness to exist")
            .inner;

        assert_eq!(witness_tx.quote, eth_deposit_quote.id);
        assert_eq!(
            witness_tx.transaction_id.to_string(),
            eth_transaction.hash.to_string()
        );
        assert_eq!(witness_tx.transaction_index, eth_transaction.index);
        assert_eq!(
            witness_tx.transaction_block_number,
            eth_transaction.block_number
        );
        assert_eq!(witness_tx.amount, eth_transaction.value);
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
