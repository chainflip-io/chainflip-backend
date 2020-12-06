use codec::{self as codec, Decode, Encode};
use sp_std::vec::Vec;

/*
    We need to use Vec<u8> to represent strings in substrate.
    The JS type definition for encoding/decoding for the testing frontend is `Text`

    ```rust
    pub struct TestData {
        pub name: Vec<u8>,
        pub address: Vec<u8>,
    }
    ```

    ```js
    TestData: {
        name: 'Text',
        address: 'Text'
    }
    ```
*/
pub type String = Vec<u8>;

// TODO: Convert these to atomic values in chainflip backend so we can use it natively within Encode and Decode withouut resorting to strings
pub type F64 = String;
pub type F32 = String;

// Types from chainflip backend
type Uuid = String;
type StakerId = String;
type Coin = String;
type PoolCoin = String;
type WalletAddress = String;
type Timestamp = u128;
type WithdrawFraction = u32;
type LokiPaymentId = String;
type LokiAmount = u128;

#[allow(non_camel_case_types)]
type ECDSA_P256_SHA256 = String;

#[derive(Clone, Debug, Decode, Encode, PartialEq, Eq)]
pub struct SwapQuote {
    pub id: Uuid,
    pub timestamp: Timestamp,
    pub input: Coin,
    pub input_address: WalletAddress,
    pub input_address_id: String,
    pub return_address: Option<WalletAddress>,
    pub output: Coin,
    pub output_address: WalletAddress,
    pub effective_price: F64,
    pub slippage_limit: F32,
}

#[derive(Clone, Debug, Decode, Encode, PartialEq, Eq)]
pub struct DepositQuote {
    pub id: Uuid,
    pub timestamp: Timestamp,
    pub staker_id: StakerId,
    pub coin_type: PoolCoin,
    pub coin_input_address: WalletAddress,
    pub coin_input_address_id: String,
    pub coin_return_address: Option<WalletAddress>,
    pub loki_input_address: WalletAddress,
    pub loki_input_address_id: LokiPaymentId,
    pub loki_return_address: Option<WalletAddress>,
}

#[derive(Clone, Debug, Decode, Encode, PartialEq, Eq)]
pub struct WithdrawRequest {
    pub id: Uuid,
    pub staker_id: StakerId,
    pub pool: PoolCoin,
    pub loki_address: WalletAddress,
    pub other_address: WalletAddress,
    pub timestamp: Timestamp,
    pub fraction: WithdrawFraction,
    pub signature: ECDSA_P256_SHA256,
}

#[derive(Clone, Debug, Decode, Encode, PartialEq, Eq)]
pub struct Witness {
    pub id: Uuid,
    pub timestamp: Timestamp,
    pub quote_id: Uuid,
    pub transaction_id: String,
    pub transaction_block_number: u64,
    pub transaction_index: u64,
    pub amount: u128,
    pub coin: Coin,
}

#[derive(Clone, Debug, Decode, Encode, PartialEq, Eq)]
pub struct PoolChange {
    pub id: Uuid,
    pub coin: Coin,
    pub depth_change: i128,
    pub loki_depth_change: i128,
}

#[derive(Clone, Debug, Decode, Encode, PartialEq, Eq)]
pub struct Deposit {
    pub id: Uuid,
    pub pool_change_tx: Uuid,
    pub quote_tx: Uuid,
    pub witness_txs: Vec<Uuid>,
    pub staker_id: StakerId,
    pub pool: PoolCoin,
    pub loki_amount: LokiAmount,
    pub other_amount: u128,
}

#[derive(Clone, Debug, Decode, Encode, PartialEq, Eq)]
pub struct Withdraw {
    pub id: Uuid,
    pub timestamp: Timestamp,
    pub request_id: Uuid,
    pub output_txs: [Uuid; 2],
}

#[derive(Clone, Debug, Decode, Encode, PartialEq, Eq)]
pub struct Output {
    pub id: Uuid,
    pub timestamp: Timestamp,
    pub quote_tx: Uuid,
    pub witness_txs: Vec<Uuid>,
    pub pool_change_txs: Vec<Uuid>,
    pub coin: Coin,
    pub address: WalletAddress,
    pub amount: u128,
}

#[derive(Clone, Debug, Decode, Encode, PartialEq, Eq)]
pub struct OutputSent {
    pub id: Uuid,
    pub timestamp: Timestamp,
    pub output_txs: Vec<Uuid>,
    pub coin: Coin,
    pub address: WalletAddress,
    pub amount: u128,
    pub fee: u128,
    pub transaction_id: String,
}
