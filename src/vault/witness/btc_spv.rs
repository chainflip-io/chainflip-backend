use crate::{
    common::{Coin, PoolCoin, Timestamp},
    side_chain::{SideChainTx, SubstrateNodeI},
    transactions::WitnessTx,
    vault::{blockchain_connection::btc::BitcoinSPVClient, transactions::TransactionProvider},
};

use parking_lot::RwLock;

use std::sync::Arc;

/// A Bitcoin transaction witness
pub struct BtcSPVWitness<T, C, S>
where
    T: TransactionProvider,
    C: BitcoinSPVClient,
    S: SubstrateNodeI,
{
    transaction_provider: Arc<RwLock<T>>,
    node: Arc<RwLock<S>>,
    client: Arc<C>,
}

/// How much of this code can be shared between chains??
impl<T, C, S> BtcSPVWitness<T, C, S>
where
    T: TransactionProvider + Send + Sync + 'static,
    C: BitcoinSPVClient + Send + Sync + 'static,
    S: SubstrateNodeI + Send + Sync + 'static,
{
    /// Create a new bitcoin chain witness
    pub fn new(client: Arc<C>, transaction_provider: Arc<RwLock<T>>, node: Arc<RwLock<S>>) -> Self {
        BtcSPVWitness {
            client,
            node,
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
            let swaps = provider.get_quote_txs();
            let stakes = provider.get_stake_quote_txs();

            let swap_id_address_pairs = swaps
                .iter()
                .filter(|quote| quote.inner.input == Coin::BTC)
                .map(|quote| {
                    let quote_inner = &quote.inner;
                    (quote_inner.id, quote_inner.input_address.clone())
                });

            let stake_id_address_pairs = stakes
                .iter()
                .filter(|quote| quote.inner.coin_type == PoolCoin::BTC)
                .map(|quote| {
                    let quote_inner = &quote.inner;
                    (quote_inner.id, quote_inner.coin_input_address.clone())
                });

            let mut witness_txs: Vec<SideChainTx> = vec![];
            for (id, address) in swap_id_address_pairs.chain(stake_id_address_pairs) {
                let utxos = match self.client.get_address_unspent(&address).await {
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
                    let tx = WitnessTx::new(
                        Timestamp::now(),
                        id,
                        utxo.tx_hash.clone(),
                        utxo.height,
                        utxo.tx_pos,
                        utxo.value as u128,
                        Coin::BTC,
                    );

                    witness_txs.push(tx.into());
                }
            }
            witness_txs
        };

        if witness_txs.len() > 0 {
            self.node.write().submit_txs(witness_txs);
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        side_chain::FakeSubstrateNode, utils::test_utils::btc::TestBitcoinSPVClient,
        utils::test_utils::create_fake_stake_quote,
        vault::blockchain_connection::btc::spv::BtcUTXO,
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
        witness: BtcSPVWitness<
            TestTransactionsProvider,
            TestBitcoinSPVClient,
            FakeSubstrateNode<TestTransactionsProvider>,
        >,
    }

    const BTC_PUBKEY: &str = "msjFLavJYLoF3hs3rgTrmBanpaHntjDgWQ";

    fn setup() -> TestObjects {
        let client = Arc::new(TestBitcoinSPVClient::new());
        let provider = Arc::new(RwLock::new(get_transactions_provider()));

        let node = Arc::new(RwLock::new(FakeSubstrateNode::new(provider.clone())));
        let witness = BtcSPVWitness::new(client.clone(), provider.clone(), node);

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
        client.add_utxos_for_address(BTC_PUBKEY.to_string(), utxos);

        // this quote will be witnessed
        let btc_quote =
            create_fake_quote_tx_coin_to_loki(Coin::BTC, WalletAddress(BTC_PUBKEY.to_string()));

        {
            let mut provider = provider.write();
            provider.add_transactions(vec![btc_quote.into()]).unwrap();

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
    async fn polling_stake_address_with_utxos_creates_witness_txs() {
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
        let mut btc_stake_quote = create_fake_stake_quote(PoolCoin::BTC);
        btc_stake_quote.coin_input_address = WalletAddress(BTC_PUBKEY.to_string());

        {
            let mut provider = provider.write();
            provider
                .add_transactions(vec![btc_stake_quote.into()])
                .unwrap();

            assert_eq!(provider.get_stake_quote_txs().len(), 1);
            assert_eq!(provider.get_witness_txs().len(), 0);
        }

        witness.poll_addresses_of_quotes().await;

        let provider = provider.read();

        assert_eq!(provider.get_stake_quote_txs().len(), 1);
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

        let btc_stake_quote = create_fake_stake_quote(PoolCoin::BTC);

        {
            let mut provider = provider.write();
            provider
                .add_transactions(vec![btc_quote.into(), btc_stake_quote.into()])
                .unwrap();

            assert_eq!(provider.get_quote_txs().len(), 1);
            assert_eq!(provider.get_stake_quote_txs().len(), 1);
            assert_eq!(provider.get_witness_txs().len(), 0);
        }

        witness.poll_addresses_of_quotes().await;

        let provider = provider.read();

        assert_eq!(provider.get_quote_txs().len(), 1);
        assert_eq!(provider.get_stake_quote_txs().len(), 1);
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
        let eth_stake_quote = create_fake_stake_quote(PoolCoin::ETH);

        {
            let mut provider = provider.write();
            provider
                .add_transactions(vec![eth_quote.into(), eth_stake_quote.into()])
                .unwrap();

            assert_eq!(provider.get_quote_txs().len(), 1);
            assert_eq!(provider.get_stake_quote_txs().len(), 1);
            assert_eq!(provider.get_witness_txs().len(), 0);
        }

        witness.poll_addresses_of_quotes().await;

        let provider = provider.read();

        assert_eq!(provider.get_quote_txs().len(), 1);
        assert_eq!(provider.get_stake_quote_txs().len(), 1);
        assert_eq!(provider.get_witness_txs().len(), 0);
    }
}
