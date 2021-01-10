use data::TestData;

use super::*;
use crate::{common::GenericCoinAmount, utils::test_utils::*};

#[test]
fn fulfilled_eth_quotes_should_produce_new_tx() {
    let coin_type = Coin::ETH;
    let loki_amount = LokiAmount::from_decimal_string("1.0");
    let coin_amount = GenericCoinAmount::from_decimal_string(coin_type, "2.0");

    let quote_tx = TestData::deposit_quote(coin_type);
    let wtx_loki = TestData::witness(quote_tx.id, loki_amount.to_atomic(), Coin::LOKI);
    let wtx_loki = WitnessTxWrapper::new(wtx_loki, false);
    let wtx_eth = TestData::witness(quote_tx.id, coin_amount.to_atomic(), coin_type);
    let wtx_eth = WitnessTxWrapper::new(wtx_eth, false);

    let quote_tx = FulfilledTxWrapper::new(quote_tx, false);

    let res = process_stake_quote(&quote_tx, &[&wtx_loki, &wtx_eth], Network::Testnet).unwrap();

    assert_eq!(
        res.pool_change.depth_change as u128,
        coin_amount.to_atomic()
    );
    assert_eq!(
        res.pool_change.base_depth_change as u128,
        loki_amount.to_atomic()
    );

    assert_eq!(res.stake_tx.pool_change, res.pool_change.id);
    assert_eq!(res.stake_tx.quote, quote_tx.inner.id);
    assert!(res.stake_tx.witnesses.contains(&wtx_loki.inner.id));
    assert!(res.stake_tx.witnesses.contains(&wtx_eth.inner.id));
}

#[test]
fn fulfilled_btc_quotes_should_produce_new_tx() {
    let coin_type = Coin::BTC;
    let loki_amount = LokiAmount::from_decimal_string("1.0");
    let coin_amount = GenericCoinAmount::from_decimal_string(coin_type, "2.0");

    let quote_tx = TestData::deposit_quote(coin_type);
    let wtx_loki = TestData::witness(quote_tx.id, loki_amount.to_atomic(), Coin::LOKI);
    let wtx_loki = WitnessTxWrapper::new(wtx_loki, false);
    let wtx_btc = TestData::witness(quote_tx.id, coin_amount.to_atomic(), coin_type);
    let wtx_btc = WitnessTxWrapper::new(wtx_btc, false);

    let quote_tx = FulfilledTxWrapper::new(quote_tx, false);

    let res = process_stake_quote(&quote_tx, &[&wtx_loki, &wtx_btc], Network::Testnet).unwrap();

    assert_eq!(
        res.pool_change.depth_change as u128,
        coin_amount.to_atomic()
    );
    assert_eq!(
        res.pool_change.base_depth_change as u128,
        loki_amount.to_atomic()
    );

    assert_eq!(res.stake_tx.pool_change, res.pool_change.id);
    assert_eq!(res.stake_tx.quote, quote_tx.inner.id);
    assert!(res.stake_tx.witnesses.contains(&wtx_loki.inner.id));
    assert!(res.stake_tx.witnesses.contains(&wtx_btc.inner.id));
}

#[test]
fn partially_fulfilled_quotes_do_not_produce_new_tx() {
    let coin_type = Coin::ETH;
    let loki_amount = LokiAmount::from_decimal_string("1.0");
    let _coin_amount = GenericCoinAmount::from_decimal_string(coin_type, "2.0");

    let quote_tx = TestData::deposit_quote(coin_type);
    let wtx_loki = TestData::witness(quote_tx.id, loki_amount.to_atomic(), Coin::LOKI);
    let wtx_loki = WitnessTxWrapper::new(wtx_loki, false);

    let quote_tx = FulfilledTxWrapper::new(quote_tx, false);

    let tx = process_stake_quote(&quote_tx, &[&wtx_loki], Network::Testnet);

    assert!(tx.is_none())
}

#[test]
fn refunds_if_stake_quote_is_fulfilled() {
    let coin_type = Coin::BTC;
    let loki_amount = LokiAmount::from_decimal_string("1.0");
    let btc_amount = GenericCoinAmount::from_decimal_string(Coin::BTC, "2.0");

    let quote_tx = TestData::deposit_quote(coin_type);
    let wtx_loki = TestData::witness(quote_tx.id, loki_amount.to_atomic(), Coin::LOKI);
    let wtx_loki = WitnessTxWrapper::new(wtx_loki, false);
    let wtx_btc = TestData::witness(quote_tx.id, btc_amount.to_atomic(), coin_type);
    let wtx_btc = WitnessTxWrapper::new(wtx_btc, false);

    let quote_tx = FulfilledTxWrapper::new(quote_tx, true);

    // Processing fulfilled stake quote should return nothing
    assert!(process_stake_quote(&quote_tx, &[&wtx_loki, &wtx_btc], Network::Testnet).is_none());

    let outputs = refund_stake_quote_txs(&quote_tx, &[&wtx_loki, &wtx_btc], Network::Testnet);
    assert_eq!(outputs.len(), 2);

    let loki_output = outputs.iter().find(|tx| tx.coin == Coin::LOKI).unwrap();
    assert_eq!(loki_output.address, quote_tx.inner.base_return_address);
    assert_eq!(loki_output.amount, loki_amount.to_atomic());

    let btc_output = outputs.iter().find(|tx| tx.coin == Coin::BTC).unwrap();
    assert_eq!(btc_output.address, quote_tx.inner.coin_return_address);
    assert_eq!(btc_output.amount, btc_amount.to_atomic());
}

#[test]
fn witness_tx_cannot_be_reused() {
    let coin_type = Coin::ETH;
    let loki_amount = LokiAmount::from_decimal_string("1.0");
    let coin_amount = GenericCoinAmount::from_decimal_string(coin_type, "2.0");

    let quote_tx = TestData::deposit_quote(coin_type);

    let wtx_loki = TestData::witness(quote_tx.id, loki_amount.to_atomic(), Coin::LOKI);
    // Witness has already been used before
    let wtx_loki = WitnessTxWrapper::new(wtx_loki, true);

    let wtx_eth = TestData::witness(quote_tx.id, coin_amount.to_atomic(), coin_type);
    let wtx_eth = WitnessTxWrapper::new(wtx_eth, false);

    let quote_tx = FulfilledTxWrapper::new(quote_tx, false);

    let tx = process_stake_quote(&quote_tx, &[&wtx_loki, &wtx_eth], Network::Testnet);

    assert!(tx.is_none())
}

#[test]
fn quote_cannot_be_fulfilled_twice() {
    let coin_type = Coin::ETH;
    let loki_amount = LokiAmount::from_decimal_string("1.0");
    let coin_amount = GenericCoinAmount::from_decimal_string(coin_type, "2.0");

    let quote_tx = TestData::deposit_quote(coin_type);

    let wtx_loki = TestData::witness(quote_tx.id, loki_amount.to_atomic(), Coin::LOKI);
    let wtx_loki = WitnessTxWrapper::new(wtx_loki, false);

    let wtx_eth = TestData::witness(quote_tx.id, coin_amount.to_atomic(), coin_type);
    let wtx_eth = WitnessTxWrapper::new(wtx_eth, false);

    // The quote has already been fulfilled
    let quote_tx = FulfilledTxWrapper::new(quote_tx, true);

    let tx = process_stake_quote(&quote_tx, &[&wtx_loki, &wtx_eth], Network::Testnet);

    assert!(tx.is_none())
}

#[test]
fn check_staking_smaller_amounts() {
    let loki_amount = LokiAmount::from_decimal_string("1.0");

    let coin_type = Coin::ETH;
    let coin_amount = GenericCoinAmount::from_decimal_string(coin_type, "2.0");

    let quote_tx = TestData::deposit_quote(coin_type);
    let wtx_loki = TestData::witness(quote_tx.id, loki_amount.to_atomic(), Coin::LOKI);
    let wtx_loki = WitnessTxWrapper::new(wtx_loki, false);
    let wtx_eth = TestData::witness(quote_tx.id, coin_amount.to_atomic(), coin_type);
    let wtx_eth = WitnessTxWrapper::new(wtx_eth, false);

    let quote_tx = FulfilledTxWrapper::new(quote_tx, false);

    let res = process_stake_quote(&quote_tx, &[&wtx_loki, &wtx_eth], Network::Testnet).unwrap();

    assert_eq!(
        res.pool_change.depth_change as u128,
        coin_amount.to_atomic()
    );
    assert_eq!(
        res.pool_change.base_depth_change as u128,
        loki_amount.to_atomic()
    );

    assert_eq!(res.stake_tx.pool_change, res.pool_change.id);
    assert_eq!(res.stake_tx.quote, quote_tx.inner.id);
    assert!(res.stake_tx.witnesses.contains(&wtx_loki.inner.id));
    assert!(res.stake_tx.witnesses.contains(&wtx_eth.inner.id));
}

#[test]
fn check_portions_of_amount() {
    assert_eq!(
        get_portion_of_amount(1_000_000_000, Portion::MAX),
        1_000_000_000
    );

    let third = Portion(Portion::MAX.0 / 3);

    assert_eq!(
        get_portion_of_amount(1_000_000_000, third),
        1_000_000_000 / 3
    );

    let half = Portion(Portion::MAX.0 / 2);

    assert_eq!(get_portion_of_amount(u128::MAX, half), u128::MAX / 2);

    assert_eq!(get_portion_of_amount(0, half), 0);
}
