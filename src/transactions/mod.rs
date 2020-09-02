use crate::{
    common::{coins::PoolCoin, Coin, LokiAmount, LokiPaymentId, Timestamp, WalletAddress},
    side_chain::SideChainTx,
};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Quote transaction stored on the Side Chain
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct QuoteTx {
    /// A unique identifier
    pub id: Uuid,
    /// Timestamp for when the transaction was added onto the side chain
    pub timestamp: Timestamp,
    /// The input coin for the quote
    pub input: Coin,
    /// The amount to input
    pub input_amount: u128,
    /// The wallet in which the user will deposit coins
    pub input_address: WalletAddress,
    /// Info used to derive unique deposit addresses
    pub input_address_id: String,
    /// The wallet used to refund coins in case of a failed swap
    pub return_address: Option<WalletAddress>,
    /// The output coin for the quote
    pub output: Coin,
    /// The slippage limit
    pub slippage_limit: f32,
}

impl Eq for QuoteTx {}
/// Staking (provisioning) quote transaction
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct StakeQuoteTx {
    /// A unique identifier
    pub id: Uuid,
    /// Info used to uniquely identify payment
    pub input_loki_address_id: LokiPaymentId,
    /// Loki amount that is meant to be deposited
    pub loki_amount: LokiAmount,
}

impl std::hash::Hash for QuoteTx {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

// This might be obsolete...
#[derive(Debug)]
pub struct CoinTx {
    pub id: Uuid,
    pub timestamp: Timestamp,
    pub deposit_address: WalletAddress,
    pub return_address: Option<WalletAddress>,
}

/// Witness transaction stored on the Side Chain
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WitnessTx {
    /// A unique identifier
    pub id: Uuid,
    /// The quote that this witness tx is linked to
    pub quote_id: Uuid,
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
    /// A unique identifier
    pub id: Uuid,
    /// The coin associated with this pool
    pub coin: PoolCoin,
    /// The depth change in atomic value of the `coin` in the pool
    pub depth_change: i128,
    /// The depth change in atomic value of the LOKI in the pool
    pub loki_depth_change: i128,
}

impl From<PoolChangeTx> for SideChainTx {
    fn from(tx: PoolChangeTx) -> Self {
        SideChainTx::PoolChangeTx(tx)
    }
}
