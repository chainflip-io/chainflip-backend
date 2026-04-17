use crate::*;
use frame_support::traits::UncheckedOnRuntimeUpgrade;

pub struct Migration<T: Config>(PhantomData<T>);

mod old {
	use super::*;

	#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
	pub struct BoostConfiguration {
		pub network_fee_deduction_from_boost_percent: Percent,
		pub minimum_add_funds_amount: BTreeMap<Asset, AssetAmount>,
	}

	#[frame_support::storage_alias]
	pub type BoostConfig<T: Config> = StorageValue<Pallet<T>, BoostConfiguration, OptionQuery>;
}

const DEFAULT_MIN_LENDING_POOL_SHARE: Percent = Percent::from_percent(30);

impl<T: Config> UncheckedOnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> Weight {
		if let Some(old_config) = old::BoostConfig::<T>::take() {
			BoostConfig::<T>::put(BoostConfiguration {
				network_fee_deduction_from_boost_percent: old_config
					.network_fee_deduction_from_boost_percent,
				minimum_add_funds_amount: old_config.minimum_add_funds_amount,
				min_lending_pool_share: DEFAULT_MIN_LENDING_POOL_SHARE,
			});
		}
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		Ok(old::BoostConfig::<T>::get().encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let old_config = <Option<old::BoostConfiguration>>::decode(&mut &state[..])
			.map_err(|_| "Failed to decode pre_upgrade state")?;
		let new_config = BoostConfig::<T>::get();
		assert_eq!(new_config.min_lending_pool_share, DEFAULT_MIN_LENDING_POOL_SHARE);
		if let Some(old_config) = old_config {
			assert_eq!(
				new_config.network_fee_deduction_from_boost_percent,
				old_config.network_fee_deduction_from_boost_percent,
			);
			assert_eq!(new_config.minimum_add_funds_amount, old_config.minimum_add_funds_amount);
		}
		Ok(())
	}
}
