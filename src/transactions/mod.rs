use crate::common::{coins::PoolCoin, Coin, Timestamp, WalletAddress};

use serde::{Deserialize, Serialize};

/// Quote identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub struct QuoteId(pub u64);

impl QuoteId {
    /// Create quote identifier from numeric representation
    pub fn new(id: u64) -> Self {
        QuoteId { 0: id }
    }
}

/// Quote transaction stored on the Side Chain
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
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
    /// Info used to derive unique deposit addresses
    pub input_address_id: String,
    /// The wallet used to refund coins in case of a failed swap
    pub return_address: WalletAddress,
    // There are more fields, but I will add them
    // when I have actually start using them
}

impl std::hash::Hash for QuoteTx {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.0.hash(state);
    }
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
    /// The coin type in which the transaction was made
    pub coin_type: Coin,
    /// The sender of the transaction
    pub sender: Option<String>,
}

impl PartialEq<WitnessTx> for WitnessTx {
    fn eq(&self, other: &WitnessTx) -> bool {
        self.quote_id == other.quote_id && self.transaction_id == other.transaction_id
    }
}

/// Pool change transaction stored on the Side Chain
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct PoolChangeTx {
    /// A unique transaction id
    pub id: u64, // TODO: use uuid
    /// The coin associated with this pool
    pub coin: PoolCoin,
    /// The depth change in atomic value of the `coin` in the pool
    pub depth_change: i128,
    /// The depth change in atomic value of the LOKI in the pool
    pub loki_depth_change: i128,
}
