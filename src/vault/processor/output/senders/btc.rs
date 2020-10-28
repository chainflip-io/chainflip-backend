use crate::{
    common::{Coin, GenericCoinAmount, Timestamp, WalletAddress},
    transactions::{OutputSentTx, OutputTx},
    utils::{
        bip44::{self, RawKey},
        primitives::U256,
    },
    vault::blockchain_connection::btc::{IBitcoinSend, SendTransaction},
};
use bip44::KeyPair;
use bitcoin::Address;
use hdwallet::ExtendedPrivKey;
use itertools::Itertools;
use std::{convert::TryFrom, str::FromStr};

use super::*;

/// An output send for Bitcoin
pub struct BtcOutputSender<B: IBitcoinSend> {
    client: B,
    root_private_key: ExtendedPrivKey,
}

impl<B: IBitcoinSend> BtcOutputSender<B> {
    pub fn new(client: B, root_key: RawKey) -> Self {
        let root_private_key = root_key
            .to_private_key()
            .expect("Failed to generate bitcoin extended private key");

        Self {
            client,
            root_private_key,
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
            return Err("Received BTC outputs with different addresses!".to_owned());
        }

        let address = match Address::from_str(&address.0) {
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
            to: address.clone(),
            amount: GenericCoinAmount::from_atomic(Coin::BTC, total_amount),
        };

        let tx_hash = match self.client.send(&transaction).await {
            Ok(hash) => hash,
            Err(err) => return Err(format!("Failed to send BTC transaction: {}", err)),
        };

        let uuids = outputs.iter().map(|tx| tx.id).collect_vec();

        let wallet_address = WalletAddress::new(&address.to_string());

        match OutputSentTx::new(
            Timestamp::now(),
            uuids,
            Coin::BTC,
            wallet_address,
            total_amount,
            // Fee is already taken from the send amount when sent
            0,
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
impl<B: IBitcoinSend + Sync + Send> OutputSender for BtcOutputSender<B> {
    async fn send(&self, outputs: &[OutputTx]) -> Vec<OutputSentTx> {
        let key_pair =
            match bip44::get_key_pair(self.root_private_key.clone(), bip44::CoinType::BTC, 0) {
                Ok(keys) => keys,
                Err(err) => {
                    error!("Failed to generate BTC key pair for index 0: {}", err);
                    return vec![];
                }
            };

        let mut sent_txs: Vec<OutputSentTx> = vec![];

        // Group outputs by their quote
        let grouped = group_outputs_by_quote(outputs, Coin::BTC);
        for (_, txs) in grouped {
            let sent = self.send_outputs(&txs, &key_pair).await;
            sent_txs.extend(sent);
        }

        sent_txs
    }
}

#[cfg(test)]
mod test {
    use crate::{
        common::WalletAddress,
        utils::test_utils::{btc::TestBitcoinSendClient, create_fake_output_tx, TEST_ROOT_KEY},
    };

    use super::*;

    fn get_key_pair() -> KeyPair {
        KeyPair::from_private_key(
            "58a99f6e6f89cbbb7fc8c86ea95e6012b68a9cd9a41c4ffa7c8f20c201d0667f",
        )
        .unwrap()
    }

    fn get_output_sender() -> BtcOutputSender<TestBitcoinSendClient> {
        let client = TestBitcoinSendClient::new();
        let key = RawKey::decode(TEST_ROOT_KEY).unwrap();
        BtcOutputSender::new(client, key)
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

        let mut output_1 = create_fake_output_tx(Coin::BTC);
        let mut output_2 = create_fake_output_tx(Coin::BTC);
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

        let output_1 = create_fake_output_tx(Coin::BTC);
        let mut output_2 = create_fake_output_tx(Coin::BTC);
        output_2.address = WalletAddress::new("different");

        assert_eq!(
            &sender
                .send_outputs_inner(&[&output_1, &output_2], &key_pair)
                .await
                .unwrap_err(),
            "Received BTC outputs with different addresses!"
        );

        // =================

        let mut output_1 = create_fake_output_tx(Coin::BTC);
        let mut output_2 = create_fake_output_tx(Coin::BTC);
        output_1.address = WalletAddress::new("invalid");
        output_2.address = output_1.address.clone();

        assert!(sender
            .send_outputs_inner(&[&output_1, &output_2], &key_pair)
            .await
            .unwrap_err()
            .contains("Failed to convert to bitcoin address"));

        // =================
        // Fee higher than or equal to the amount

        let mut output_1 = create_fake_output_tx(Coin::BTC);
        // amount less than fee
        output_1.amount = 10;

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

        let mut output_1 = create_fake_output_tx(Coin::BTC);
        let mut output_2 = create_fake_output_tx(Coin::BTC);
        output_1.amount = 100;
        output_2.amount = 200;

        // bitcoin address (old) are case sensitive
        let address = "1BvBMSEYstWetqTFn5Au4m4GFg7xJaNVN2";
        output_1.address = WalletAddress::new(address);
        output_2.address = WalletAddress::new(address);

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

        assert_eq!(sent_tx.output_txs, vec![output_1.id, output_2.id]);
        assert_eq!(sent_tx.coin, Coin::BTC);
        assert_eq!(&sent_tx.address.0, address);
        assert_eq!(sent_tx.amount, 300);
        // Fee is sent from the send amount
        assert_eq!(sent_tx.fee, 0);
        assert_eq!(&sent_tx.transaction_id, hash);
    }

    #[tokio::test]
    async fn send_outputs_skips_outputs_with_errors() {
        let mut sender = get_output_sender();
        let key_pair = get_key_pair();

        let mut output_1 = create_fake_output_tx(Coin::BTC);
        let mut output_2 = create_fake_output_tx(Coin::BTC);

        // Make sure send_outputs splits these outputs in 2 distinct sends
        output_1.amount = u128::MAX;
        output_2.amount = 200;

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
        assert_eq!(&first.output_txs, &[output_2.id]);
        assert_eq!(first.amount, 200);
        assert_eq!(first.fee, 0);
    }
}
