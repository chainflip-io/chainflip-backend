use frame_support::traits::UncheckedOnRuntimeUpgrade;

use crate::*;

pub struct Migration<T: Config>(PhantomData<T>);

impl<T: Config> UncheckedOnRuntimeUpgrade for Migration<T> {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let range_order_fees: Vec<u32> = Pools::<T>::iter_values()
			.map(|pool| pool.pool_state.range_order_fee())
			.collect();

		Ok(range_order_fees.encode())
	}

	fn on_runtime_upgrade() -> Weight {
		// Setting pool fee for limit orders to 0 (to make sure all existing fees
		// are collected correctly)

		// Collect to avoid undefined behaviour (See StorageMap::iter_keys documentation).
		for asset_pair in Pools::<T>::iter_keys().collect::<Vec<_>>() {
			if let Err(err) =
				Pallet::<T>::try_mutate_pool(asset_pair, |asset_pair: &AssetPair, pool| {
					Pallet::<T>::set_pool_fee_for_limit_orders(pool, asset_pair, 0)
				}) {
				log_or_panic!("Failed to set pool fee for limit orders during migration: {err:?}");
			}
		}

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		// Range order fees should not have changed:
		let old_range_order_fees: Vec<u32> = Decode::decode(&mut state.as_slice())
			.map_err(|_| DispatchError::from("Failed to decode state"))?;

		let new_range_order_fees: Vec<u32> = Pools::<T>::iter_values()
			.map(|pool| pool.pool_state.range_order_fee())
			.collect();

		assert_eq!(old_range_order_fees, new_range_order_fees);

		assert!(
			Pools::<T>::iter_values().all(|pool| pool.pool_state.limit_order_fee() == 0),
			"limit order pool fees should be set to 0"
		);

		Ok(())
	}
}

#[cfg(test)]
mod tests {

	const BASE_ASSET: Asset = Asset::Usdt;

	#[track_caller]
	fn assert_balances(account_id: u64, base_amount: AssetAmount, quote_amount: AssetAmount) {
		assert_eq!(MockBalance::get_balance(&account_id, BASE_ASSET), base_amount);
		assert_eq!(MockBalance::get_balance(&account_id, STABLE_ASSET), quote_amount);
	}

	#[track_caller]
	fn assert_pool_fees(range_order_fee: u32, limit_order_fee: u32) {
		let pool = Pools::<Test>::iter_values().next().unwrap();

		assert_eq!(pool.pool_state.range_order_fee(), range_order_fee);
		assert_eq!(pool.pool_state.limit_order_fee(), limit_order_fee);
	}

	fn fees_to_collect() -> AssetAmount {
		let pool = Pools::<Test>::iter_values().next().unwrap();

		pool.pool_state.clone().collect_all_limit_orders().quote[0]
			.2
			.fees
			.try_into()
			.unwrap()
	}

	use cf_amm::math::price_at_tick;
	use cf_traits::mocks::balance_api::MockBalance;
	use cf_utilities::assert_ok;
	use mock::{new_test_ext, RuntimeOrigin, Test, ALICE};

	use super::*;

	#[test]
	fn test_migration() {
		const SWAP_AMOUNT: AssetAmount = 250_000_000;
		const INIT_POOL_FEE: u32 = 500; // 5bps pool fee

		new_test_ext().execute_with(|| {
			assert_ok!(Pallet::<Test>::new_pool(
				RuntimeOrigin::root(),
				BASE_ASSET,
				STABLE_ASSET,
				INIT_POOL_FEE,
				price_at_tick(0).unwrap(),
			));

			assert_pool_fees(INIT_POOL_FEE, INIT_POOL_FEE);

			MockBalance::credit_account(&ALICE, STABLE_ASSET, SWAP_AMOUNT * 2);
			assert_ok!(Pallet::<Test>::set_limit_order(
				RuntimeOrigin::signed(ALICE),
				BASE_ASSET,
				STABLE_ASSET,
				Side::Buy,
				0,
				Some(0),
				SWAP_AMOUNT * 2
			));

			// Initially there aren't any fees to collect:
			assert_eq!(fees_to_collect(), 0);
			assert_balances(ALICE, 0, 0);

			assert_ok!(Pallet::<Test>::swap_single_leg(BASE_ASSET, STABLE_ASSET, SWAP_AMOUNT));

			// Should have accrued some fees after the swap, but they are not yet collected:
			let pool_fees = fees_to_collect();
			assert!(pool_fees > 0);
			assert_balances(ALICE, 0, 0);

			Migration::<Test>::on_runtime_upgrade();

			assert_pool_fees(INIT_POOL_FEE, 0);

			// Runtime upgrade should have collected the fees into the LP account:
			assert_eq!(fees_to_collect(), 0);
			assert_balances(ALICE, SWAP_AMOUNT + pool_fees, 0);
		});
	}
}
