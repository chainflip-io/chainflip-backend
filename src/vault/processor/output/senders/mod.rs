use crate::{
    transactions::{OutputSentTx, OutputTx},
    vault::transactions::TransactionProvider,
};
use async_trait::async_trait;

/// A trait for an output sender
#[async_trait]
pub trait OutputSender {
    /// Send the given outputs and return output sent txs
    async fn send<T: TransactionProvider + Sync>(
        &self,
        provider: &T,
        outputs: &[OutputTx],
    ) -> Vec<OutputSentTx>;
}

pub mod ethereum;
