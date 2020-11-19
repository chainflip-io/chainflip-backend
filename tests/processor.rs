use blockswap::{
    common::*,
    transactions::signatures::get_random_staker,
    transactions::UnstakeRequestTx,
    utils::test_utils::{self, *},
    vault::transactions::memory_provider::Portion,
    vault::transactions::TransactionProvider,
};

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
    assert_eq!(liquidity.loki_depth, loki_amount.to_atomic());
    assert_eq!(liquidity.depth, coin_amount.to_atomic());
}

fn create_unstake_tx(pool: PoolCoin, staker: &Staker) -> UnstakeRequestTx {
    create_unstake_for_staker(pool, staker)
}

#[test]
fn witnessed_staked_changes_pool_liquidity() {
    let mut runner = TestRunner::new();

    let coin_type = Coin::ETH;
    let loki_amount = LokiAmount::from_decimal_string("1.0");
    let coin_amount = GenericCoinAmount::from_decimal_string(coin_type, "2.0");

    let stake_tx = create_fake_stake_quote(PoolCoin::from(coin_type).unwrap());
    let wtx_loki = create_fake_witness(&stake_tx, loki_amount, Coin::LOKI);
    let wtx_eth = create_fake_witness(&stake_tx, coin_amount, coin_type);

    runner.add_block([stake_tx.clone().into()]);
    runner.add_block([wtx_loki.into(), wtx_eth.into()]);

    check_liquidity(
        &mut *runner.provider.write(),
        coin_type,
        loki_amount,
        coin_amount,
    );

    runner.add_block([stake_tx.clone().into()]);

    // Check that the balance has not changed
    check_liquidity(
        &mut *runner.provider.write(),
        coin_type,
        loki_amount,
        coin_amount,
    );
}

#[test]
fn multiple_stakes() {
    test_utils::logging::init();

    let mut runner = TestRunner::new();

    // 1. Make a Stake TX and make sure it is acknowledged

    let coin_type = Coin::ETH;
    let loki_amount = LokiAmount::from_decimal_string("1.0");
    let coin_amount = GenericCoinAmount::from_decimal_string(coin_type, "2.0");

    let stake_tx = create_fake_stake_quote(PoolCoin::from(coin_type).unwrap());
    let wtx_loki = create_fake_witness(&stake_tx, loki_amount, Coin::LOKI);
    let wtx_eth = create_fake_witness(&stake_tx, coin_amount, coin_type);

    // Add blocks with those transactions
    runner.add_block([stake_tx.clone().into()]);
    runner.add_block([wtx_loki.into(), wtx_eth.into()]);

    check_liquidity(
        &mut *runner.provider.write(),
        coin_type,
        loki_amount,
        coin_amount,
    );

    // 2. Add another stake with another staker id

    let stake_tx = create_fake_stake_quote(PoolCoin::from(coin_type).unwrap());
    let wtx_loki = create_fake_witness(&stake_tx, loki_amount, Coin::LOKI);
    let wtx_eth = create_fake_witness(&stake_tx, coin_amount, coin_type);

    runner.add_block([stake_tx.clone().into()]);
    runner.add_block([wtx_loki.into(), wtx_eth.into()]);
}

#[test]
fn sole_staker_can_unstake_all() {
    test_utils::logging::init();

    let mut runner = TestRunner::new();

    let loki_amount = LokiAmount::from_decimal_string("1.0");
    let eth_amount = GenericCoinAmount::from_decimal_string(Coin::ETH, "2.0");

    let staker = get_random_staker();

    let stake_tx = runner.add_witnessed_stake_tx(&staker.id(), loki_amount, eth_amount);

    // Check that the liquidity is non-zero before unstaking
    runner.check_eth_liquidity(loki_amount.to_atomic(), eth_amount.to_atomic());

    let unstake_tx = create_unstake_tx(stake_tx.coin_type, &staker);

    runner.add_block([unstake_tx.clone().into()]);

    // Check that outputs have been payed out
    let outputs = runner.get_outputs_for_unstake(&unstake_tx);

    assert_eq!(outputs.loki_output.amount, loki_amount.to_atomic());
    assert_eq!(outputs.eth_output.amount, eth_amount.to_atomic());

    // Check that liquidity is 0 after unstaking. (Is this even a valid state???)
    runner.check_eth_liquidity(0, 0);
}

#[test]
fn half_staker_can_unstake_half() {
    test_utils::logging::init();

    let mut runner = TestRunner::new();

    let loki_amount = LokiAmount::from_decimal_string("1.0");
    let eth_amount = GenericCoinAmount::from_decimal_string(Coin::ETH, "2.0");

    let alice = get_random_staker();
    let bob = get_random_staker();

    let _ = runner.add_witnessed_stake_tx(&alice.id(), loki_amount, eth_amount);
    let stake2 = runner.add_witnessed_stake_tx(&bob.id(), loki_amount, eth_amount);

    // Check that liquidity is the sum of two stakes
    runner.check_eth_liquidity(loki_amount.to_atomic() * 2, eth_amount.to_atomic() * 2);

    let unstake_tx = create_unstake_tx(stake2.coin_type, &bob);
    runner.add_block([unstake_tx.clone().into()]);

    // Check that outputs have been payed out
    let outputs = runner.get_outputs_for_unstake(&unstake_tx);

    assert_eq!(outputs.loki_output.amount, loki_amount.to_atomic());
    assert_eq!(outputs.eth_output.amount, eth_amount.to_atomic());

    // Check that liquidity halved
    runner.check_eth_liquidity(loki_amount.to_atomic(), eth_amount.to_atomic());
}

#[test]
fn portions_adjusted_after_unstake() {
    // Two stakers, one unstakes, the other
    // should own MAX portions

    test_utils::logging::init();

    let mut runner = TestRunner::new();

    let loki_amount = LokiAmount::from_decimal_string("1.0");
    let eth_amount = GenericCoinAmount::from_decimal_string(Coin::ETH, "2.0");

    let alice = get_random_staker();
    let bob = get_random_staker();

    let _stake1 = runner.add_witnessed_stake_tx(&alice.id(), loki_amount, eth_amount);
    let _stake2 = runner.add_witnessed_stake_tx(&bob.id(), loki_amount, eth_amount);

    // Each should have 50% portions

    let portions_alice = runner
        .get_portions_for(&alice.id(), PoolCoin::ETH)
        .expect("Alice must have portions");
    let portions_bob = runner
        .get_portions_for(&bob.id(), PoolCoin::ETH)
        .expect("Bob must have portions");

    assert_eq!(portions_alice.0, Portion::MAX.0 / 2);
    assert_eq!(portions_bob.0, Portion::MAX.0 / 2);

    // Bob unstakes

    runner.add_unstake_for(&bob, PoolCoin::ETH);

    // Alice should have 100%, bob 0%

    let portions_alice = runner
        .get_portions_for(&alice.id(), PoolCoin::ETH)
        .expect("Alice must have portions");
    let portions_bob = runner.get_portions_for(&bob.id(), PoolCoin::ETH);

    assert_eq!(portions_alice, Portion::MAX);
    assert!(portions_bob.is_err(), "Bob must not have portions");
}

#[test]
fn non_staker_cannot_unstake() {
    test_utils::logging::init();

    let mut runner = TestRunner::new();

    let loki_amount = LokiAmount::from_decimal_string("1.0");
    let eth_amount = GenericCoinAmount::from_decimal_string(Coin::ETH, "2.0");

    let alice = get_random_staker();

    let _ = runner.add_witnessed_stake_tx(&alice.id(), loki_amount, eth_amount);

    let bob = get_random_staker();

    // Bob creates a stake quote tx, but never pays the amounts:
    let stake =
        create_fake_stake_quote_for_id(bob.id(), PoolCoin::from(eth_amount.coin_type()).unwrap());

    runner.add_block([stake.clone().into()]);

    // Bob tries to unstake:
    let unstake_tx = create_unstake_tx(stake.coin_type, &bob);
    runner.add_block([unstake_tx.clone().into()]);

    // Check that no outputs are created:
    let sent_outputs = runner.sent_outputs.lock().unwrap();

    let outputs = sent_outputs
        .iter()
        .filter(|output| output.quote_tx == unstake_tx.id)
        .count();

    assert_eq!(outputs, 0);
}

#[test]
fn assymetric_stake_result_in_autoswap() {
    test_utils::logging::init();

    let mut runner = TestRunner::new();

    let loki_amount = LokiAmount::from_decimal_string("500.0");
    let btc_amount = GenericCoinAmount::from_decimal_string(Coin::BTC, "0.02");

    let alice = get_random_staker();
    let _ = runner.add_witnessed_stake_tx(&alice.id(), loki_amount, btc_amount);

    let bob = get_random_staker();

    let loki_amount = LokiAmount::from_decimal_string("250.0");
    let btc_amount = GenericCoinAmount::from_decimal_string(Coin::BTC, "0.028");

    let _ = runner.add_witnessed_stake_tx(&bob.id(), loki_amount, btc_amount);

    let a = runner
        .get_portions_for(&alice.id(), PoolCoin::BTC)
        .expect("Portion should exist for Alice");
    let b = runner
        .get_portions_for(&bob.id(), PoolCoin::BTC)
        .expect("Portion should exist for Bob");

    dbg!(a, b);
}

#[test]
#[ignore = "todo"]
fn cannot_unstake_with_invalid_signature() {
    todo!();
}
