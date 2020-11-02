use crate::{
    common::Coin,
    transactions::{OutputSentTx, OutputTx},
};

use super::senders::OutputSender;

/// Handy trait for injecting custom processing code during testing
#[async_trait]
pub trait CoinProcessor {
    /// Send outputs using corresponding "sender" for each coin
    async fn process(&self, coin: Coin, outputs: &[OutputTx]) -> Vec<OutputSentTx>;
}

/// Struct responsible for sending outputs all supported coin types
pub struct OutputCoinProcessor<L, E, B>
where
    L: OutputSender,
    E: OutputSender,
    B: OutputSender,
{
    loki: L,
    eth: E,
    btc: B,
}

impl<L: OutputSender, E: OutputSender, B: OutputSender> OutputCoinProcessor<L, E, B> {
    /// Create a new output coin processor
    pub fn new(loki: L, eth: E, btc: B) -> Self {
        OutputCoinProcessor { eth, btc, loki }
    }
}

#[async_trait]
impl<L, E, B> CoinProcessor for OutputCoinProcessor<L, E, B>
where
    L: OutputSender + Sync + Send,
    E: OutputSender + Sync + Send,
    B: OutputSender + Sync + Send,
{
    async fn process(&self, coin: Coin, outputs: &[OutputTx]) -> Vec<OutputSentTx> {
        match coin {
            Coin::ETH => self.eth.send(outputs).await,
            Coin::BTC => self.btc.send(outputs).await,
            Coin::LOKI => self.loki.send(outputs).await,
        }
    }
}
