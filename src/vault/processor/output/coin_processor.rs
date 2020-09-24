use crate::{
    common::Coin,
    transactions::OutputSentTx,
    transactions::OutputTx,
    utils::bip44,
    vault::{
        blockchain_connection::ethereum::EthereumClient, config::VAULT_CONFIG,
        transactions::TransactionProvider,
    },
};
use async_trait::async_trait;

use super::senders::{ethereum::EthOutputSender, OutputSender};

/// Handy trait for injecting custom processing code during testing
#[async_trait]
pub trait CoinProcessor {
    async fn process<T: TransactionProvider + Sync>(
        &self,
        provider: &T,
        coin: Coin,
        outputs: &[OutputTx],
    ) -> Vec<OutputSentTx>;
}

pub struct OutputCoinProcessor<E: EthereumClient> {
    eth: E,
}

impl<E: EthereumClient> OutputCoinProcessor<E> {
    /// Create a new output coin processor
    pub fn new(eth: E) -> Self {
        OutputCoinProcessor { eth }
    }
}

#[async_trait]
impl<E: EthereumClient + Clone + Sync + Send> CoinProcessor for OutputCoinProcessor<E> {
    async fn process<T: TransactionProvider + Sync>(
        &self,
        provider: &T,
        coin: Coin,
        outputs: &[OutputTx],
    ) -> Vec<OutputSentTx> {
        match coin {
            Coin::ETH => {
                let root_key = match bip44::RawKey::decode(&VAULT_CONFIG.eth.master_root_key) {
                    Ok(key) => key,
                    Err(_) => {
                        error!("Failed to generate root key from eth master root key");
                        return vec![];
                    }
                };

                let sender = EthOutputSender::new(self.eth.clone(), root_key);
                sender.send(provider, outputs).await
            }
            coin @ _ => {
                warn!(
                    "Cannot process outputs for {} because no associated sender is found!",
                    coin
                );
                vec![]
            }
        }
    }
}
