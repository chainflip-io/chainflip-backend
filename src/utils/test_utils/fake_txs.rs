use crate::{
    common::Timestamp,
    common::{Coin, GenericCoinAmount, LokiPaymentId, PoolCoin, WalletAddress},
    transactions::{StakeQuoteTx, WitnessTx},
};
use std::str::FromStr;
use uuid::Uuid;

use super::{TEST_BTC_ADDRESS, TEST_ETH_ADDRESS, TEST_LOKI_ADDRESS};

/// Create a fake stake quote transaction for testing
pub fn create_fake_stake_quote(coin: PoolCoin) -> StakeQuoteTx {
    let staker_id = Uuid::new_v4().to_string();
    create_fake_stake_quote_for_id(&staker_id, coin)
}

/// Create a fake stake quote transaction for a specific staker (for testing only)
pub fn create_fake_stake_quote_for_id(staker_id: &str, coin: PoolCoin) -> StakeQuoteTx {
    let address = match coin.get_coin() {
        Coin::BTC => TEST_BTC_ADDRESS,
        Coin::ETH => TEST_ETH_ADDRESS,
        _ => panic!("Failed to create fake stake quote"),
    };

    StakeQuoteTx {
        id: Uuid::new_v4(),
        timestamp: Timestamp::now(),
        coin_type: coin,
        loki_input_address: WalletAddress::new(TEST_LOKI_ADDRESS),
        loki_input_address_id: LokiPaymentId::from_str("60900e5603bf96e3").unwrap(),
        staker_id: staker_id.to_string(),
        coin_input_address: WalletAddress::new(address),
        coin_input_address_id: "6".to_string(),
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
    }
}
