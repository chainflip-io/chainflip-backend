use crate::common::{coins::Coin, Timestamp, WalletAddress};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
pub struct QuoteId(pub u64);

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
    /// The input coin for the quote
    pub input: Coin,
    /// The output coin for the quote
    pub output: Coin,
    /// The wallet in which the user will deposit coins
    pub input_address: WalletAddress,
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
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WitnessTx {
    /// The quote that this witness tx is linked to
    pub quote_id: QuoteId,
    /// The input transaction id or hash
    pub transaction_id: String,
    /// The input transaction block number
    pub transaction_block_number: u64,
    /// The input transaction index in the block
    pub transaction_index: u64,
    /// The atomic input amount
    pub amount: u128,
    /// The sender of the transaction
    pub sender: Option<String>,
}

impl PartialEq<WitnessTx> for WitnessTx {
    fn eq(&self, other: &WitnessTx) -> bool {
        self.quote_id == other.quote_id && self.transaction_id == other.transaction_id
    }
}
