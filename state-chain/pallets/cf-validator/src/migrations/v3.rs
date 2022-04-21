use crate::*;
use frame_support::storage::migration::*;
use sp_std::marker::PhantomData;

pub struct Migration<T: Config>(PhantomData<T>);

const WITNESSER_NAME: &[u8] = b"Witnesser";
const VALIDATOR_NAME: &[u8] = b"Validator";
const VALIDATOR_INDEX_NAME: &[u8] = b"ValidatorIndex";
const VALIDATOR_LOOKUP_NAME: &[u8] = b"ValidatorLookup";
const NUM_VALIDATORS_NAME: &[u8] = b"NumValidators";

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		move_storage_from_pallet(VALIDATOR_INDEX_NAME, WITNESSER_NAME, VALIDATOR_NAME);
		remove_storage_prefix(VALIDATOR_NAME, VALIDATOR_LOOKUP_NAME, b"");
		remove_storage_prefix(WITNESSER_NAME, NUM_VALIDATORS_NAME, b"");

		// Get the current state of the storage.
		let current_epoch = T::EpochInfo::epoch_index();
		let current_validators = Validators::<T>::get();
		let current_bond = Bond::<T>::get();
		let number_of_current_validators = current_validators.len() as u32;

		EpochValidatorCount::<T>::insert(current_epoch, number_of_current_validators);

		// We have 2 stable writes as well as n for itterating over all validators.
		let writes = 2 + number_of_current_validators;

		// Insert the´current epoch bond into the storage.
		HistoricalBonds::<T>::insert(current_epoch, current_bond);
		for validator in &current_validators {
			HistoricalActiveEpochs::<T>::insert(validator, vec![current_epoch]);
		}

		// Set the historical state for the current epoch.
		HistoricalValidators::<T>::insert(current_epoch, current_validators);

		#[allow(clippy::unnecessary_cast)]
		T::DbWeight::get().reads_writes(3 as Weight, writes as Weight)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<(), &'static str> {
		assert!(
			storage_iter::<()>(WITNESSER_NAME, VALIDATOR_INDEX_NAME).next().is_some(),
			"ValidatorIndex not found in Witnesser"
		);
		assert!(
			get_storage_value::<u32>(WITNESSER_NAME, NUM_VALIDATORS_NAME, b"").is_some(),
			"NumValidators not found in Witnesser"
		);
		assert!(
			storage_iter::<()>(VALIDATOR_NAME, VALIDATOR_LOOKUP_NAME).next().is_some(),
			"ValidatorLookup not found in Validator"
		);
		Ok(())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade() -> Result<(), &'static str> {
		assert!(
			storage_iter::<()>(VALIDATOR_NAME, VALIDATOR_INDEX_NAME).next().is_some(),
			"ValidatorIndex not found in Validator"
		);
		assert!(
			storage_iter::<()>(VALIDATOR_NAME, b"EpochValidatorCount").next().is_some(),
			"EpochValidatorCount not found in Validator"
		);
		assert_eq!(
			HistoricalValidators::<T>::get(T::EpochInfo::epoch_index()),
			Validators::<T>::get(),
			"HistoricalValidators for this Epoch and Current Validators are not equal"
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
				"HistoricalActiveEpochs and known current active epochs are not equal"
			);
		}
		Ok(())
	}
}
