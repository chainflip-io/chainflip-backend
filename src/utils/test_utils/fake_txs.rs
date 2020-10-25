use crate::{
    common::Timestamp,
    common::{Coin, GenericCoinAmount, LokiAmount, LokiPaymentId, PoolCoin, WalletAddress},
    transactions::{StakeQuoteTx, WitnessTx},
};
use std::str::FromStr;
use uuid::Uuid;

/// Create a fake stake quote transaction for testing
pub fn create_fake_stake_quote(
    loki_amount: LokiAmount,
    coin_amount: GenericCoinAmount,
) -> StakeQuoteTx {
    let staker_id = Uuid::new_v4().to_string();
    create_fake_stake_quote_for_id(&staker_id, loki_amount, coin_amount)
}

/// Create a fake stake quote transaction for a specific staker (for testing only)
pub fn create_fake_stake_quote_for_id(
    staker_id: &str,
    loki_amount: LokiAmount,
    coin_amount: GenericCoinAmount,
) -> StakeQuoteTx {
    StakeQuoteTx {
        id: Uuid::new_v4(),
        input_loki_address_id: LokiPaymentId::from_str("60900e5603bf96e3").unwrap(),
        loki_atomic_amount: loki_amount.to_atomic(),
        coin_type: PoolCoin::from(coin_amount.coin_type()).expect("invalid coin type"),
        coin_atomic_amount: coin_amount.to_atomic(),
        staker_id: staker_id.to_string(),
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
