use data::TestData;

use super::*;
use crate::{
    common::GenericCoinAmount, utils::test_utils::*,
    vault::transactions::memory_provider::WitnessStatus,
};

#[test]
fn fulfilled_eth_quotes_should_produce_new_tx() {
    let coin_type = Coin::ETH;
    let oxen_amount = OxenAmount::from_decimal_string("1.0");
    let coin_amount = GenericCoinAmount::from_decimal_string(coin_type, "2.0");

    let quote_tx = TestData::deposit_quote(coin_type);

    let wtx_oxen = TestData::witness(quote_tx.unique_id(), oxen_amount.to_atomic(), Coin::OXEN);
    let wtx_oxen = StatusWitnessWrapper::new(wtx_oxen, WitnessStatus::Confirmed);

    let wtx_eth = TestData::witness(quote_tx.unique_id(), coin_amount.to_atomic(), coin_type);
    let wtx_eth = StatusWitnessWrapper::new(wtx_eth, WitnessStatus::Confirmed);

    let quote_tx = FulfilledWrapper::new(quote_tx, false);

    let res = process_deposit_quote(&quote_tx, &[&wtx_oxen, &wtx_eth], Network::Testnet).unwrap();

    assert_eq!(
        res.pool_change.depth_change as u128,
        coin_amount.to_atomic()
    );
    assert_eq!(
        res.pool_change.base_depth_change as u128,
        oxen_amount.to_atomic()
    );

    assert_eq!(res.deposit.pool_change, res.pool_change.unique_id());
    assert_eq!(res.deposit.quote, quote_tx.inner.unique_id());
    assert!(res.deposit.witnesses.contains(&wtx_oxen.inner.unique_id()));
    assert!(res.deposit.witnesses.contains(&wtx_eth.inner.unique_id()));
}

#[test]
fn fulfilled_btc_quotes_should_produce_new_tx() {
    let coin_type = Coin::BTC;
    let oxen_amount = OxenAmount::from_decimal_string("1.0");
    let coin_amount = GenericCoinAmount::from_decimal_string(coin_type, "2.0");

    let quote_tx = TestData::deposit_quote(coin_type);

    let wtx_oxen = TestData::witness(quote_tx.unique_id(), oxen_amount.to_atomic(), Coin::OXEN);
    let wtx_oxen = StatusWitnessWrapper::new(wtx_oxen, WitnessStatus::Confirmed);

    let wtx_btc = TestData::witness(quote_tx.unique_id(), coin_amount.to_atomic(), coin_type);
    let wtx_btc = StatusWitnessWrapper::new(wtx_btc, WitnessStatus::Confirmed);

    let quote_tx = FulfilledWrapper::new(quote_tx, false);

    let res = process_deposit_quote(&quote_tx, &[&wtx_oxen, &wtx_btc], Network::Testnet).unwrap();

    assert_eq!(
        res.pool_change.depth_change as u128,
        coin_amount.to_atomic()
    );
    assert_eq!(
        res.pool_change.base_depth_change as u128,
        oxen_amount.to_atomic()
    );

    assert_eq!(res.deposit.pool_change, res.pool_change.unique_id());
    assert_eq!(res.deposit.quote, quote_tx.inner.unique_id());
    assert!(res.deposit.witnesses.contains(&wtx_oxen.inner.unique_id()));
    assert!(res.deposit.witnesses.contains(&wtx_btc.inner.unique_id()));
}

#[test]
fn partially_fulfilled_quotes_do_not_produce_new_tx() {
    let coin_type = Coin::ETH;
    let oxen_amount = OxenAmount::from_decimal_string("1.0");
    let _coin_amount = GenericCoinAmount::from_decimal_string(coin_type, "2.0");

    let quote_tx = TestData::deposit_quote(coin_type);

    let wtx_oxen = TestData::witness(quote_tx.unique_id(), oxen_amount.to_atomic(), Coin::OXEN);
    let wtx_oxen = StatusWitnessWrapper::new(wtx_oxen, WitnessStatus::Confirmed);

    let quote_tx = FulfilledWrapper::new(quote_tx, false);

    let tx = process_deposit_quote(&quote_tx, &[&wtx_oxen], Network::Testnet);

    assert!(tx.is_none())
}

#[test]
fn refunds_if_deposit_quote_is_fulfilled() {
    let coin_type = Coin::BTC;
    let oxen_amount = OxenAmount::from_decimal_string("1.0");
    let btc_amount = GenericCoinAmount::from_decimal_string(Coin::BTC, "2.0");

    let quote_tx = TestData::deposit_quote(coin_type);

    let wtx_oxen = TestData::witness(quote_tx.unique_id(), oxen_amount.to_atomic(), Coin::OXEN);
    let wtx_oxen = StatusWitnessWrapper::new(wtx_oxen, WitnessStatus::Confirmed);

    let wtx_btc = TestData::witness(quote_tx.unique_id(), btc_amount.to_atomic(), coin_type);
    let wtx_btc = StatusWitnessWrapper::new(wtx_btc, WitnessStatus::Confirmed);

    let quote_tx = FulfilledWrapper::new(quote_tx, true);

    // Processing fulfilled deposit quote should return nothing
    assert!(process_deposit_quote(&quote_tx, &[&wtx_oxen, &wtx_btc], Network::Testnet).is_none());

    let outputs = refund_deposit_quotes(&quote_tx, &[&wtx_oxen, &wtx_btc], Network::Testnet);
    assert_eq!(outputs.len(), 2);

    let oxen_output = outputs.iter().find(|tx| tx.coin == Coin::OXEN).unwrap();
    assert_eq!(oxen_output.address, quote_tx.inner.base_return_address);
    assert_eq!(oxen_output.amount, oxen_amount.to_atomic());

    let btc_output = outputs.iter().find(|tx| tx.coin == Coin::BTC).unwrap();
    assert_eq!(btc_output.address, quote_tx.inner.coin_return_address);
    assert_eq!(btc_output.amount, btc_amount.to_atomic());
}

#[test]
fn witness_tx_cannot_be_reused() {
    let coin_type = Coin::ETH;
    let oxen_amount = OxenAmount::from_decimal_string("1.0");
    let coin_amount = GenericCoinAmount::from_decimal_string(coin_type, "2.0");

    let quote_tx = TestData::deposit_quote(coin_type);

    let wtx_oxen = TestData::witness(quote_tx.unique_id(), oxen_amount.to_atomic(), Coin::OXEN);
    // Witness has already been used before
    let wtx_oxen = StatusWitnessWrapper::new(wtx_oxen, WitnessStatus::Processed);

    let wtx_eth = TestData::witness(quote_tx.unique_id(), coin_amount.to_atomic(), coin_type);
    let wtx_eth = StatusWitnessWrapper::new(wtx_eth, WitnessStatus::AwaitingConfirmation);

    let quote_tx = FulfilledWrapper::new(quote_tx, false);

    let tx = process_deposit_quote(&quote_tx, &[&wtx_oxen, &wtx_eth], Network::Testnet);

    assert!(tx.is_none())
}

#[test]
fn quote_cannot_be_fulfilled_twice() {
    let coin_type = Coin::ETH;
    let oxen_amount = OxenAmount::from_decimal_string("1.0");
    let coin_amount = GenericCoinAmount::from_decimal_string(coin_type, "2.0");

    let quote_tx = TestData::deposit_quote(coin_type);

    let wtx_oxen = TestData::witness(quote_tx.unique_id(), oxen_amount.to_atomic(), Coin::OXEN);
    let wtx_oxen = StatusWitnessWrapper::new(wtx_oxen, WitnessStatus::AwaitingConfirmation);

    let wtx_eth = TestData::witness(quote_tx.unique_id(), coin_amount.to_atomic(), coin_type);
    let wtx_eth = StatusWitnessWrapper::new(wtx_eth, WitnessStatus::AwaitingConfirmation);

    // The quote has already been fulfilled
    let quote_tx = FulfilledWrapper::new(quote_tx, true);

    let tx = process_deposit_quote(&quote_tx, &[&wtx_oxen, &wtx_eth], Network::Testnet);

    assert!(tx.is_none())
}

#[test]
fn check_staking_smaller_amounts() {
    let oxen_amount = OxenAmount::from_decimal_string("1.0");

    let coin_type = Coin::ETH;
    let coin_amount = GenericCoinAmount::from_decimal_string(coin_type, "2.0");

    let quote_tx = TestData::deposit_quote(coin_type);

    let wtx_oxen = TestData::witness(quote_tx.unique_id(), oxen_amount.to_atomic(), Coin::OXEN);
    let wtx_oxen = StatusWitnessWrapper::new(wtx_oxen, WitnessStatus::Confirmed);

    let wtx_eth = TestData::witness(quote_tx.unique_id(), coin_amount.to_atomic(), coin_type);
    let wtx_eth = StatusWitnessWrapper::new(wtx_eth, WitnessStatus::Confirmed);

    let quote_tx = FulfilledWrapper::new(quote_tx, false);

    let res = process_deposit_quote(&quote_tx, &[&wtx_oxen, &wtx_eth], Network::Testnet).unwrap();

    assert_eq!(
        res.pool_change.depth_change as u128,
        coin_amount.to_atomic()
    );
    assert_eq!(
        res.pool_change.base_depth_change as u128,
        oxen_amount.to_atomic()
    );

    assert_eq!(res.deposit.pool_change, res.pool_change.unique_id());
    assert_eq!(res.deposit.quote, quote_tx.inner.unique_id());
    assert!(res.deposit.witnesses.contains(&wtx_oxen.inner.unique_id()));
    assert!(res.deposit.witnesses.contains(&wtx_eth.inner.unique_id()));
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
