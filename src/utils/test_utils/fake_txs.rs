use crate::{
    common::{
        coins::{CoinAmount, GenericCoinAmount, PoolCoin},
        Coin, LokiAmount, LokiPaymentId,
    },
    transactions::{StakeQuoteTx, WitnessTx},
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
    }
}

/// Create a fake witness transaction for testing
pub fn create_fake_witness(
    quote: &StakeQuoteTx,
    amount: GenericCoinAmount,
    coin_type: Coin,
) -> WitnessTx {
    WitnessTx {
        id: Uuid::new_v4(),
        quote_id: quote.id,
        transaction_id: "".to_owned(),
        transaction_block_number: 0,
        transaction_index: 0,
        amount: amount.to_atomic(),
        coin_type,
        sender: None,
    }
}
