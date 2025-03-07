use cf_primitives::{Asset, AssetAmount, Tick, STABLE_ASSET};
use cf_test_utilities::assert_event_sequence;
use cf_traits::{
	mocks::{
		balance_api::MockBalance,
		pool_api::{MockLimitOrder, MockPoolApi},
	},
	BalanceApi, Side,
};
use frame_support::{assert_err, assert_ok};

use crate::{mock::*, *};

const BASE_ASSET: Asset = Asset::Usdt;
const BASE_AMOUNT: AssetAmount = 100_000;
const QUOTE_AMOUNT: AssetAmount = 50_000;

const BUY_TICK: Tick = -1;
const SELL_TICK: Tick = 1;

type AccountId = u64;

const STRATEGY: TradingStrategy =
	TradingStrategy::SellAndBuyAtTicks { sell_tick: SELL_TICK, buy_tick: BUY_TICK };

fn get_balance(account_id: AccountId) -> (AssetAmount, AssetAmount) {
	(
		MockBalance::get_balance(&account_id, BASE_ASSET),
		MockBalance::get_balance(&account_id, STABLE_ASSET),
	)
}

fn deploy_strategy() -> AccountId {
	MockBalance::credit_account(&LP, BASE_ASSET, BASE_AMOUNT);
	MockBalance::credit_account(&LP, STABLE_ASSET, QUOTE_AMOUNT);

	assert_ok!(TradingStrategyPallet::deploy_trading_strategy(
		RuntimeOrigin::signed(LP),
		BASE_AMOUNT,
		QUOTE_AMOUNT,
		BASE_ASSET,
		STRATEGY.clone(),
	));

	// An entry for the trading agent is created:
	let (strategy_id, strategy_entry) = Strategies::<Test>::iter().next().unwrap();
	assert_eq!(
		strategy_entry,
		TradingStrategyEntry { base_asset: BASE_ASSET, strategy: STRATEGY, owner: LP }
	);

	assert_event_sequence!(
		Test,
		RuntimeEvent::TradingStrategyPallet(Event::<Test>::StrategyDeployed {
			account_id: LP,
			strategy_id: id,
			base_asset: BASE_ASSET,
			strategy: STRATEGY,
		}) if id == strategy_id,
		RuntimeEvent::TradingStrategyPallet(Event::<Test>::FundsAddedToStrategy {
			base_asset: BASE_ASSET,
			strategy_id: id,
			base_asset_amount: BASE_AMOUNT,
			quote_asset_amount: QUOTE_AMOUNT,
		}) if id == strategy_id,
	);

	// The funds are moved from the LP to the strategy:
	assert_eq!(get_balance(strategy_id), (BASE_AMOUNT, QUOTE_AMOUNT));
	assert_eq!(get_balance(LP), (0, 0));

	strategy_id
}

#[test]
fn automated_strategy_basic_usage() {
	const ADDITIONAL_BASE_AMOUNT: AssetAmount = 5_000;

	new_test_ext()
		.then_execute_at_next_block(|_| deploy_strategy())
		.then_execute_at_next_block(|strategy_id| {
			// The strategy should have created two limit orders:
			assert_eq!(
				MockPoolApi::get_limit_orders(),
				vec![
					MockLimitOrder {
						base_asset: BASE_ASSET,
						account_id: strategy_id,
						side: Side::Buy,
						order_id: STRATEGY_ORDER_ID,
						tick: BUY_TICK,
						amount: QUOTE_AMOUNT
					},
					MockLimitOrder {
						base_asset: BASE_ASSET,
						account_id: strategy_id,
						side: Side::Sell,
						order_id: STRATEGY_ORDER_ID,
						tick: SELL_TICK,
						amount: BASE_AMOUNT
					}
				]
			);

			// Add additional funds by calling the add funds extrinsic.
			MockBalance::credit_account(&LP, BASE_ASSET, ADDITIONAL_BASE_AMOUNT);
			assert_ok!(TradingStrategyPallet::add_funds_to_strategy(
				RuntimeOrigin::signed(LP),
				ADDITIONAL_BASE_AMOUNT,
				0,
				strategy_id
			));

			// Update the threshold to check that limit orders won't be updated
			// if the threshold is not reached:
			TradingStrategyParametersStorage::<Test>::mutate(|params| {
				params
					.order_update_thresholds
					.try_insert(BASE_ASSET, ADDITIONAL_BASE_AMOUNT * 2)
					.unwrap();
			});

			assert_eq!(get_balance(LP), (0, 0));
			assert_eq!(get_balance(strategy_id), (ADDITIONAL_BASE_AMOUNT, 0));

			strategy_id
		})
		.then_execute_at_next_block(|strategy_id| {
			// The funds have not been added to the limit order yet
			assert_eq!(get_balance(strategy_id), (ADDITIONAL_BASE_AMOUNT, 0));

			// This time we credit the strategy directly (which is what would happen
			// if our limit order is executed in the pools pallet). Now the strategy
			// should have enough free balance in BASE ASSET to update the limit order:
			MockBalance::credit_account(&strategy_id, BASE_ASSET, ADDITIONAL_BASE_AMOUNT);

			strategy_id
		})
		.then_execute_at_next_block(|strategy_id| {
			// The should now have been used to update the limit order:
			assert_eq!(get_balance(strategy_id), (0, 0));

			assert_eq!(
				MockPoolApi::get_limit_orders(),
				vec![
					MockLimitOrder {
						base_asset: BASE_ASSET,
						account_id: strategy_id,
						side: Side::Buy,
						order_id: STRATEGY_ORDER_ID,
						tick: BUY_TICK,
						amount: QUOTE_AMOUNT
					},
					MockLimitOrder {
						base_asset: BASE_ASSET,
						account_id: strategy_id,
						side: Side::Sell,
						order_id: STRATEGY_ORDER_ID,
						tick: SELL_TICK,
						amount: BASE_AMOUNT + ADDITIONAL_BASE_AMOUNT * 2
					}
				]
			);
		});
}

#[test]
fn closing_strategy() {
	const ADDITIONAL_BASE_AMOUNT: AssetAmount = 5_000;
	new_test_ext()
		.then_execute_at_next_block(|_| deploy_strategy())
		.then_execute_at_next_block(|strategy_id| {
			// Two orders must have been created:
			assert_eq!(MockPoolApi::get_limit_orders().len(), 2);

			// Credit the strategy account so has a non-zero free balance:
			MockBalance::credit_account(&LP, BASE_ASSET, ADDITIONAL_BASE_AMOUNT);
			assert_eq!(get_balance(LP), (ADDITIONAL_BASE_AMOUNT, 0));

			// Other LPs can't close our strategy
			assert_err!(
				TradingStrategyPallet::close_strategy(RuntimeOrigin::signed(OTHER_LP), strategy_id),
				Error::<Test>::InvalidOwner
			);

			// Closing the strategy
			assert_ok!(TradingStrategyPallet::close_strategy(
				RuntimeOrigin::signed(LP),
				strategy_id
			));

			assert_event_sequence!(
				Test,
				RuntimeEvent::TradingStrategyPallet(Event::<Test>::StrategyClosed {
					strategy_id: id,
				}) if id == strategy_id,
			);

			// Limit orders should be closed:
			assert!(MockPoolApi::get_limit_orders().is_empty());
			assert_eq!(Strategies::<Test>::iter().count(), 0);
			assert_eq!(get_balance(strategy_id), (0, 0));
		});
}

#[test]
fn strategy_deployment_threshold() {
	const MIN_BASE_AMOUNT: AssetAmount = 10_000;
	const MIN_QUOTE_AMOUNT: AssetAmount = 1_000;

	new_test_ext().then_execute_at_next_block(|_| {
		MockBalance::credit_account(&LP, BASE_ASSET, MIN_BASE_AMOUNT * 10);
		MockBalance::credit_account(&LP, STABLE_ASSET, MIN_QUOTE_AMOUNT * 10);

		TradingStrategyParametersStorage::<Test>::mutate(|params| {
			params
				.strategy_deployment_thresholds
				.try_insert(BASE_ASSET, MIN_BASE_AMOUNT)
				.unwrap();
			params
				.strategy_deployment_thresholds
				.try_insert(STABLE_ASSET, MIN_QUOTE_AMOUNT)
				.unwrap();
		});

		for (base_amount, quote_amount) in [
			(MIN_BASE_AMOUNT - 1, 0),
			(0, MIN_QUOTE_AMOUNT - 1),
			(MIN_BASE_AMOUNT / 2 - 1, MIN_QUOTE_AMOUNT / 2),
			(MIN_BASE_AMOUNT / 10 - 1, (MIN_QUOTE_AMOUNT * 9) / 10),
		] {
			assert_err!(
				TradingStrategyPallet::deploy_trading_strategy(
					RuntimeOrigin::signed(LP),
					base_amount,
					quote_amount,
					BASE_ASSET,
					STRATEGY.clone(),
				),
				Error::<Test>::AmountBelowDeploymentThreshold
			);
		}

		for (base_amount, quote_amount) in [
			(MIN_BASE_AMOUNT, 0),
			(0, MIN_QUOTE_AMOUNT),
			(MIN_BASE_AMOUNT / 2, MIN_QUOTE_AMOUNT / 2),
			(MIN_BASE_AMOUNT / 10, (MIN_QUOTE_AMOUNT * 9) / 10),
		] {
			assert_ok!(TradingStrategyPallet::deploy_trading_strategy(
				RuntimeOrigin::signed(LP),
				base_amount,
				quote_amount,
				BASE_ASSET,
				STRATEGY.clone(),
			));
		}
	});
}
