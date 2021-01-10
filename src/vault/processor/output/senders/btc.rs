use super::{
    get_input_id_indices, group_outputs_by_sending_amounts,
    wallet_utils::{get_sending_wallets, WalletBalance},
    OutputSender,
};
use crate::{
    common::{GenericCoinAmount, WalletAddress},
    utils::{
        address::generate_btc_address,
        bip44::{self, RawKey},
        primitives::U256,
    },
    vault::blockchain_connection::btc::{IBitcoinSend, SendTransaction},
    vault::transactions::TransactionProvider,
};
use bip44::KeyPair;
use bitcoin::Address;
use chainflip_common::types::{
    chain::{Output, OutputSent, Validate},
    coin::Coin,
    Network, Timestamp, UUIDv4,
};
use hdwallet::ExtendedPrivKey;
use itertools::Itertools;
use parking_lot::RwLock;
use std::{convert::TryFrom, str::FromStr, sync::Arc};

/// An output send for Bitcoin
pub struct BtcOutputSender<B: IBitcoinSend, T: TransactionProvider> {
    client: B,
    root_private_key: ExtendedPrivKey,
    provider: Arc<RwLock<T>>,
    net_type: Network,
}

impl<B: IBitcoinSend, T: TransactionProvider> BtcOutputSender<B, T> {
    /// Create new BtcOutputSender
    pub fn new(client: B, provider: Arc<RwLock<T>>, root_key: RawKey, net_type: Network) -> Self {
        let root_private_key = root_key
            .to_private_key()
            .expect("Failed to generate bitcoin extended private key");

        Self {
            client,
            root_private_key,
            provider,
            net_type,
        }
    }

    async fn send_outputs_inner(
        &self,
        outputs: &[&Output],
        key_pair: &KeyPair,
    ) -> Result<OutputSent, String> {
        if outputs.is_empty() {
            return Err("Empty outputs".to_owned());
        }

        let total_amount = outputs.iter().fold(U256::from(0), |amount, tx| {
            amount.saturating_add(tx.amount.into())
        });
        let total_amount = match u128::try_from(total_amount) {
            Ok(amount) => amount,
            Err(_) => return Err("Total output amount is greater than u128::Max".to_owned()),
        };

        let address = &outputs.first().unwrap().address;
        if !outputs.iter().all(|tx| &tx.address == address) {
            return Err("Received BTC outputs with different addresses!".to_owned());
        }

        let btc_address = match Address::from_str(&address.to_string()) {
            Ok(address) => address,
            Err(_) => {
                return Err(format!(
                    "Failed to convert to bitcoin address: {:?}",
                    address
                ));
            }
        };

        // Send!
        let transaction = SendTransaction {
            from: key_pair.clone(),
            to: btc_address.clone(),
            amount: GenericCoinAmount::from_atomic(Coin::BTC, total_amount),
        };

        let tx_hash = match self.client.send(&transaction).await {
            Ok(hash) => hash,
            Err(err) => return Err(format!("Failed to send BTC transaction: {}", err)),
        };

        let uuids = outputs.iter().map(|tx| tx.id).collect_vec();

        let sent = OutputSent {
            id: UUIDv4::new(),
            timestamp: Timestamp::now(),
            outputs: uuids,
            coin: Coin::BTC,
            address: address.clone(),
            amount: total_amount,
            // Fee is already taken from the send amount when sent
            fee: 0,
            transaction_id: tx_hash.to_string().into(),
        };

        match sent.validate(self.net_type) {
            Ok(_) => Ok(sent),
            // Panic here because we sent money but didn't record it into the system
            Err(err) => panic!(
                "Failed to create output tx for {:?} with hash {}: {}",
                outputs, tx_hash, err
            ),
        }
    }

    async fn send_outputs(&self, outputs: &[Output], key_pair: &KeyPair) -> Vec<OutputSent> {
        let mut sent_txs: Vec<OutputSent> = vec![];

        // Split outputs into chunks of u128
        let groups = group_outputs_by_sending_amounts(outputs);
        for (_, outputs) in groups {
            match self.send_outputs_inner(&outputs, key_pair).await {
                Ok(sent_tx) => sent_txs.push(sent_tx),
                Err(err) => {
                    error!("{}", err);
                    debug!("{:?}", outputs);
                    continue;
                }
            }
        }

        sent_txs
    }
}

#[async_trait]
impl<B: IBitcoinSend + Sync + Send, T: TransactionProvider + Sync + Send> OutputSender
    for BtcOutputSender<B, T>
{
    async fn send(&self, outputs: &[Output]) -> Vec<OutputSent> {
        if (outputs.is_empty()) {
            return vec![];
        }

        if let Some(tx) = outputs.iter().find(|quote| quote.coin != Coin::BTC) {
            error!("Invalid output {:?} sent to BTC output sender", tx);
            return vec![];
        }

        let keys = get_input_id_indices(self.provider.clone(), Coin::BTC)
            .into_iter()
            .filter_map(|index| {
                match bip44::get_key_pair(
                    self.root_private_key.clone(),
                    bip44::CoinType::BTC,
                    index,
                ) {
                    Ok(keys) => Some(keys),
                    Err(err) => {
                        error!(
                            "Failed to generate btc key pair for index {}: {}",
                            index, err
                        );
                        None
                    }
                }
            });

        // Don't know how we can do this in parallel
        // futures::future::join_all(keys).await doesn't seems to work
        let mut wallet_balances = vec![];
        for key in keys {
            let public_key = match generate_btc_address(
                key.clone(),
                true,
                bitcoin::AddressType::P2wpkh,
                self.net_type,
            ) {
                Ok(address) => address,
                Err(err) => {
                    warn!("Failed to generate bitcoin address: {}", err);
                    continue;
                }
            };

            match self
                .client
                .get_address_balance(WalletAddress::new(&public_key))
                .await
            {
                Ok(balance) => wallet_balances.push(WalletBalance::new(key, balance.to_atomic())),
                Err(err) => {
                    warn!("Failed to fetch balance for {}: {}", public_key, err);
                }
            };
        }
        let wallet_outputs = get_sending_wallets(&wallet_balances, outputs);

        let mut sent_txs: Vec<OutputSent> = vec![];
        for output in wallet_outputs {
            let sent = self.send_outputs(&[output.output], &output.wallet).await;
            sent_txs.extend(sent);
        }
        sent_txs
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        side_chain::MemorySideChain,
        utils::test_utils::{
            btc::TestBitcoinSendClient, data::TestData, TEST_BTC_ADDRESS, TEST_ROOT_KEY,
        },
        vault::transactions::MemoryTransactionsProvider,
    };
    use std::sync::Mutex;

    fn get_key_pair() -> KeyPair {
        KeyPair::from_private_key(
            "58a99f6e6f89cbbb7fc8c86ea95e6012b68a9cd9a41c4ffa7c8f20c201d0667f",
        )
        .unwrap()
    }

    fn get_output_sender(
    ) -> BtcOutputSender<TestBitcoinSendClient, MemoryTransactionsProvider<MemorySideChain>> {
        let side_chain = MemorySideChain::new();
        let side_chain = Arc::new(Mutex::new(side_chain));
        let provider = MemoryTransactionsProvider::new_protected(side_chain.clone());
        let client = TestBitcoinSendClient::new();
        let key = RawKey::decode(TEST_ROOT_KEY).unwrap();
        BtcOutputSender::new(client, provider, key, Network::Testnet)
    }

    #[tokio::test]
    async fn send_outputs_inner_throws_error() {
        let mut sender = get_output_sender();
        let key_pair = get_key_pair();

        assert_eq!(
            &sender.send_outputs_inner(&[], &key_pair).await.unwrap_err(),
            "Empty outputs"
        );

        // =================

        let output_1 = TestData::output(Coin::BTC, u128::MAX);
        let output_2 = TestData::output(Coin::BTC, 100);

        assert_eq!(
            &sender
                .send_outputs_inner(&[&output_1, &output_2], &key_pair)
                .await
                .unwrap_err(),
            "Total output amount is greater than u128::Max"
        );

        // =================

        let output_1 = TestData::output(Coin::BTC, 100);
        let mut output_2 = TestData::output(Coin::BTC, 100);
        output_2.address = "different".into();

        assert_eq!(
            &sender
                .send_outputs_inner(&[&output_1, &output_2], &key_pair)
                .await
                .unwrap_err(),
            "Received BTC outputs with different addresses!"
        );

        // =================

        let mut output_1 = TestData::output(Coin::BTC, 100);
        let mut output_2 = TestData::output(Coin::BTC, 100);
        output_1.address = "invalid".into();
        output_2.address = output_1.address.clone();

        assert!(sender
            .send_outputs_inner(&[&output_1, &output_2], &key_pair)
            .await
            .unwrap_err()
            .contains("Failed to convert to bitcoin address"));

        // =================
        // Fee higher than or equal to the amount

        let output_1 = TestData::output(Coin::BTC, 10);

        sender.client.set_send_handler(|_| Err("Send Error".into()));

        assert_eq!(
            &sender
                .send_outputs_inner(&[&output_1], &key_pair)
                .await
                .unwrap_err(),
            "Failed to send BTC transaction: Send Error"
        );
    }

    #[tokio::test]
    async fn send_outputs_inner_returns_output_sent_tx_on_success() {
        let mut sender = get_output_sender();
        let key_pair = get_key_pair();
        let key_pair_public_key = (&key_pair.public_key).to_string();

        let mut output_1 = TestData::output(Coin::BTC, 100);
        let mut output_2 = TestData::output(Coin::BTC, 200);

        // bitcoin address (old) are case sensitive
        let address = TEST_BTC_ADDRESS;
        output_1.address = address.into();
        output_2.address = address.into();

        // random testnet transaction id
        let hash = "525fed92644fbd91b3bd183ed2134acd0ca1aaa4fa0fb714c2d1322be550fefe";

        sender.client.set_send_handler(move |tx| {
            assert_eq!(tx.from.public_key.to_string(), key_pair_public_key);
            assert_eq!(&tx.to.to_string(), address);
            assert_eq!(tx.amount.to_atomic(), 300);

            Ok(bitcoin::Txid::from_str(hash).unwrap())
        });

        let sent_tx = sender
            .send_outputs_inner(&[&output_1, &output_2], &key_pair)
            .await
            .unwrap();

        assert_eq!(sent_tx.outputs, vec![output_1.id, output_2.id]);
        assert_eq!(sent_tx.coin, Coin::BTC);
        assert_eq!(sent_tx.address, address.into());
        assert_eq!(sent_tx.amount, 300);
        // Fee is sent from the send amount
        assert_eq!(sent_tx.fee, 0);
        assert_eq!(sent_tx.transaction_id, hash.into());
    }

    #[tokio::test]
    async fn send_outputs_skips_outputs_with_errors() {
        let mut sender = get_output_sender();
        let key_pair = get_key_pair();

        // Make sure send_outputs splits these outputs in 2 distinct sends
        let output_1 = TestData::output(Coin::BTC, u128::MAX);
        let output_2 = TestData::output(Coin::BTC, 200);

        sender.client.set_send_handler(|tx| {
            // Make output_1 fail
            if tx.amount.to_atomic() != 200 {
                Err("Oh no! send error".to_owned())
            } else {
                // random testnet transaction id
                Ok(bitcoin::Txid::from_str(
                    "525fed92644fbd91b3bd183ed2134acd0ca1aaa4fa0fb714c2d1322be550fefe",
                )
                .unwrap())
            }
        });

        let sent = sender
            .send_outputs(&[output_1, output_2.clone()], &key_pair)
            .await;
        assert_eq!(sent.len(), 1);

        let first = sent.first().unwrap();
        assert_eq!(&first.outputs, &[output_2.id]);
        assert_eq!(first.amount, 200);
        assert_eq!(first.fee, 0);
    }
}
