use crate::{
    common::Coin, transactions::OutputSentTx, transactions::OutputTx,
    vault::blockchain_connection::ethereum::EthereumClient,
};

/// Handy trait for injecting custom processing code during testing
pub trait CoinProcessor {
    fn process(&self, coin: Coin, outputs: &[OutputTx]) -> Vec<OutputSentTx>;
}

pub struct OutputCoinProcessor<E: EthereumClient> {
    eth: E,
}

impl<E> OutputCoinProcessor<E>
where
    E: EthereumClient,
{
    /// Create a new output coin processor
    pub fn new(eth: E) -> Self {
        OutputCoinProcessor { eth }
    }
}

impl<E> CoinProcessor for OutputCoinProcessor<E>
where
    E: EthereumClient,
{
    fn process(&self, coin: Coin, outputs: &[OutputTx]) -> Vec<OutputSentTx> {
        match coin {
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
