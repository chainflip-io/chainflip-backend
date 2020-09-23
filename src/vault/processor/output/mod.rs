use crate::vault::transactions::TransactionProvider;

mod senders;

/// Process all pending outputs
pub fn process_outputs<T: TransactionProvider>(provider: &mut T) {
    provider.sync();
}
