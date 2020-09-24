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
    common::WalletAddress,
    transactions::OutputSentTx,
    transactions::OutputTx,
    utils::{
        bip44::{self, RawKey},
        primitives::U256,
    },
    vault::{
        blockchain_connection::ethereum::EstimateRequest,
        blockchain_connection::ethereum::EthereumClient, transactions::TransactionProvider,
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

    fn group_outputs<'a>(&self, outputs: &'a [OutputTx]) -> Vec<(Uuid, Vec<&'a OutputTx>)> {
        // Make sure we only have valid outputs and group them by the quote
        let groups = outputs
            .iter()
            .filter(|tx| tx.coin == Coin::ETH)
            .group_by(|tx| tx.quote_tx);

        groups
            .into_iter()
            .map(|(coin, group)| (coin, group.collect_vec()))
            .collect()
    }

    async fn send_outputs(
        &self,
        outputs: &[&OutputTx],
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

        // TODO: Split outputs into chunks of u128

        let request = EstimateRequest {
            from: Address::from_public_key(key_pair.public_key),
            to: address,
            amount: GenericCoinAmount::from_atomic(Coin::ETH, 100),
        };
        let estimate = match self.client.get_estimated_fee(&request).await {
            Ok(result) => result,
            Err(err) => {
                return Err(format!("Failed to get estimate: {}", err));
            }
        };

        let fee = U256::from(estimate.gas_limit).saturating_mul(estimate.gas_price.into());
        let fee = match u128::try_from(fee) {
            Ok(fee) => fee,
            Err(_) => {
                return Err("Eth Fee is higher than U128::MAX".to_owned());
            }
        };

        Err("not finished".to_owned())
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

        let grouped = self.group_outputs(outputs);

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
