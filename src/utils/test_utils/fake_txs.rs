use crate::{
    common::Timestamp,
    common::{
        coins::{CoinAmount, GenericCoinAmount, PoolCoin},
        Coin, LokiAmount, LokiPaymentId, WalletAddress,
    },
    transactions::{QuoteTx, StakeQuoteTx, UnstakeRequestTx, WitnessTx},
};
use std::str::FromStr;
use uuid::Uuid;

/// Create a fake stake quote transaction for testing
pub fn create_fake_stake_quote(
    loki_amount: LokiAmount,
    coin_amount: GenericCoinAmount,
) -> StakeQuoteTx {
    StakeQuoteTx {
        id: Uuid::new_v4(),
        input_loki_address_id: LokiPaymentId::from_str("60900e5603bf96e3").unwrap(),
        loki_amount,
        coin_type: PoolCoin::from(coin_amount.coin_type()).expect("invalid coin type"),
        coin_amount,
        staker_id: Uuid::new_v4().to_string(),
    }
}

/// Create a fake witness transaction for testing
pub fn create_fake_witness<T>(quote: &StakeQuoteTx, amount: T, coin: Coin) -> WitnessTx
where
    T: Into<GenericCoinAmount>,
{
    WitnessTx {
        id: Uuid::new_v4(),
        timestamp: Timestamp::now(),
        quote_id: quote.id,
        transaction_id: "".to_owned(),
        transaction_block_number: 0,
        transaction_index: 0,
        amount: amount.into().to_atomic(),
        coin,
        sender: None,
    }
}

/// Create a fake unstake request for testing
pub fn create_fake_unstake_request_tx(staker_id: String) -> UnstakeRequestTx {
    UnstakeRequestTx {
        id: Uuid::new_v4(),
        staker_id,
    }
}
