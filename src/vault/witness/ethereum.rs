use parking_lot::RwLock;
use uuid::Uuid;

use crate::{
    common::Timestamp,
    common::{store::KeyValueStore, Coin, PoolCoin},
    side_chain::SideChainTx,
    transactions::WitnessTx,
    vault::{blockchain_connection::ethereum::EthereumClient, transactions::TransactionProvider},
};
use std::sync::{Arc, Mutex};

/// The block that we should start scanning from if we're starting the witness from scratch.
/// There's no reason to scan from a block before blockswap launch.
const START_BLOCK: u64 = 8975000;

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
    pub fn new(client: Arc<C>, transaction_provider: Arc<RwLock<T>>, store: Arc<Mutex<S>>) -> Self {
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
            let swaps = provider.get_quote_txs();
            let stakes = provider.get_stake_quote_txs();

            let mut witness_txs: Vec<WitnessTx> = vec![];

            for transaction in transactions {
                if let Some(recipient) = transaction.to.as_ref() {
                    let recipient = recipient.to_string();
                    let swap_quote = swaps.iter().find(|quote_info| {
                        let quote = &quote_info.inner;
                        quote.input == Coin::ETH
                            && quote.input_address.0.to_lowercase() == recipient.to_lowercase()
                    });

                    let stake_quote = stakes.iter().find(|quote_info| {
                        let quote = &quote_info.inner;
                        quote.coin_type == PoolCoin::ETH
                            && quote.coin_input_address.0.to_lowercase() == recipient.to_lowercase()
                    });

                    let quote_id = {
                        let swap_id = swap_quote.map(|q| q.inner.id);
                        let stake_id = stake_quote.map(|q| q.inner.id);

                        swap_id.or(stake_id)
                    };

                    if quote_id.is_none() {
                        continue;
                    }

                    let quote_id = quote_id.unwrap();

                    debug!("Publishing witness transaction for quote: {}", &quote_id);

                    let tx = WitnessTx::new(
                        Timestamp::now(),
                        quote_id,
                        transaction.hash.to_string(),
                        transaction.block_number,
                        transaction.index,
                        transaction.value,
                        Coin::ETH,
                    );

                    if tx.amount > 0 {
                        witness_txs.push(tx);
                    }
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
    use crate::utils::test_utils::{create_fake_stake_quote, ethereum::TestEthereumClient};
    use crate::{
        common::{
            ethereum::{Address, Hash, Transaction},
            WalletAddress,
        },
        side_chain::MemorySideChain,
        utils::test_utils::{
            create_fake_quote_tx_coin_to_loki, get_transactions_provider, store::MemoryKVS,
        },
        vault::transactions::MemoryTransactionsProvider,
    };
    use rand::Rng;

    type TestTransactionsProvider = MemoryTransactionsProvider<MemorySideChain>;

    struct TestObjects {
        client: Arc<TestEthereumClient>,
        provider: Arc<RwLock<TestTransactionsProvider>>,
        store: Arc<Mutex<MemoryKVS>>,
        witness: EthereumWitness<TestTransactionsProvider, TestEthereumClient, MemoryKVS>,
    }

    fn setup() -> TestObjects {
        let client = Arc::new(TestEthereumClient::new());
        let provider = Arc::new(RwLock::new(get_transactions_provider()));
        let store = Arc::new(Mutex::new(MemoryKVS::new()));
        let witness = EthereumWitness::new(client.clone(), provider.clone(), store.clone());

        TestObjects {
            client,
            provider,
            store,
            witness,
        }
    }

    fn generate_eth_address() -> Address {
        Address(rand::thread_rng().gen::<[u8; 20]>())
    }

    #[tokio::test]
    async fn adds_swap_quote_witness_transaction_correctly() {
        let params = setup();
        let client = params.client;
        let provider = params.provider;
        let mut witness = params.witness;

        let input_address = generate_eth_address();

        // Add a quote so we can witness it
        let input_wallet_address = WalletAddress::new(&input_address.to_string().to_lowercase());
        let eth_quote = create_fake_quote_tx_coin_to_loki(Coin::ETH, input_wallet_address);
        let btc_quote = create_fake_quote_tx_coin_to_loki(
            Coin::BTC,
            WalletAddress::new("1BvBMSEYstWetqTFn5Au4m4GFg7xJaNVN2"),
        );

        {
            let mut provider = provider.write();
            provider
                .add_transactions(vec![eth_quote.clone().into(), btc_quote.into()])
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
        let provider = provider.read();

        assert_eq!(provider.get_quote_txs().len(), 2);
        assert_eq!(
            provider.get_witness_txs().len(),
            1,
            "Expected a witness transaction to be added"
        );

        let witness_tx = &provider
            .get_witness_txs()
            .first()
            .expect("Expected witness tx to exist")
            .inner;

        assert_eq!(witness_tx.quote_id, eth_quote.id);
        assert_eq!(witness_tx.transaction_id, eth_transaction.hash.to_string());
        assert_eq!(witness_tx.transaction_index, eth_transaction.index);
        assert_eq!(
            witness_tx.transaction_block_number,
            eth_transaction.block_number
        );
        assert_eq!(witness_tx.amount, eth_transaction.value);
    }

    #[tokio::test]
    async fn adds_stake_quote_witness_transaction_correctly() {
        let params = setup();
        let client = params.client;
        let provider = params.provider;
        let mut witness = params.witness;

        let input_address = generate_eth_address();
        let input_wallet_address = WalletAddress::new(&input_address.to_string().to_lowercase());

        // Add a quote so we can witness it
        let mut eth_stake_quote = create_fake_stake_quote(PoolCoin::ETH);
        eth_stake_quote.coin_input_address = input_wallet_address;

        let btc_stake_quote = create_fake_stake_quote(PoolCoin::BTC);

        {
            let mut provider = provider.write();
            provider
                .add_transactions(vec![eth_stake_quote.clone().into(), btc_stake_quote.into()])
                .unwrap();

            assert_eq!(provider.get_stake_quote_txs().len(), 2);
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
        let provider = provider.read();

        assert_eq!(provider.get_stake_quote_txs().len(), 2);
        assert_eq!(
            provider.get_witness_txs().len(),
            1,
            "Expected a witness transaction to be added"
        );

        let witness_tx = &provider
            .get_witness_txs()
            .first()
            .expect("Expected witness tx to exist")
            .inner;

        assert_eq!(witness_tx.quote_id, eth_stake_quote.id);
        assert_eq!(witness_tx.transaction_id, eth_transaction.hash.to_string());
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
