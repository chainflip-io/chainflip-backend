use crate::constants::SWAP_QUOTE_EXPIRE;
use chainflip_common::types::{chain::SwapQuote, Timestamp};

/// Return is a swap quote has expired
pub fn is_swap_quote_expired(quote: &SwapQuote) -> bool {
    Timestamp::now().0 - quote.timestamp.0 >= SWAP_QUOTE_EXPIRE
}

/// Get the expire timestamp for a swap quote
pub fn get_swap_expire_timestamp(created_at: &Timestamp) -> Timestamp {
    let expires_at = created_at.0.saturating_add(SWAP_QUOTE_EXPIRE);
    Timestamp(expires_at)
}
