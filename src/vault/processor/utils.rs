use crate::{common::Timestamp, transactions::QuoteTx};

/// The quote expiry time
const SWAP_QUOTE_EXPIRE_AFTER_MS: u128 = 12 * 60 * 60 * 1000; // 12 hours

/// Return is a swap quote has expired
pub fn is_swap_quote_expired(quote: &QuoteTx) -> bool {
    Timestamp::now().0 - quote.timestamp.0 >= SWAP_QUOTE_EXPIRE_AFTER_MS
}

/// Get the expire timestamp for a swap quote
pub fn get_swap_expire_timestamp(created_at: &Timestamp) -> Timestamp {
    let expires_at = created_at.0.saturating_add(SWAP_QUOTE_EXPIRE_AFTER_MS);
    Timestamp(expires_at)
}
