use crate::{
    common::{
        coins::{GenericCoinAmount, PoolCoin},
        Coin, LokiAmount, LokiPaymentId, Timestamp, WalletAddress,
    },
    utils::validation::{validate_address, validate_address_id},
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
    /// The wallet in which the user will deposit coins
    pub input_address: WalletAddress,
    /// Info used to derive unique deposit addresses
    pub input_address_id: String,
    /// The wallet used to refund coins in case of a failed swap
    ///
    /// Invariant: must be set if `slippage_limit` > 0
    pub return_address: Option<WalletAddress>,
    /// The output coin for the quote
    pub output: Coin,
    /// The output address for the quote
    pub output_address: WalletAddress,
    /// The ratio between the input amount and output amounts at the time of quote creation
    pub effective_price: f64,
    /// The maximim price slippage limit
    ///
    /// Invariant: `0 <= slippage_limit < 1`
    ///
    /// Invariant: `return_address` must be set if slippage_limit > 0
    pub slippage_limit: f32,
}

impl Eq for QuoteTx {}

impl std::hash::Hash for QuoteTx {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl QuoteTx {
    /// Create a new quote transaction
    pub fn new(
        timestamp: Timestamp,
        input: Coin,
        input_address: WalletAddress,
        input_address_id: String,
        return_address: Option<WalletAddress>,
        output: Coin,
        output_address: WalletAddress,
        effective_price: f64,
        slippage_limit: f32,
    ) -> Result<Self, &'static str> {
        if slippage_limit < 0.0 || slippage_limit >= 1.0 {
            return Err("Slippage limit must be between 0 and 1");
        }

        if (slippage_limit > 0.0 || input.get_info().requires_return_address)
            && return_address.is_none()
        {
            return Err("Return address must be specified");
        }

        if validate_address_id(input, &input_address_id).is_err() {
            return Err("Input address id is invalid");
        }

        Ok(QuoteTx {
            id: Uuid::new_v4(),
            timestamp,
            input,
            input_address,
            input_address_id,
            return_address,
            output,
            output_address,
            effective_price,
            slippage_limit,
        })
    }
}

/// Staking (provisioning) quote transaction
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct StakeQuoteTx {
    /// A unique identifier
    pub id: Uuid,
    /// Info used to uniquely identify payment
    pub input_loki_address_id: LokiPaymentId,
    /// Loki amount that is meant to be deposited
    pub loki_amount: LokiAmount,
    /// Other coin's type
    pub coin_type: PoolCoin,
    /// Amount of the other (non-Loki) pool coin
    pub coin_amount: GenericCoinAmount,
    /// Stakers identity
    pub staker_id: String,
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
    pub sender: Option<WalletAddress>,
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

impl PoolChangeTx {
    /// Construct from fields
    pub fn new(coin: PoolCoin, loki_depth_change: i128, depth_change: i128) -> Self {
        PoolChangeTx {
            id: Uuid::new_v4(),
            coin,
            depth_change,
            loki_depth_change,
        }
    }
}

/// A transaction acknowledging pool provisioning
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct StakeTx {
    /// A unique identifier
    pub id: Uuid,
    /// Identifier of the corresponding pool change transaction
    pub pool_change_tx: Uuid,
    /// Identifier of the corresponding quote transaction
    pub quote_tx: Uuid,
    /// Identifier of the corresponding witness transactions
    pub witness_txs: Vec<Uuid>,
    /// For now this is just a simple way to identify "stakers".
    /// We are likely to replace with something more private
    pub staker_id: String,
    /// Pool in which the stake is made
    pub pool: PoolCoin,
}

/// Request to unstake funds
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct UnstakeRequestTx {
    /// Unique identifier
    pub id: Uuid,
    /// Stakers identity (TODO: needs to be more private and with authentication)
    pub staker_id: String,
}

/// A transaction for keeping track of any outgoing mainchain transaction
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct OutputTx {
    /// A unique identifier
    pub id: Uuid,
    /// The time when the transaction was made
    pub timestamp: Timestamp,
    /// The quote that was processed in this output
    pub quote_tx: Uuid,
    /// The witness transactions that were processed in this output
    pub witness_txs: Vec<Uuid>,
    /// The pool change transactions that were made for this output
    pub pool_change_txs: Vec<Uuid>,
    /// The output coin
    pub coin: Coin,
    /// The address the output was sent to
    pub address: WalletAddress,
    /// The amount that was sent
    pub amount: u128,
    /// The fee incurred during sending
    pub fee: u128,
    /// The main chain transaction id
    pub main_chain_tx_ids: Vec<String>,
}

impl OutputTx {
    /// Construct from fields
    pub fn new(
        timestamp: Timestamp,
        quote_tx: Uuid,
        witness_txs: Vec<Uuid>,
        pool_change_txs: Vec<Uuid>,
        coin: Coin,
        address: WalletAddress,
        amount: u128,
        fee: u128,
        main_chain_tx_ids: Vec<String>,
    ) -> Result<Self, &'static str> {
        if witness_txs.is_empty() {
            return Err("Cannot create an OutputTx with empty witness transactions");
        }

        if main_chain_tx_ids.is_empty() {
            return Err("Cannot create an OutputTx with empty main chain transaction ids");
        }

        if validate_address(coin, &address.0).is_err() {
            return Err("Invalid address passed in");
        }

        Ok(OutputTx {
            id: Uuid::new_v4(),
            timestamp,
            quote_tx,
            witness_txs,
            pool_change_txs,
            coin,
            address,
            amount,
            fee,
            main_chain_tx_ids,
        })
    }
}
