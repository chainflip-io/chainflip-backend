use crate::{
    common::WalletAddress,
    local_store::LocalEvent,
    vault::{blockchain_connection::btc::BitcoinSPVClient, transactions::TransactionProvider},
};
use chainflip_common::types::{chain::Witness, coin::Coin, unique_id::GetUniqueId};
use parking_lot::RwLock;
use std::sync::Arc;

/// A Bitcoin transaction witness
pub struct BtcSPVWitness<T, C>
where
    T: TransactionProvider,
    C: BitcoinSPVClient,
{
    transaction_provider: Arc<RwLock<T>>,
    client: Arc<C>,
}

/// How much of this code can be shared between chains??
impl<T, C> BtcSPVWitness<T, C>
where
    T: TransactionProvider + Send + Sync + 'static,
    C: BitcoinSPVClient + Send + Sync + 'static,
{
    /// Create a new bitcoin chain witness
    pub fn new(client: Arc<C>, transaction_provider: Arc<RwLock<T>>) -> Self {
        BtcSPVWitness {
            client,
            transaction_provider,
        }
    }

    async fn event_loop(&mut self) {
        loop {
            self.poll_addresses_of_quotes().await;

            std::thread::sleep(std::time::Duration::from_secs(10));
        }
    }

    /// Start witnessing the bitcoin chain on a new thread
    pub fn start(mut self) {
        std::thread::spawn(move || {
            let mut rt = tokio::runtime::Runtime::new().unwrap();

            rt.block_on(async {
                self.event_loop().await;
            });
        });
    }

    async fn poll_addresses_of_quotes(&mut self) {
        self.transaction_provider.write().sync();

        let witness_txs = {
            let provider = self.transaction_provider.read();
            let swaps = provider.get_swap_quotes();
            let deposit_quotes = provider.get_deposit_quotes();

            let swap_id_address_pairs = swaps
                .iter()
                .filter(|quote| quote.inner.input == Coin::BTC)
                .map(|quote| {
                    let quote_inner = &quote.inner;
                    (quote_inner.unique_id(), quote_inner.input_address.clone())
                });

            let deposit_id_address_pairs = deposit_quotes
                .iter()
                .filter(|quote| quote.inner.pool == Coin::BTC)
                .map(|quote| {
                    let quote_inner = &quote.inner;
                    (
                        quote_inner.unique_id(),
                        quote_inner.coin_input_address.clone(),
                    )
                });

            let mut witness_txs: Vec<LocalEvent> = vec![];
            for (id, address) in swap_id_address_pairs.chain(deposit_id_address_pairs) {
                let btc_address = WalletAddress(address.to_string());
                let utxos = match self.client.get_address_unspent(&btc_address).await {
                    Ok(utxos) => utxos,
                    Err(err) => {
                        warn!(
                            "Could not fetch UTXOs for bitcoin address: {}. Error: {}",
                            &address, err
                        );
                        continue;
                    }
                };

                if utxos.0.len() == 0 {
                    // no inputs to this address
                    continue;
                }

                for utxo in utxos.0 {
                    let tx = Witness {
                        quote: id,
                        transaction_id: utxo.tx_hash.into(),
                        transaction_block_number: utxo.height,
                        transaction_index: utxo.tx_pos,
                        amount: utxo.value as u128,
                        coin: Coin::BTC,
                        event_number: None,
                    };

                    witness_txs.push(tx.into());
                }
            }
            witness_txs
        };

        if witness_txs.len() > 0 {
            self.transaction_provider
                .write()
                .add_local_events(witness_txs)
                .unwrap();
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        local_store::MemoryLocalStore,
        utils::test_utils::{
            btc::TestBitcoinSPVClient, data::TestData, get_transactions_provider, TEST_BTC_ADDRESS,
        },
        vault::{
            blockchain_connection::btc::spv::BtcUTXO, transactions::MemoryTransactionsProvider,
        },
    };

    type TestTransactionsProvider = MemoryTransactionsProvider<MemoryLocalStore>;
    struct TestObjects {
        client: Arc<TestBitcoinSPVClient>,
        provider: Arc<RwLock<TestTransactionsProvider>>,
        witness: BtcSPVWitness<TestTransactionsProvider, TestBitcoinSPVClient>,
    }

    fn setup() -> TestObjects {
        let client = Arc::new(TestBitcoinSPVClient::new());
        let provider = Arc::new(RwLock::new(get_transactions_provider()));

        let witness = BtcSPVWitness::new(client.clone(), provider.clone());

        TestObjects {
            client,
            provider,
            witness,
        }
    }

    #[tokio::test]
    async fn polling_swap_address_with_utxos_creates_witness_txs() {
        let params = setup();
        let mut witness = params.witness;
        let provider = params.provider;
        let client = params.client;

        let utxo1 = BtcUTXO::new(
            250000,
            "a9ec47601a25f0cc27c63c78cab3d446294c5eccb171f3973ee9979c00bee432".to_string(),
            0,
            1000,
        );
        let utxo2 = BtcUTXO::new(
            250002,
            "b9ec47601a25f0cd27c63c78cab3d446294c5eccb171f3973ee9979c00bee442".to_string(),
            0,
            4000,
        );

        let utxos = vec![utxo1, utxo2];
        // add utxos to test client
        client.add_utxos_for_address(TEST_BTC_ADDRESS.to_string(), utxos);

        // this quote will be witnessed
        let btc_quote = TestData::swap_quote(Coin::BTC, Coin::OXEN);

        {
            let mut provider = provider.write();
            provider.add_local_events(vec![btc_quote.into()]).unwrap();

            assert_eq!(provider.get_swap_quotes().len(), 1);
            assert_eq!(provider.get_witnesses().len(), 0);
        }
        assert_eq!(provider.write().get_swap_quotes().len(), 1);
        witness.poll_addresses_of_quotes().await;

        let provider = provider.read();
        assert_eq!(provider.get_swap_quotes().len(), 1);
        // one witness for each utxo
        assert_eq!(provider.get_witnesses().len(), 2);
    }

    #[tokio::test]
    async fn polling_deposit_address_with_utxos_creates_witness_txs() {
        let params = setup();
        let mut witness = params.witness;
        let provider = params.provider;
        let client = params.client;

        let utxo1 = BtcUTXO::new(
            250000,
            "a9ec47601a25f0cc27c63c78cab3d446294c5eccb171f3973ee9979c00bee432".to_string(),
            0,
            1000,
        );
        let utxo2 = BtcUTXO::new(
            250002,
            "b9ec47601a25f0cd27c63c78cab3d446294c5eccb171f3973ee9979c00bee442".to_string(),
            0,
            4000,
        );

        let utxos = vec![utxo1, utxo2];
        // add utxos to test client
        client.add_utxos_for_address(TEST_BTC_ADDRESS.to_string(), utxos);

        // this quote will be witnessed
        let btc_deposit_quote = TestData::deposit_quote(Coin::BTC);

        {
            let mut provider = provider.write();
            provider
                .add_local_events(vec![btc_deposit_quote.into()])
                .unwrap();

            assert_eq!(provider.get_deposit_quotes().len(), 1);
            assert_eq!(provider.get_witnesses().len(), 0);
        }

        witness.poll_addresses_of_quotes().await;

        let provider = provider.read();

        assert_eq!(provider.get_deposit_quotes().len(), 1);
        // one witness for each utxo
        assert_eq!(provider.get_witnesses().len(), 2);
    }

    #[tokio::test]
    async fn polling_address_without_utxos_does_not_create_witness_txs() {
        let params = setup();
        let mut witness = params.witness;
        let provider = params.provider;

        // this quote will be witnessed
        let btc_quote = TestData::swap_quote(Coin::BTC, Coin::OXEN);
        let btc_deposit_quote = TestData::deposit_quote(Coin::BTC);

        {
            let mut provider = provider.write();
            provider
                .add_local_events(vec![btc_quote.into(), btc_deposit_quote.into()])
                .unwrap();

            assert_eq!(provider.get_swap_quotes().len(), 1);
            assert_eq!(provider.get_deposit_quotes().len(), 1);
            assert_eq!(provider.get_witnesses().len(), 0);
        }

        witness.poll_addresses_of_quotes().await;

        let provider = provider.read();

        assert_eq!(provider.get_swap_quotes().len(), 1);
        assert_eq!(provider.get_deposit_quotes().len(), 1);
        assert_eq!(provider.get_witnesses().len(), 0);
    }

    #[tokio::test]
    async fn polling_on_eth_quote_does_not_create_witness_txs() {
        let params = setup();
        let mut witness = params.witness;
        let provider = params.provider;

        // this quote should NOT be witnessed by the BTC witness since it's an ETH quote
        let eth_quote = TestData::swap_quote(Coin::ETH, Coin::OXEN);
        let eth_deposit_quote = TestData::deposit_quote(Coin::ETH);

        {
            let mut provider = provider.write();
            provider
                .add_local_events(vec![eth_quote.into(), eth_deposit_quote.into()])
                .unwrap();

            assert_eq!(provider.get_swap_quotes().len(), 1);
            assert_eq!(provider.get_deposit_quotes().len(), 1);
            assert_eq!(provider.get_witnesses().len(), 0);
        }

        witness.poll_addresses_of_quotes().await;

        let provider = provider.read();

        assert_eq!(provider.get_swap_quotes().len(), 1);
        assert_eq!(provider.get_deposit_quotes().len(), 1);
        assert_eq!(provider.get_witnesses().len(), 0);
    }
}
