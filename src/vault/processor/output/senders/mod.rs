use crate::{
    transactions::{OutputSentTx, OutputTx},
    vault::transactions::TransactionProvider,
};

/// A trait for an output sender
pub trait OutputSender {
    /// Send the given outputs and return output sent txs
    fn send<T: TransactionProvider>(&self, provider: &T, outputs: &[OutputTx])
        -> Vec<OutputSentTx>;
}
