use crate::*;
use cf_runtime_upgrade_utilities::move_storage;
use frame_support::storage::{migration::*, storage_prefix};
use sp_std::marker::PhantomData;

pub struct Migration<T: Config>(PhantomData<T>);

const WITNESSER_PALLET_NAME: &[u8] = b"Witnesser";
const VALIDATOR_PALLET_NAME: &[u8] = b"Validator";
const VALIDATORS_NAME: &[u8] = b"Validators";
const CURRENT_AUTHORITIES_NAME: &[u8] = b"CurrentAuthorities";
const VALIDATOR_INDEX_NAME: &[u8] = b"ValidatorIndex";
const AUTHORITY_INDEX_NAME: &[u8] = b"AuthorityIndex";
const VALIDATOR_LOOKUP_NAME: &[u8] = b"ValidatorLookup";
const NUM_VALIDATORS_NAME: &[u8] = b"NumValidators";

impl<T: Config> OnRuntimeUpgrade for Migration<T> {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		// These are no longer used
		remove_storage_prefix(VALIDATOR_PALLET_NAME, VALIDATOR_LOOKUP_NAME, b"");
		remove_storage_prefix(WITNESSER_PALLET_NAME, NUM_VALIDATORS_NAME, b"");

		move_storage(
			VALIDATOR_PALLET_NAME,
			VALIDATORS_NAME,
			VALIDATOR_PALLET_NAME,
			CURRENT_AUTHORITIES_NAME,
		);

		// move it from witnesser pallet to validator pallet *and* rename it from ValidatorIndex to
		// AuthorityIndex
		let validator_index_prefix = storage_prefix(WITNESSER_PALLET_NAME, VALIDATOR_INDEX_NAME);
		let authority_index_prefix = storage_prefix(VALIDATOR_PALLET_NAME, AUTHORITY_INDEX_NAME);
		move_prefix(&validator_index_prefix, &authority_index_prefix);

		let validator_cfe_version_prefix =
			storage_prefix(VALIDATOR_PALLET_NAME, b"ValidatorCFEVersion");
		let node_cfe_version = storage_prefix(VALIDATOR_PALLET_NAME, b"NodeCFEVersion");
		move_prefix(&validator_cfe_version_prefix, &node_cfe_version);

		// Get the current state of the storage.
		let current_epoch = T::EpochInfo::epoch_index();
		let current_validators = CurrentAuthorities::<T>::get();
		let current_bond = Bond::<T>::get();
		let number_of_current_validators = current_validators.len() as u32;

		EpochAuthorityCount::<T>::insert(current_epoch, number_of_current_validators);

		// Insert the´current epoch bond into the storage.
		HistoricalBonds::<T>::insert(current_epoch, current_bond);
		for validator in &current_validators {
			HistoricalActiveEpochs::<T>::insert(validator, vec![current_epoch]);
		}

		// Set the historical state for the current epoch.
		HistoricalAuthorities::<T>::insert(current_epoch, current_validators);

		// We have 6 stable writes as well as n for itterating over all validators.
		#[allow(clippy::unnecessary_cast)]
		T::DbWeight::get().reads_writes(3 as Weight, 6 + number_of_current_validators as Weight)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<(), &'static str> {
		use sp_runtime::AccountId32;

		assert!(get_storage_value::<Vec<AccountId32>>(VALIDATOR_PALLET_NAME, VALIDATORS_NAME, b"")
			.is_some());
		assert!(
			storage_iter::<()>(WITNESSER_PALLET_NAME, VALIDATOR_INDEX_NAME).next().is_some(),
			"ValidatorIndex not found in Witnesser"
		);
		assert!(
			get_storage_value::<u32>(WITNESSER_PALLET_NAME, NUM_VALIDATORS_NAME, b"").is_some(),
			"NumValidators not found in Witnesser"
		);
		assert!(
			storage_iter::<()>(VALIDATOR_PALLET_NAME, VALIDATOR_LOOKUP_NAME)
				.next()
				.is_some(),
			"ValidatorLookup not found in Validator"
		);
		Ok(())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade() -> Result<(), &'static str> {
		let epoch_index = T::EpochInfo::epoch_index();
		assert!(EpochAuthorityCount::<T>::get(&epoch_index).is_some());
		assert!(!CurrentAuthorities::<T>::get().is_empty());
		assert_eq!(
			HistoricalAuthorities::<T>::get(T::EpochInfo::epoch_index()),
			CurrentAuthorities::<T>::get(),
			"HistoricalAuthorities for this Epoch and CurrentAuthorities are not equal"
		);
		assert_eq!(
			HistoricalBonds::<T>::get(T::EpochInfo::epoch_index()),
			Bond::<T>::get(),
			"HistoricalBonds and Bond are not equal"
		);

		for validator in CurrentAuthorities::<T>::get() {
			assert!(AuthorityIndex::<T>::get(epoch_index, &validator).is_some());
			assert_eq!(
				HistoricalActiveEpochs::<T>::get(validator),
				vec![epoch_index],
				"HistoricalActiveEpochs and known current active epochs are not equal"
			);
		}
		Ok(())
	}
}
