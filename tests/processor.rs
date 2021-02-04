use chainflip::{
    common::*,
    utils::test_utils::{self, staking::get_random_staker, *},
    vault::transactions::memory_provider::Portion,
    vault::transactions::TransactionProvider,
};
use chainflip_common::types::{coin::Coin, unique_id::GetUniqueId};
use data::TestData;

fn check_liquidity<T>(
    tx_provider: &mut T,
    coin_type: Coin,
    loki_amount: LokiAmount,
    coin_amount: GenericCoinAmount,
) where
    T: TransactionProvider,
{
    tx_provider.sync();

    let liquidity = tx_provider
        .get_liquidity(PoolCoin::from(coin_type).unwrap())
        .unwrap();

    // Check that a pool with the right amount was created
    assert_eq!(liquidity.base_depth, loki_amount.to_atomic());
    assert_eq!(liquidity.depth, coin_amount.to_atomic());
}

#[test]
fn witnessed_deposit_changes_pool_liquidity() {
    let mut runner = TestRunner::new();

    let coin_type = Coin::ETH;
    let loki_amount = LokiAmount::from_decimal_string("1.0");
    let coin_amount = GenericCoinAmount::from_decimal_string(coin_type, "2.0");

    let deposit_quote = TestData::deposit_quote(coin_type);
    let wtx_loki = TestData::witness(
        deposit_quote.unique_id(),
        loki_amount.to_atomic(),
        Coin::LOKI,
    );
    let wtx_eth = TestData::witness(
        deposit_quote.unique_id(),
        coin_amount.to_atomic(),
        coin_type,
    );

    runner.add_local_events([deposit_quote.clone().into()]);
    runner.add_local_events([wtx_loki.into(), wtx_eth.into()]);

    check_liquidity(
        &mut *runner.provider.write(),
        coin_type,
        loki_amount,
        coin_amount,
    );

    runner.add_local_events([deposit_quote.clone().into()]);

    // Check that the balance has not changed
    check_liquidity(
        &mut *runner.provider.write(),
        coin_type,
        loki_amount,
        coin_amount,
    );
}

#[test]
fn multiple_deposits() {
    test_utils::logging::init();

    let mut runner = TestRunner::new();

    // 1. Make a deposit and make sure it is acknowledged

    let coin_type = Coin::ETH;
    let loki_amount = LokiAmount::from_decimal_string("1.0");
    let coin_amount = GenericCoinAmount::from_decimal_string(coin_type, "2.0");

    let deposit_quote = TestData::deposit_quote(coin_type);
    let wtx_loki = TestData::witness(
        deposit_quote.unique_id(),
        loki_amount.to_atomic(),
        Coin::LOKI,
    );
    let wtx_eth = TestData::witness(
        deposit_quote.unique_id(),
        coin_amount.to_atomic(),
        coin_type,
    );

    // Add blocks with those transactions
    runner.add_local_events([deposit_quote.clone().into()]);
    runner.add_local_events([wtx_loki.into(), wtx_eth.into()]);

    check_liquidity(
        &mut *runner.provider.write(),
        coin_type,
        loki_amount,
        coin_amount,
    );

    // 2. Add another deposit with another staker id

    let deposit_quote = TestData::deposit_quote(coin_type);
    let wtx_loki = TestData::witness(
        deposit_quote.unique_id(),
        loki_amount.to_atomic(),
        Coin::LOKI,
    );
    let wtx_eth = TestData::witness(
        deposit_quote.unique_id(),
        coin_amount.to_atomic(),
        coin_type,
    );

    runner.add_local_events([deposit_quote.clone().into()]);
    runner.add_local_events([wtx_loki.into(), wtx_eth.into()]);
}

#[test]
fn sole_staker_can_withdraw_all() {
    test_utils::logging::init();

    let mut runner = TestRunner::new();

    let loki_amount = LokiAmount::from_decimal_string("1.0");
    let eth_amount = GenericCoinAmount::from_decimal_string(Coin::ETH, "2.0");

    let staker = get_random_staker();

    let deposit_quote = runner.add_witnessed_deposit_quote(&staker.id(), loki_amount, eth_amount);

    // Check that the liquidity is non-zero before unstaking
    runner.check_eth_liquidity(loki_amount.to_atomic(), eth_amount.to_atomic());

    let withdraw_request = TestData::withdraw_request_for_staker(&staker, deposit_quote.pool);

    runner.add_local_events([withdraw_request.clone().into()]);

    // Check that outputs have been payed out
    let outputs = runner.get_outputs_for_withdraw_request(&withdraw_request);

    assert_eq!(outputs.loki_output.amount, loki_amount.to_atomic());
    assert_eq!(outputs.eth_output.amount, eth_amount.to_atomic());

    // Check that liquidity is 0 after unstaking. (Is this even a valid state???)
    runner.check_eth_liquidity(0, 0);
}

#[test]
fn half_staker_can_withdraw_half() {
    test_utils::logging::init();

    let mut runner = TestRunner::new();

    let loki_amount = LokiAmount::from_decimal_string("1.0");
    let eth_amount = GenericCoinAmount::from_decimal_string(Coin::ETH, "2.0");

    let alice = get_random_staker();
    let bob = get_random_staker();

    let _ = runner.add_witnessed_deposit_quote(&alice.id(), loki_amount, eth_amount);
    let deposit2 = runner.add_witnessed_deposit_quote(&bob.id(), loki_amount, eth_amount);

    // Check that liquidity is the sum of two deposits
    runner.check_eth_liquidity(loki_amount.to_atomic() * 2, eth_amount.to_atomic() * 2);

    let withdraw_request = TestData::withdraw_request_for_staker(&bob, deposit2.pool);
    runner.add_local_events([withdraw_request.clone().into()]);

    // Check that outputs have been payed out
    let outputs = runner.get_outputs_for_withdraw_request(&withdraw_request);

    assert_eq!(outputs.loki_output.amount, loki_amount.to_atomic());
    assert_eq!(outputs.eth_output.amount, eth_amount.to_atomic());

    // Check that liquidity halved
    runner.check_eth_liquidity(loki_amount.to_atomic(), eth_amount.to_atomic());
}

#[test]
fn portions_adjusted_after_withdraw() {
    // Two stakers, one withdraws, the other
    // should own MAX portions

    test_utils::logging::init();

    let mut runner = TestRunner::new();

    let loki_amount = LokiAmount::from_decimal_string("1.0");
    let eth_amount = GenericCoinAmount::from_decimal_string(Coin::ETH, "2.0");

    let alice = get_random_staker();
    let bob = get_random_staker();

    runner.add_witnessed_deposit_quote(&alice.id(), loki_amount, eth_amount);
    runner.add_witnessed_deposit_quote(&bob.id(), loki_amount, eth_amount);

    // Each should have 50% portions

    let portions_alice = runner
        .get_portions_for(&alice.id(), PoolCoin::ETH)
        .expect("Alice must have portions");

    println!("Alice has portions");
    let portions_bob = runner
        .get_portions_for(&bob.id(), PoolCoin::ETH)
        .expect("Bob must have portions");
    println!("Bob has portions");
    assert_eq!(portions_alice.0, Portion::MAX.0 / 2);
    assert_eq!(portions_bob.0, Portion::MAX.0 / 2);

    // Bob withdraws

    runner.add_withdraw_request_for(&bob, PoolCoin::ETH);

    // Alice should have 100%, bob 0%

    let portions_alice = runner
        .get_portions_for(&alice.id(), PoolCoin::ETH)
        .expect("Alice must have portions");
    let portions_bob = runner.get_portions_for(&bob.id(), PoolCoin::ETH);

    assert_eq!(portions_alice, Portion::MAX);
    assert!(portions_bob.is_err(), "Bob must not have portions");
}

#[test]
fn non_staker_cannot_withdraw() {
    test_utils::logging::init();

    let mut runner = TestRunner::new();

    let loki_amount = LokiAmount::from_decimal_string("1.0");
    let eth_amount = GenericCoinAmount::from_decimal_string(Coin::ETH, "2.0");

    let alice = get_random_staker();

    let _ = runner.add_witnessed_deposit_quote(&alice.id(), loki_amount, eth_amount);

    let bob = get_random_staker();

    // Bob creates a deposit quote, but never pays the amounts:
    let deposit_quote = TestData::deposit_quote_for_id(bob.id(), eth_amount.coin_type());

    runner.add_local_events([deposit_quote.clone().into()]);

    // Bob tries to withdraw:
    let withdraw_request = TestData::withdraw_request_for_staker(&bob, deposit_quote.pool);
    runner.add_local_events([withdraw_request.clone().into()]);

    // Check that no outputs are created:
    let sent_outputs = runner.sent_outputs.lock().unwrap();

    let outputs = sent_outputs
        .iter()
        .filter(|output| output.parent_id() == withdraw_request.unique_id())
        .count();

    assert_eq!(outputs, 0);
}

#[test]
fn asymmetric_deposit_result_in_autoswap() {
    test_utils::logging::init();

    let mut runner = TestRunner::new();

    let loki_amount = LokiAmount::from_decimal_string("500.0");
    let btc_amount = GenericCoinAmount::from_decimal_string(Coin::BTC, "0.02");

    let alice = get_random_staker();
    let _ = runner.add_witnessed_deposit_quote(&alice.id(), loki_amount, btc_amount);

    let bob = get_random_staker();

    let loki_amount = LokiAmount::from_decimal_string("250.0");
    let btc_amount = GenericCoinAmount::from_decimal_string(Coin::BTC, "0.02");

    let _ = runner.add_witnessed_deposit_quote(&bob.id(), loki_amount, btc_amount);

    // observe the witness

    let a = runner
        .get_portions_for(&alice.id(), PoolCoin::BTC)
        .expect("Portion should exist for Alice");
    let b = runner
        .get_portions_for(&bob.id(), PoolCoin::BTC)
        .expect("Portion should exist for Bob");

    // We expect the 50% < a < 66% (Bob deposits a 50% of Alices Loki,
    // but the same amount of BTC, resulting in autoswap)
    assert_eq!(a.0, 6162430986);
    assert_eq!(b.0, 3837569014);
}

#[test]
#[ignore = "todo"]
fn cannot_withdraw_with_invalid_signature() {
    todo!();
}
