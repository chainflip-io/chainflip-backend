use crate::common::{Timestamp, WalletAddress};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
pub struct QuoteId(u64);

impl QuoteId {
    pub fn new(id: u64) -> Self {
        QuoteId { 0: id }
    }
}

/// Quote transaction stored on the Side Chain
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct QuoteTx {
    /// Quote id generated on the server (same as tx id?)
    pub id: QuoteId,
    /// Timestamp for when the transaction was added onto the side chain
    pub timestamp: Timestamp,
    /// The wallet in which the user will deposit coins
    pub deposit_address: WalletAddress,
    /// The wallet used to refund coins in case of a failed swap
    pub return_address: WalletAddress,
    // There are more fields, but I will add them
    // when I have actually start using them
}

#[derive(Debug)]
pub struct CoinTx {
    pub id: u64,
    pub timestamp: Timestamp,
    pub deposit_address: WalletAddress,
    pub return_address: WalletAddress,
}

/// Witness transaction stored on the Side Chain
#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
pub struct WitnessTx {
    pub quote_id: QuoteId,
}

impl WitnessTx {
    pub fn new(id: QuoteId) -> Self {
        WitnessTx { quote_id: id }
    }
}
