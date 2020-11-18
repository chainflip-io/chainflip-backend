use crate::{
    common::*,
    utils::validation::{validate_address, validate_address_id},
};

use ring::signature::EcdsaKeyPair;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Signing of unstake transactions
pub mod signatures;

/// Quote transaction stored on the Side Chain
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
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

        if validate_address(input, &input_address.0).is_err() {
            return Err("Input address is invalid");
        }

        if let Some(address) = &return_address {
            if validate_address(input, &address.0).is_err() {
                return Err("Return address is invalid");
            }
        }

        if validate_address(output, &output_address.0).is_err() {
            return Err("Output address is invalid");
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
#[serde(rename_all = "camelCase")]
pub struct StakeQuoteTx {
    /// A unique identifier
    pub id: Uuid,
    /// When the quote was created
    pub timestamp: Timestamp,
    /// Stakers identity
    pub staker_id: StakerId,
    /// Other coin's type
    pub coin_type: PoolCoin,
    /// The coin input address
    pub coin_input_address: WalletAddress,
    /// The coin input address id
    pub coin_input_address_id: String,
    /// Address to return other coin to if Stake quote already fulfilled
    /// TODO: This should be a required field but since we have older stake quotes, we need to make this optional. Remove it in the future
    pub coin_return_address: Option<WalletAddress>,
    /// The loki input address
    pub loki_input_address: WalletAddress,
    /// Info used to uniquely identify payment
    pub loki_input_address_id: LokiPaymentId,
    /// Address to return Loki to if Stake quote already fulfilled
    /// TODO: This should be a required field but since we have older stake quotes, we need to make this optional. Remove it in the future
    pub loki_return_address: Option<WalletAddress>,
}

impl StakeQuoteTx {
    /// Create a new stake quote tx
    pub fn new(
        timestamp: Timestamp,
        coin_type: PoolCoin,
        coin_input_address: WalletAddress,
        coin_input_address_id: String,
        loki_input_address: WalletAddress,
        loki_input_address_id: LokiPaymentId,
        staker_id: StakerId,
        loki_return_address: WalletAddress,
        coin_return_address: WalletAddress,
    ) -> Result<Self, &'static str> {
        if validate_address_id(coin_type.get_coin(), &coin_input_address_id).is_err() {
            return Err("Coin input address id is invalid");
        }

        if validate_address(coin_type.get_coin(), &coin_input_address.0).is_err() {
            return Err("Coin input address is invalid");
        }

        if validate_address(Coin::LOKI, &loki_input_address.0).is_err() {
            return Err("Loki input address is invalid");
        }

        if validate_address(coin_type.get_coin(), &coin_return_address.0).is_err() {
            return Err("Coin return address is invalid");
        }

        if validate_address(Coin::LOKI, &loki_return_address.0).is_err() {
            return Err("Loki return address is invalid");
        }

        Ok(Self {
            id: Uuid::new_v4(),
            timestamp,
            coin_type,
            coin_input_address,
            coin_input_address_id,
            loki_input_address,
            loki_input_address_id,
            staker_id,
            loki_return_address: Some(loki_return_address),
            coin_return_address: Some(coin_return_address),
        })
    }
}

/// Witness transaction stored on the Side Chain
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WitnessTx {
    /// A unique identifier
    pub id: Uuid,
    /// Timestamp for when the transaction was created
    pub timestamp: Timestamp,
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
    pub coin: Coin,
}

impl PartialEq<WitnessTx> for WitnessTx {
    fn eq(&self, other: &WitnessTx) -> bool {
        self.quote_id == other.quote_id && self.transaction_id == other.transaction_id
    }
}

impl WitnessTx {
    /// Create a new witness transaction
    pub fn new(
        timestamp: Timestamp,
        quote_id: Uuid,
        transaction_id: String,
        transaction_block_number: u64,
        transaction_index: u64,
        amount: u128,
        coin: Coin,
    ) -> Self {
        WitnessTx {
            id: Uuid::new_v4(),
            timestamp,
            quote_id,
            transaction_id,
            transaction_block_number,
            transaction_index,
            amount,
            coin,
        }
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

/// A transaction acknowledging pool provisioning. Note that `loki_amount`
/// and `other_amount` don't necessarily match the amounts
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StakeTx {
    /// A unique identifier
    pub id: Uuid,
    /// Identifier of the corresponding pool change transaction
    pub pool_change_tx: Uuid,
    /// Identifier of the corresponding quote transaction
    pub quote_tx: Uuid,
    /// Identifier of the corresponding witness transactions
    pub witness_txs: Vec<Uuid>,
    /// Staker's identity
    pub staker_id: StakerId,
    /// Pool in which the stake is made
    pub pool: PoolCoin,
    /// Amount in the loki pool attributed to the staker in this tx
    pub loki_amount: LokiAmount,
    /// Atomic amount in the other coin (of type `pool`) attributed to the
    /// staker in this tx
    pub other_amount: u128,
}

/// Request to unstake funds
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnstakeRequestTx {
    /// Unique identifier
    pub id: Uuid,
    /// Staker's identity
    pub staker_id: StakerId,
    /// Which pool to unstake from
    pub pool: PoolCoin,
    /// Address to which withdraw loki
    pub loki_address: WalletAddress,
    /// Address to which withdraw the other coin
    pub other_address: WalletAddress,
    /// Time of creation
    pub timestamp: Timestamp,
    /// Fraction of the total portions to unstake (a number from 1 to 10000)
    pub fraction: UnstakeFraction,
    /// Signature ECDSA-P256-SHA256
    pub signature: String,
}

impl UnstakeRequestTx {
    /// Construct from staker_id
    pub fn new(
        pool: PoolCoin,
        staker_id: StakerId,
        loki_address: WalletAddress,
        other_address: WalletAddress,
        fraction: UnstakeFraction,
        timestamp: Timestamp,
        signature: String,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            staker_id,
            pool,
            loki_address,
            other_address,
            timestamp,
            fraction,
            signature,
        }
    }

    /// Create a base64 encoded signature using `keys`
    pub fn sign(&self, keys: &EcdsaKeyPair) -> Result<String, ()> {
        let signature = signatures::sign_unstake(&self, keys)?;
        Ok(base64::encode(&signature))
    }

    /// Check that the signature is valid
    pub fn verify(&self) -> Result<(), ()> {
        signatures::verify_unstake(&self)
    }
}

/// A transaction that acknowledges a processed unstake request
#[serde(rename_all = "camelCase")]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct UnstakeTx {
    /// A unique identifier
    pub id: Uuid,
    /// The time the transaction was made
    pub timestamp: Timestamp,
    /// Unstake request id
    pub request_id: Uuid,
    /// Coin output transactions (one for each coin type in the pool)
    pub output_txs: [Uuid; 2],
}

impl UnstakeTx {
    /// Create from unstake request id
    pub fn new(request_id: Uuid, output_txs: [Uuid; 2]) -> Self {
        UnstakeTx {
            id: Uuid::new_v4(),
            timestamp: Timestamp::now(),
            request_id,
            output_txs,
        }
    }
}

/// A transaction which indicates that we need to send to the main chain.
///
/// Note: The `amount` specified in this transaction does not include any fees.
/// Fees will need to be determined at a later stage and be taken away from the amount.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputTx {
    /// A unique identifier
    pub id: Uuid,
    /// The time when the transaction was made
    pub timestamp: Timestamp,
    /// The quote that was processed in this output
    // TODO: Rename this because it's not necessary that only quote txs are used here
    // When processing unstake txs this field is set to Unstake Request id
    pub quote_tx: Uuid,
    /// The witness transactions that were processed in this output
    pub witness_txs: Vec<Uuid>,
    /// The pool change transactions that were made for this output
    pub pool_change_txs: Vec<Uuid>,
    /// The output coin
    pub coin: Coin,
    /// The receiving address the output
    pub address: WalletAddress,
    /// The amount that we want to send
    pub amount: u128,
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
    ) -> Result<Self, &'static str> {
        if validate_address(coin, &address.0).is_err() {
            error!(
                "Invalid output address {} for quote {}",
                address.0, quote_tx
            );
            return Err("Invalid output address");
        }

        if amount == 0 {
            error!("Invalid output amount {} for quote {}", amount, quote_tx);
            return Err("Invalid output amount");
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
        })
    }
}

/// A transaction which indicates that we sent to the main chain.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputSentTx {
    /// A unique identifier
    pub id: Uuid,
    /// The time when the transaction was made
    pub timestamp: Timestamp,
    /// The output transactions that were sent
    pub output_txs: Vec<Uuid>,
    /// The output coin
    pub coin: Coin,
    /// The receiving address the output
    pub address: WalletAddress,
    /// The amount that we sent
    pub amount: u128,
    /// The fee that was taken
    pub fee: u128,
    /// The output transaction id or hash
    pub transaction_id: String,
}

impl OutputSentTx {
    /// Construct from fields
    pub fn new(
        timestamp: Timestamp,
        output_txs: Vec<Uuid>,
        coin: Coin,
        address: WalletAddress,
        amount: u128,
        fee: u128,
        transaction_id: String,
    ) -> Result<Self, &'static str> {
        if output_txs.is_empty() {
            return Err("Cannot create an OutputSentTx with empty output transactions");
        }
        if validate_address(coin, &address.0).is_err() {
            return Err("Invalid output address");
        }

        Ok(OutputSentTx {
            id: Uuid::new_v4(),
            timestamp,
            output_txs,
            coin,
            address,
            amount,
            fee,
            transaction_id,
        })
    }
}
