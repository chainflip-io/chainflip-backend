use crate::*;
use frame_support::traits::OnRuntimeUpgrade;
pub struct Migration<T>(PhantomData<T>);

mod old {
	use super::*;

	#[frame_support::storage_alias]
	pub type MaximumRelativeSlippage<T: Config> = StorageValue<Pallet<T>, u32, OptionQuery>;
}

impl<T: pallet::Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> Weight {
		let price_impact: Option<u32> = old::MaximumRelativeSlippage::<T>::get();

		Pools::<T>::iter_keys().for_each(|asset_pair| {
			MaximumPriceImpact::<T>::mutate(asset_pair, |limit| *limit = price_impact);
		});

		old::MaximumRelativeSlippage::<T>::kill();

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let slippage = old::MaximumRelativeSlippage::<T>::get();

		Ok(slippage.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let slippage =
			Option::<u32>::decode(&mut &state[..]).expect("Pre-migration should encode a u32.");

		Pools::<T>::iter_keys().for_each(|asset_pair| {
			assert_eq!(MaximumPriceImpact::<T>::get(asset_pair), slippage);
		});

		Ok(())
	}
}

#[cfg(test)]
mod migration_tests {
	use crate::mock::{RuntimeOrigin, Test};

	use self::mock::new_test_ext;
	use super::*;
	use frame_support::assert_ok;

	#[test]
	fn test_migration() {
		new_test_ext().execute_with(|| {
			let limit = 100u32;
			old::MaximumRelativeSlippage::<Test>::put(limit);

			let base_asset = Asset::Eth;
			let asset_pair = AssetPair::try_new::<Test>(base_asset, STABLE_ASSET)
				.expect("Asset pair must succeed.");
			let default_price = cf_amm::common::price_at_tick(0).unwrap();

			assert_ok!(Pallet::<Test>::new_pool(
				RuntimeOrigin::root(),
				base_asset,
				STABLE_ASSET,
				500_000u32,
				default_price,
			));

			#[cfg(feature = "try-runtime")]
			let state: Vec<u8> =
				crate::migrations::rename_slippage_to_price_impact::Migration::<Test>::pre_upgrade(
				)
				.unwrap();
			// Perform runtime migration.
			crate::migrations::rename_slippage_to_price_impact::Migration::<Test>::on_runtime_upgrade();

			#[cfg(feature = "try-runtime")]
			crate::migrations::rename_slippage_to_price_impact::Migration::<Test>::post_upgrade(
				state,
			)
			.unwrap();

			// Verify data is correctly migrated into new storage.
			assert_eq!(MaximumPriceImpact::<Test>::get(asset_pair), Some(limit));
		});
	}
}
