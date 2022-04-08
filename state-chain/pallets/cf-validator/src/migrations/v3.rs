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

		let number_of_current_validators = Validators::<T>::get().len() as u32;
		EpochValidatorCount::<T>::insert(T::EpochInfo::epoch_index(), number_of_current_validators);
		T::DbWeight::get().reads_writes(2 as Weight, 3 as Weight)
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
		Ok(())
	}
}
