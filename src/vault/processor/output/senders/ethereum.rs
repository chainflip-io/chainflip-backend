use std::{convert::TryFrom, str::FromStr};

use async_trait::async_trait;
use bip44::KeyPair;
use hdwallet::ExtendedPrivKey;
use itertools::Itertools;
use uuid::Uuid;

use crate::{
    common::coins::GenericCoinAmount,
    common::ethereum::Address,
    common::Coin,
    common::Timestamp,
    transactions::OutputSentTx,
    transactions::OutputTx,
    utils::{
        bip44::{self, RawKey},
        primitives::U256,
    },
    vault::{
        blockchain_connection::ethereum::EstimateRequest,
        blockchain_connection::ethereum::EthereumClient,
        blockchain_connection::ethereum::SendTransaction, transactions::TransactionProvider,
    },
};

use super::OutputSender;

/// An output sender for Ethereum
pub struct EthOutputSender<E: EthereumClient> {
    client: E,
    root_private_key: ExtendedPrivKey,
}

impl<E: EthereumClient> EthOutputSender<E> {
    /// Create a new output sender
    pub fn new(client: E, root_key: RawKey) -> Self {
        let root_private_key = root_key
            .to_private_key()
            .expect("Failed to generate ethereum extended private key");

        Self {
            client,
            root_private_key,
        }
    }

    fn group_outputs_by_quote(&self, outputs: &[OutputTx]) -> Vec<(Uuid, Vec<OutputTx>)> {
        // Make sure we only have valid outputs and group them by the quote
        let groups = outputs
            .iter()
            .filter(|tx| tx.coin == Coin::ETH)
            .group_by(|tx| tx.quote_tx);

        groups
            .into_iter()
            .map(|(quote, group)| (quote, group.cloned().collect_vec()))
            .collect()
    }

    /// Groups outputs where the total amount is less than u128::MAX
    fn group_outputs_by_sending_amounts<'a>(
        &self,
        outputs: &'a [OutputTx],
    ) -> Vec<(u128, Vec<&'a OutputTx>)> {
        let mut groups: Vec<(u128, Vec<&OutputTx>)> = vec![];
        let mut current_amount: u128 = 0;
        let mut current_outputs: Vec<&OutputTx> = vec![];
        for output in outputs {
            match current_amount.checked_add(output.amount) {
                Some(amount) => {
                    current_amount = amount;
                    current_outputs.push(output);
                }
                None => {
                    let outputs = current_outputs;
                    groups.push((current_amount, outputs));
                    current_amount = 0;
                    current_outputs = vec![];
                }
            }
        }

        groups
    }

    async fn send_outputs(
        &self,
        outputs: &[OutputTx],
        key_pair: &KeyPair,
    ) -> Result<Vec<OutputSentTx>, String> {
        if outputs.is_empty() {
            return Err("Empty outputs".to_owned());
        }

        let address = &outputs.first().unwrap().address;
        if !outputs.iter().all(|tx| &tx.address == address) {
            return Err("Received eth outputs with different addresses!".to_owned());
        }

        let address = match Address::from_str(&address.0) {
            Ok(address) => address,
            Err(err) => {
                return Err(format!(
                    "Failed to convert to ethereum address: {}, {}",
                    address.0, err
                ));
            }
        };

        // Split outputs into chunks of u128
        let groups = self.group_outputs_by_sending_amounts(outputs);

        let mut sent_txs: Vec<OutputSentTx> = vec![];

        for (total_amount, outputs) in groups {
            let request = EstimateRequest {
                from: Address::from(key_pair.public_key),
                to: address,
                amount: GenericCoinAmount::from_atomic(Coin::ETH, total_amount),
            };
            let estimate = match self.client.get_estimated_fee(&request).await {
                Ok(result) => result,
                Err(err) => {
                    error!("Failed to get estimate: {}", err);
                    continue;
                }
            };

            let fee = U256::from(estimate.gas_limit).saturating_mul(estimate.gas_price.into());
            let fee = match u128::try_from(fee) {
                Ok(fee) if fee > 0 && fee < total_amount => fee,
                _ => {
                    error!("Eth Fee is higher than U128::MAX or total amount");
                    continue;
                }
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
                Err(err) => {
                    error!("Failed to send eth transaction: {}", err);
                    continue;
                }
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
                Ok(tx) => sent_txs.push(tx),
                // Panic here because we sent money but didn't record it into the system
                Err(err) => panic!(
                    "Failed to create output tx for {:?} with hash {}: {}",
                    outputs, tx_hash, err
                ),
            };
        }

        Ok(sent_txs)
    }
}

#[async_trait]
impl<E: EthereumClient + Sync + Send> OutputSender for EthOutputSender<E> {
    async fn send<T: TransactionProvider + Sync>(
        &self,
        _provider: &T,
        outputs: &[OutputTx],
    ) -> Vec<OutputSentTx> {
        // For now we'll simply send from the main wallet (index 0)
        // In the future it'll be better to send it from our other generated wallets if they have enough eth
        let key_pair =
            match bip44::get_key_pair(self.root_private_key.clone(), bip44::CoinType::ETH, 0) {
                Ok(keys) => keys,
                Err(err) => {
                    error!("Failed to generate eth key pair for index 0: {}", err);
                    return vec![];
                }
            };

        let grouped = self.group_outputs_by_quote(outputs);

        let mut sent_txs: Vec<OutputSentTx> = vec![];

        for (quote, txs) in grouped {
            match self.send_outputs(&txs, &key_pair).await {
                Ok(sent) => sent_txs.extend(sent),
                Err(err) => {
                    error!("{} - {} - {:?}", err, quote, txs);
                    continue;
                }
            }
        }

        sent_txs
    }
}
