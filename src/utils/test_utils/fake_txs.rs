use crate::{
    common::*,
    transactions::{signatures::get_random_staker, StakeQuoteTx, UnstakeRequestTx, WitnessTx},
};

use std::str::FromStr;
use uuid::Uuid;

use super::{TEST_BTC_ADDRESS, TEST_ETH_ADDRESS, TEST_LOKI_ADDRESS};

/// Create a fake stake quote transaction for testing
pub fn create_fake_stake_quote(coin: PoolCoin) -> StakeQuoteTx {
    // TODO: should probably use a ecdsa key here
    let staker_id = get_random_staker().id();
    create_fake_stake_quote_for_id(staker_id, coin)
}

/// Create a fake stake quote transaction for a specific staker (for testing only)
pub fn create_fake_stake_quote_for_id(staker_id: StakerId, coin: PoolCoin) -> StakeQuoteTx {
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
        loki_return_address: Some(WalletAddress::new(TEST_LOKI_ADDRESS)),
        staker_id,
        coin_input_address: WalletAddress::new(address),
        coin_input_address_id: "6".to_string(),
        coin_return_address: Some(WalletAddress::new(address)),
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

/// Create a correctly signed unstake tx with arbitrary addresses
pub fn create_unstake_for_staker(coin_type: PoolCoin, staker: &Staker) -> UnstakeRequestTx {
    let loki_address = WalletAddress::new("T6SMsepawgrKXeFmQroAbuTQMqLWyMxiVUgZ6APCRFgxQAUQ1AkEtHxAgDMZJJG9HMJeTeDsqWiuCMsNahScC7ZS2StC9kHhY");
    let other_address = WalletAddress::new("0x70e7db0678460c5e53f1ffc9221d1c692111dcc5");

    let timestamp = Timestamp::now();

    let staker_id = staker.id();

    let unsigned = UnstakeRequestTx::new(
        coin_type,
        staker_id,
        loki_address,
        other_address,
        UnstakeFraction::MAX,
        timestamp,
        "".to_owned(),
    );

    let signature = unsigned
        .sign(&staker.keys)
        .expect("could not sign unstake request");

    UnstakeRequestTx::new(
        coin_type,
        unsigned.staker_id,
        unsigned.loki_address,
        unsigned.other_address,
        UnstakeFraction::MAX,
        timestamp,
        signature,
    )
}
