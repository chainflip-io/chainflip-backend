use std::{convert::TryFrom, str::FromStr, sync::Arc};

use bip44::KeyPair;
use hdwallet::ExtendedPrivKey;
use itertools::Itertools;
use parking_lot::RwLock;

use crate::{
    common::{ethereum::Address, Coin, GenericCoinAmount, Timestamp},
    transactions::{OutputSentTx, OutputTx},
    utils::{
        bip44::{self, RawKey},
        primitives::U256,
    },
    vault::{
        blockchain_connection::ethereum::{EstimateRequest, EthereumClient, SendTransaction},
        transactions::TransactionProvider,
    },
};

use super::{
    get_input_id_indicies, group_outputs_by_sending_amounts,
    wallet_utils::{get_sending_wallets, WalletBalance},
    OutputSender,
};

/// An output sender for Ethereum
pub struct EthOutputSender<E: EthereumClient, T: TransactionProvider> {
    client: E,
    root_private_key: ExtendedPrivKey,
    provider: Arc<RwLock<T>>,
}

impl<E: EthereumClient, T: TransactionProvider> EthOutputSender<E, T> {
    /// Create a new output sender
    pub fn new(client: E, provider: Arc<RwLock<T>>, root_key: RawKey) -> Self {
        let root_private_key = root_key
            .to_private_key()
            .expect("Failed to generate ethereum extended private key");

        Self {
            client,
            root_private_key,
            provider,
        }
    }

    async fn send_outputs_inner(
        &self,
        outputs: &[&OutputTx],
        key_pair: &KeyPair,
    ) -> Result<OutputSentTx, String> {
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
            return Err("Received eth outputs with different addresses!".to_owned());
        }

        let address = match Address::from_str(&address.0) {
            Ok(address) => address,
            Err(_) => {
                return Err(format!(
                    "Failed to convert to ethereum address: {}",
                    address.0
                ));
            }
        };

        let request = EstimateRequest {
            from: Address::from(key_pair.public_key),
            to: address,
            amount: GenericCoinAmount::from_atomic(Coin::ETH, total_amount),
        };

        let estimate = match self.client.get_estimated_fee(&request).await {
            Ok(result) => result,
            Err(err) => return Err(format!("Failed to get estimate: {}", err)),
        };

        let fee = U256::from(estimate.gas_limit).saturating_mul(estimate.gas_price.into());
        let fee = match u128::try_from(fee) {
            Ok(fee) if fee > 0 && fee < total_amount => fee,
            Ok(fee) => return Err(format!("Invalid fee: {}", fee)),
            _ => return Err("Eth Fee is higher than U128::MAX".to_owned()),
        };

        let new_amount = total_amount - fee;

        // Send!
        let transaction = SendTransaction {
            from: key_pair.clone(),
            to: address,
            amount: GenericCoinAmount::from_atomic(Coin::ETH, new_amount),
            gas_limit: estimate.gas_limit,
            gas_price: estimate.gas_price,
        };

        let tx_hash = match self.client.send(&transaction).await {
            Ok(hash) => hash,
            Err(err) => return Err(format!("Failed to send eth transaction: {}", err)),
        };

        let uuids = outputs.iter().map(|tx| tx.id).collect_vec();

        match OutputSentTx::new(
            Timestamp::now(),
            uuids,
            Coin::ETH,
            address.into(),
            new_amount,
            fee,
            tx_hash.to_string(),
        ) {
            Ok(tx) => Ok(tx),
            // Panic here because we sent money but didn't record it into the system
            Err(err) => panic!(
                "Failed to create output tx for {:?} with hash {}: {}",
                outputs, tx_hash, err
            ),
        }
    }

    async fn send_outputs(&self, outputs: &[OutputTx], key_pair: &KeyPair) -> Vec<OutputSentTx> {
        let mut sent_txs: Vec<OutputSentTx> = vec![];

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
impl<E: EthereumClient + Sync + Send, T: TransactionProvider + Sync + Send> OutputSender
    for EthOutputSender<E, T>
{
    async fn send(&self, outputs: &[OutputTx]) -> Vec<OutputSentTx> {
        if (outputs.is_empty()) {
            return vec![];
        }

        if let Some(tx) = outputs.iter().find(|quote| quote.coin != Coin::ETH) {
            error!("Invalid output {:?} sent to ETH output sender", tx);
            return vec![];
        }

        let keys = get_input_id_indicies(self.provider.clone(), Coin::ETH)
            .into_iter()
            .filter_map(|index| {
                match bip44::get_key_pair(
                    self.root_private_key.clone(),
                    bip44::CoinType::ETH,
                    index,
                ) {
                    Ok(keys) => Some(keys),
                    Err(err) => {
                        error!(
                            "Failed to generate eth key pair for index {}: {}",
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
            match self.client.get_balance(key.public_key.into()).await {
                Ok(balance) => wallet_balances.push(WalletBalance::new(key, balance)),
                Err(err) => {
                    warn!("Failed to fetch balance for {}: {}", key.public_key, err);
                }
            };
        }
        let wallet_outputs = get_sending_wallets(&wallet_balances, outputs);

        let mut sent_txs: Vec<OutputSentTx> = vec![];
        for output in wallet_outputs {
            let sent = self.send_outputs(&[output.output], &output.wallet).await;
            sent_txs.extend(sent);
        }
        sent_txs
    }
}

#[cfg(test)]
mod test {
    use std::sync::Mutex;

    use crate::{
        common::{ethereum, WalletAddress},
        side_chain::MemorySideChain,
        utils::test_utils::TEST_ROOT_KEY,
        utils::test_utils::{create_fake_output_tx, ethereum::TestEthereumClient},
        vault::blockchain_connection::ethereum::EstimateResult,
        vault::transactions::MemoryTransactionsProvider,
    };

    use super::*;

    fn get_key_pair() -> KeyPair {
        KeyPair::from_private_key(
            "58a99f6e6f89cbbb7fc8c86ea95e6012b68a9cd9a41c4ffa7c8f20c201d0667f",
        )
        .unwrap()
    }

    fn get_output_sender(
    ) -> EthOutputSender<TestEthereumClient, MemoryTransactionsProvider<MemorySideChain>> {
        let side_chain = MemorySideChain::new();
        let side_chain = Arc::new(Mutex::new(side_chain));
        let provider = MemoryTransactionsProvider::new_protected(side_chain.clone());
        let client = TestEthereumClient::new();
        let key = RawKey::decode(TEST_ROOT_KEY).unwrap();
        EthOutputSender::new(client, provider, key)
    }

    #[tokio::test]
    async fn send_outputs_inner_throws_errors() {
        let mut sender = get_output_sender();
        let key_pair = get_key_pair();
        assert_eq!(
            &sender.send_outputs_inner(&[], &key_pair).await.unwrap_err(),
            "Empty outputs"
        );

        // =================

        let mut output_1 = create_fake_output_tx(Coin::ETH);
        let mut output_2 = create_fake_output_tx(Coin::ETH);
        output_1.amount = u128::MAX;
        output_2.amount = 100;

        assert_eq!(
            &sender
                .send_outputs_inner(&[&output_1, &output_2], &key_pair)
                .await
                .unwrap_err(),
            "Total output amount is greater than u128::Max"
        );

        // =================

        let output_1 = create_fake_output_tx(Coin::ETH);
        let mut output_2 = create_fake_output_tx(Coin::ETH);
        output_2.address = WalletAddress::new("different");

        assert_eq!(
            &sender
                .send_outputs_inner(&[&output_1, &output_2], &key_pair)
                .await
                .unwrap_err(),
            "Received eth outputs with different addresses!"
        );

        // =================

        let mut output_1 = create_fake_output_tx(Coin::ETH);
        let mut output_2 = create_fake_output_tx(Coin::ETH);
        output_1.address = WalletAddress::new("invalid");
        output_2.address = output_1.address.clone();

        assert!(sender
            .send_outputs_inner(&[&output_1, &output_2], &key_pair)
            .await
            .unwrap_err()
            .contains("Failed to convert to ethereum address"));

        // =================

        let output_1 = create_fake_output_tx(Coin::ETH);
        sender
            .client
            .set_get_estimate_fee_handler(|_| Err("Error".into()));

        assert_eq!(
            &sender
                .send_outputs_inner(&[&output_1], &key_pair)
                .await
                .unwrap_err(),
            "Failed to get estimate: Error"
        );

        // =================
        // Fee higher than or equal to total amount

        let mut output_1 = create_fake_output_tx(Coin::ETH);
        output_1.amount = 100;
        sender.client.set_get_estimate_fee_handler(|_| {
            Ok(EstimateResult {
                gas_limit: 100,
                gas_price: 1,
            })
        });

        assert_eq!(
            &sender
                .send_outputs_inner(&[&output_1], &key_pair)
                .await
                .unwrap_err(),
            "Invalid fee: 100"
        );

        output_1.amount = 90;
        assert_eq!(
            &sender
                .send_outputs_inner(&[&output_1], &key_pair)
                .await
                .unwrap_err(),
            "Invalid fee: 100"
        );

        // =================

        let mut output_1 = create_fake_output_tx(Coin::ETH);
        output_1.amount = 100;
        sender.client.set_get_estimate_fee_handler(|_| {
            Ok(EstimateResult {
                gas_limit: 100,
                gas_price: 0,
            })
        });

        assert_eq!(
            &sender
                .send_outputs_inner(&[&output_1], &key_pair)
                .await
                .unwrap_err(),
            "Invalid fee: 0"
        );

        // =================

        let mut output_1 = create_fake_output_tx(Coin::ETH);
        output_1.amount = 100;
        sender.client.set_get_estimate_fee_handler(|_| {
            Ok(EstimateResult {
                gas_limit: u128::MAX,
                gas_price: u128::MAX,
            })
        });

        assert_eq!(
            &sender
                .send_outputs_inner(&[&output_1], &key_pair)
                .await
                .unwrap_err(),
            "Eth Fee is higher than U128::MAX"
        );

        // =================

        let mut output_1 = create_fake_output_tx(Coin::ETH);
        output_1.amount = 100;
        sender.client.set_get_estimate_fee_handler(|_| {
            Ok(EstimateResult {
                gas_limit: 1,
                gas_price: 1,
            })
        });
        sender.client.set_send_handler(|_| Err("Send Error".into()));

        assert_eq!(
            &sender
                .send_outputs_inner(&[&output_1], &key_pair)
                .await
                .unwrap_err(),
            "Failed to send eth transaction: Send Error"
        );
    }

    #[tokio::test]
    async fn send_outputs_inner_returns_output_sent_tx_on_success() {
        let mut sender = get_output_sender();
        let key_pair = get_key_pair();
        let key_pair_public_key = (&key_pair.public_key).to_string();

        let mut output_1 = create_fake_output_tx(Coin::ETH);
        let mut output_2 = create_fake_output_tx(Coin::ETH);
        output_1.amount = 100;
        output_2.amount = 200;

        let address = "0x70e7db0678460c5e53f1ffc9221d1c692111dcc5";
        output_1.address = WalletAddress::new(address);
        output_2.address = WalletAddress::new(address);

        sender.client.set_get_estimate_fee_handler(|_| {
            Ok(EstimateResult {
                gas_limit: 1,
                gas_price: 1,
            })
        });

        let hash = "0xe8be8a8cd13e077730b2bf58af9434a8f2b53878372e693dbf61c06a08dfc5af";

        sender.client.set_send_handler(move |tx| {
            assert_eq!(tx.from.public_key.to_string(), key_pair_public_key);
            assert_eq!(&tx.to.to_string().to_lowercase(), address);
            assert_eq!(tx.amount.to_atomic(), 299);
            assert_eq!(tx.gas_limit, 1);
            assert_eq!(tx.gas_price, 1);

            Ok(ethereum::Hash::from_str(hash).unwrap())
        });

        let sent_tx = sender
            .send_outputs_inner(&[&output_1, &output_2], &key_pair)
            .await
            .unwrap();

        assert_eq!(sent_tx.output_txs, vec![output_1.id, output_2.id]);
        assert_eq!(sent_tx.coin, Coin::ETH);
        assert_eq!(&sent_tx.address.0.to_lowercase(), address);
        assert_eq!(sent_tx.amount, 299);
        assert_eq!(sent_tx.fee, 1);
        assert_eq!(&sent_tx.transaction_id, hash);
    }

    #[tokio::test]
    async fn send_outputs_skips_outputs_with_errors() {
        let mut sender = get_output_sender();
        let key_pair = get_key_pair();

        let mut output_1 = create_fake_output_tx(Coin::ETH);
        let mut output_2 = create_fake_output_tx(Coin::ETH);

        // Make sure send_outputs splits these outputs in 2 distinct sends
        output_1.amount = u128::MAX;
        output_2.amount = 200;

        sender.client.set_get_estimate_fee_handler(|_| {
            Ok(EstimateResult {
                gas_limit: 1,
                gas_price: 1,
            })
        });

        sender.client.set_send_handler(|tx| {
            // Make output_1 fail
            if tx.amount.to_atomic() != 199 {
                Err("Oh no! send error".to_owned())
            } else {
                Ok(ethereum::Hash::from_str(
                    "0xe8be8a8cd13e077730b2bf58af9434a8f2b53878372e693dbf61c06a08dfc5af",
                )
                .unwrap())
            }
        });

        let sent = sender
            .send_outputs(&[output_1, output_2.clone()], &key_pair)
            .await;
        assert_eq!(sent.len(), 1);

        let first = sent.first().unwrap();
        assert_eq!(&first.output_txs, &[output_2.id]);
        assert_eq!(first.amount, 199);
        assert_eq!(first.fee, 1);
    }
}
