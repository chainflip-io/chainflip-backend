use crate::{
    common::Coin,
    transactions::{OutputSentTx, OutputTx},
    utils::bip44,
    vault::{
        blockchain_connection::{btc::IBitcoinSend, ethereum::EthereumClient},
        config::VAULT_CONFIG,
    },
};

use super::senders::{btc::BtcOutputSender, ethereum::EthOutputSender, OutputSender};

/// Handy trait for injecting custom processing code during testing
#[async_trait]
pub trait CoinProcessor {
    /// Send outputs using corresponding "sender" for each coin
    async fn process(&self, coin: Coin, outputs: &[OutputTx]) -> Vec<OutputSentTx>;
}

/// Struct responsible for sending outputs all supported coin types
pub struct OutputCoinProcessor<L: OutputSender, E: EthereumClient, B: IBitcoinSend> {
    loki: L,
    eth: E,
    btc: B,
}

impl<L: OutputSender, E: EthereumClient, B: IBitcoinSend> OutputCoinProcessor<L, E, B> {
    /// Create a new output coin processor
    pub fn new(loki: L, eth: E, btc: B) -> Self {
        OutputCoinProcessor { eth, btc, loki }
    }
}

#[async_trait]
impl<L, E, B> CoinProcessor for OutputCoinProcessor<L, E, B>
where
    L: OutputSender + Sync + Send,
    E: EthereumClient + Clone + Sync + Send,
    B: IBitcoinSend + Clone + Sync + Send,
{
    async fn process(&self, coin: Coin, outputs: &[OutputTx]) -> Vec<OutputSentTx> {
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
                sender.send(outputs).await
            }
            Coin::BTC => {
                let root_key = match bip44::RawKey::decode(&VAULT_CONFIG.btc.master_root_key) {
                    Ok(key) => key,
                    Err(_) => {
                        error!("Failed to generate root key from btc master root key");
                        return vec![];
                    }
                };
                let sender = BtcOutputSender::new(self.btc.clone(), root_key);
                sender.send(outputs).await
            }
            Coin::LOKI => self.loki.send(outputs).await,
        }
    }
}
