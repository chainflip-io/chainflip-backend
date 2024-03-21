use crate::*;
use frame_support::{migration::move_prefix, traits::OnRuntimeUpgrade};
pub struct Migration<T>(PhantomData<T>);

mod old {
	use super::*;

	#[frame_support::storage_alias]
	pub type MaximumRelativeSlippage<T: Config> = StorageValue<Pallet<T>, u32, OptionQuery>;
}

impl<T: pallet::Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> Weight {
		// let price_impact: Option<u32> = old::MaximumRelativeSlippage::<T>::get();
		// MaximumRelativePriceImpact::<T>::set(price_impact);
		// old::MaximumRelativeSlippage::<T>::move_prefix(
		// 	b"MaximumRelativeSlippage",
		// 	b"MaximumRelativePriceImpact",
		// );
		move_prefix(b"MaximumRelativeSlippage", b"MaximumRelativePriceImpact");
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let slippage = old::MaximumRelativeSlippage::<T>::get().unwrap();

		Ok(slippage.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let slippage = <u32>::decode(&mut &state[..]).expect("Pre-migration should encode a u32.");

		ensure!(
			MaximumRelativePriceImpact::<T>::get().unwrap() == slippage,
			"DepositChannelLookup migration failed."
		);

		Ok(())
	}
}

#[cfg(test)]
mod migration_tests {
	use crate::mock::Test;

	use self::mock::new_test_ext;

	use super::*;

	#[test]
	fn test_migration() {
		new_test_ext().execute_with(|| {
			old::MaximumRelativeSlippage::<Test>::put(100);

			#[cfg(feature = "try-runtime")]
			let state: Vec<u8> =
				crate::migrations::rename_slippage_to_price_impact::Migration::<Test>::pre_upgrade(
				)
				.unwrap();
			println!("{:?}", old::MaximumRelativeSlippage::<Test>::get());
			// Perform runtime migration.
			crate::migrations::rename_slippage_to_price_impact::Migration::<Test>::on_runtime_upgrade();

			#[cfg(feature = "try-runtime")]
			crate::migrations::rename_slippage_to_price_impact::Migration::<Test>::post_upgrade(
				state,
			)
			.unwrap();

			// Verify data is correctly migrated into new storage.
			let price_impact = MaximumRelativePriceImpact::<Test>::get();
			println!("{:?}", price_impact);
			assert!(price_impact.is_some());

			assert_eq!(price_impact.unwrap(), 100);
		});
	}
}
