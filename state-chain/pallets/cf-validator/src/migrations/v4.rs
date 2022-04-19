use crate::*;
use sp_std::marker::PhantomData;

pub struct Migration<T: Config>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		// Get the current state of the storage.
		let current_epoch = T::EpochInfo::epoch_index();
		let current_validators = Validators::<T>::get();
		let current_bond = Bond::<T>::get();
		// Set the historical state for the current epoch.
		HistoricalValidators::<T>::insert(current_epoch, current_validators.clone());
		HistoricalBonds::<T>::insert(current_epoch, current_bond);
		for validator in current_validators.clone() {
			HistoricalActiveEpochs::<T>::insert(validator, vec![current_epoch]);
		}
		#[allow(clippy::unnecessary_cast)]
		T::DbWeight::get().reads_writes(3 as Weight, 2 + current_validators.len() as Weight)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<(), &'static str> {
		Ok(())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade() -> Result<(), &'static str> {
		assert_eq!(
			HistoricalValidators::<T>::get(T::EpochInfo::epoch_index()),
			Validators::<T>::get(),
			"HistoricalValidators and Validators are not equal"
		);
		assert_eq!(
			HistoricalBonds::<T>::get(T::EpochInfo::epoch_index()),
			Bond::<T>::get(),
			"HistoricalBonds and Bond are not equal"
		);
		for validator in Validators::<T>::get() {
			assert_eq!(
				HistoricalActiveEpochs::<T>::get(validator),
				vec![T::EpochInfo::epoch_index()],
				"HistoricalActiveEpochs and Validators are not equal"
			);
		}
		Ok(())
	}
}
