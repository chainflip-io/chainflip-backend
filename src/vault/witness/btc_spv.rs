use crate::{
    common::{Coin, Timestamp},
    side_chain::SideChainTx,
    transactions::WitnessTx,
    vault::{blockchain_connection::btc::BitcoinSPVClient, transactions::TransactionProvider},
};

use parking_lot::RwLock;
use uuid::Uuid;

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

            std::thread::sleep(std::time::Duration::from_millis(10));
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
            let quotes = provider.get_quote_txs();

            let mut witness_txs: Vec<WitnessTx> = vec![];
            for quote in quotes.iter().filter(|quote| quote.inner.input == Coin::BTC) {
                let quote_inner = &quote.inner;
                let input_addr = &quote_inner.input_address;

                let utxos = match self.client.get_address_unspent(input_addr).await {
                    Ok(utxos) => utxos,
                    Err(err) => {
                        warn!(
                            "Could not fetch UTXOs for bitcoin address: {}. Error: {}",
                            &input_addr, err
                        );
                        continue;
                    }
                };

                if utxos.0.len() == 0 {
                    // no inputs to this address
                    continue;
                }

                for utxo in utxos.0 {
                    let tx = WitnessTx::new(
                        Timestamp::now(),
                        quote_inner.id,
                        utxo.tx_hash.clone(),
                        utxo.height,
                        utxo.tx_pos,
                        utxo.value as u128,
                        Coin::BTC,
                    );

                    witness_txs.push(tx);
                }
            }
            witness_txs
        };

        if witness_txs.len() > 0 {
            let side_chain_txs = witness_txs
                .into_iter()
                .map(SideChainTx::WitnessTx)
                .collect();

            self.transaction_provider
                .write()
                .add_transactions(side_chain_txs)
                .expect("Could not publish witness txs");
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        utils::test_utils::btc::TestBitcoinSPVClient,
        vault::blockchain_connection::btc::btc_spv::BtcUTXO,
    };

    use crate::{
        common::WalletAddress,
        side_chain::MemorySideChain,
        utils::test_utils::{create_fake_quote_tx_coin_to_loki, get_transactions_provider},
        vault::transactions::MemoryTransactionsProvider,
    };

    type TestTransactionsProvider = MemoryTransactionsProvider<MemorySideChain>;
    struct TestObjects {
        client: Arc<TestBitcoinSPVClient>,
        provider: Arc<RwLock<TestTransactionsProvider>>,
        witness: BtcSPVWitness<TestTransactionsProvider, TestBitcoinSPVClient>,
    }

    const BTC_PUBKEY: &str = "msjFLavJYLoF3hs3rgTrmBanpaHntjDgWQ";

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
    async fn polling_address_with_utxos_creates_witness_txs() {
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
        client.add_utxos_for_address(BTC_PUBKEY.to_string(), utxos);

        // this quote will be witnessed
        let btc_quote =
            create_fake_quote_tx_coin_to_loki(Coin::BTC, WalletAddress(BTC_PUBKEY.to_string()));

        {
            let mut provider = provider.write();
            provider
                .add_transactions(vec![btc_quote.clone().into()])
                .unwrap();

            assert_eq!(provider.get_quote_txs().len(), 1);
            assert_eq!(provider.get_witness_txs().len(), 0);
        }

        witness.poll_addresses_of_quotes().await;

        let provider = provider.read();

        assert_eq!(provider.get_quote_txs().len(), 1);
        // one witness tx for each utxo
        assert_eq!(provider.get_witness_txs().len(), 2);
    }

    #[tokio::test]
    async fn polling_address_without_utxos_does_not_create_witness_txs() {
        let params = setup();
        let mut witness = params.witness;
        let provider = params.provider;

        // this quote will be witnessed
        let btc_quote =
            create_fake_quote_tx_coin_to_loki(Coin::BTC, WalletAddress(BTC_PUBKEY.to_string()));

        {
            let mut provider = provider.write();
            provider
                .add_transactions(vec![btc_quote.clone().into()])
                .unwrap();

            assert_eq!(provider.get_quote_txs().len(), 1);
            assert_eq!(provider.get_witness_txs().len(), 0);
        }

        witness.poll_addresses_of_quotes().await;

        let provider = provider.read();

        assert_eq!(provider.get_quote_txs().len(), 1);

        assert_eq!(provider.get_witness_txs().len(), 0);
    }

    #[tokio::test]
    async fn polling_on_eth_quote_does_not_create_witness_txs() {
        let params = setup();
        let mut witness = params.witness;
        let provider = params.provider;

        // this quote should NOT be witnessed by the BTC witness since it's an ETH quote
        let eth_quote = create_fake_quote_tx_coin_to_loki(
            Coin::ETH,
            WalletAddress("0x70e7db0678460c5e53f1ffc9221d1c692111dcc5".to_string()),
        );

        {
            let mut provider = provider.write();
            provider
                .add_transactions(vec![eth_quote.clone().into()])
                .unwrap();

            assert_eq!(provider.get_quote_txs().len(), 1);
            assert_eq!(provider.get_witness_txs().len(), 0);
        }

        witness.poll_addresses_of_quotes().await;

        let provider = provider.read();

        assert_eq!(provider.get_quote_txs().len(), 1);

        assert_eq!(provider.get_witness_txs().len(), 0);
    }
}
