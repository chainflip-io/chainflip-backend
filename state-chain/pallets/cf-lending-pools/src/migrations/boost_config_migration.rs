use frame_support::traits::UncheckedOnRuntimeUpgrade;

use crate::Config;

use crate::*;
use frame_support::pallet_prelude::Weight;
#[cfg(feature = "try-runtime")]
use frame_support::sp_runtime::DispatchError;

pub mod old {
	use super::*;

	// Moving this setting into the BoostConfig struct
	#[frame_support::storage_alias]
	pub type NetworkFeeDeductionFromBoostPercent<T: Config> =
		StorageValue<Pallet<T>, Percent, ValueQuery>;
}

pub struct Migration<T: Config>(PhantomData<T>);

impl<T: Config> UncheckedOnRuntimeUpgrade for Migration<T> {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		let percent = old::NetworkFeeDeductionFromBoostPercent::<T>::get();
		Ok(percent.encode())
	}

	fn on_runtime_upgrade() -> Weight {
		let percent = old::NetworkFeeDeductionFromBoostPercent::<T>::take();
		BoostConfig::<T>::put(BoostConfiguration {
			network_fee_deduction_from_boost_percent: percent,
			..BoostConfigDefault::get()
		});

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		let new_fee = BoostConfig::<T>::get().network_fee_deduction_from_boost_percent;
		let old_fee = Percent::decode(&mut state.as_slice())
			.map_err(|_| DispatchError::from("Failed to decode state"))?;
		assert_eq!(new_fee, old_fee);
		log::info!(
			"🧜‍♂️ Migration successful: network_fee_deduction_from_boost_percent is now set to {:?}",
			new_fee
		);

		Ok(())
	}
}
