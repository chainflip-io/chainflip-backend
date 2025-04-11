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
	fn assert_pool_fees(range_order_fee: u32, limit_order_fee: u32) {
		let pool = Pools::<Test>::iter_values().next().unwrap();

		assert_eq!(pool.pool_state.range_order_fee(), range_order_fee);
		assert_eq!(pool.pool_state.limit_order_fee(), limit_order_fee);
	}

	use cf_amm::math::price_at_tick;
	use cf_utilities::assert_ok;
	use mock::{new_test_ext, RuntimeOrigin, Test};

	use super::*;

	#[test]
	fn test_migration() {
		const INIT_POOL_FEE: u32 = 500;

		new_test_ext().execute_with(|| {
			assert_ok!(Pallet::<Test>::new_pool(
				RuntimeOrigin::root(),
				BASE_ASSET,
				STABLE_ASSET,
				INIT_POOL_FEE,
				price_at_tick(0).unwrap(),
			));

			assert_pool_fees(INIT_POOL_FEE, INIT_POOL_FEE);

			Migration::<Test>::on_runtime_upgrade();

			assert_pool_fees(INIT_POOL_FEE, 0);
		});
	}
}
