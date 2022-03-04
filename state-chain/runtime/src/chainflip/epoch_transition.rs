use cf_traits::{
	BlockEmissions, Bonding, EmissionsTrigger, EpochExpiry, EpochTransitionHandler, FlipBalance,
};

use crate::{chainflip::EpochHistory, AccountId, Emissions, Flip, Runtime, Validator, Witnesser};
use cf_traits::{
	Chainflip, ChainflipAccount, ChainflipAccountStore, EpochIndex, EpochInfo, HistoricalEpoch,
};

use crate::chainflip::PhantomData;

pub struct ChainflipEpochTransitions;

/// Trigger emissions on epoch transitions.
impl EpochTransitionHandler for ChainflipEpochTransitions {
	type ValidatorId = AccountId;
	type Amount = FlipBalance;

	fn on_new_epoch(
		old_validators: &[Self::ValidatorId],
		new_validators: &[Self::ValidatorId],
		new_bond: Self::Amount,
	) {
		// Calculate block emissions on every epoch
		<Emissions as BlockEmissions>::calculate_block_emissions();
		// Process any outstanding emissions.
		<Emissions as EmissionsTrigger>::trigger_emissions();
		// Update the the bond of all validators for the new epoch
		for validator in new_validators {
			BondManager::bond_validator(validator);
		}
		// Update the list of validators in the witnesser.
		<Witnesser as EpochTransitionHandler>::on_new_epoch(
			old_validators,
			new_validators,
			new_bond,
		);

		<AccountStateManager<Runtime> as EpochTransitionHandler>::on_new_epoch(
			old_validators,
			new_validators,
			new_bond,
		);

		<pallet_cf_online::Pallet<Runtime> as cf_traits::KeygenExclusionSet>::forgive_all();
	}
}

pub struct AccountStateManager<T>(PhantomData<T>);

impl<T: Chainflip> EpochTransitionHandler for AccountStateManager<T> {
	type ValidatorId = AccountId;
	type Amount = T::Amount;

	fn on_new_epoch(
		_old_validators: &[Self::ValidatorId],
		new_validators: &[Self::ValidatorId],
		_new_bid: Self::Amount,
	) {
		// Update the last active epoch for the new validating set
		let epoch_index = Validator::epoch_index();
		for validator in new_validators {
			ChainflipAccountStore::<Runtime>::update_last_active_epoch(validator, epoch_index);
		}
	}
}

pub struct EpochExpiryHandler;

impl EpochExpiry for EpochExpiryHandler {
	fn expire_epoch(epoch: EpochIndex) {
		EpochHistory::<Runtime>::set_last_expired_epoch(epoch);
		for validator in EpochHistory::<Runtime>::epoch_validators(epoch).iter() {
			EpochHistory::<Runtime>::remove_epoch(validator, epoch);
			BondManager::bond_validator(validator);
		}
	}
}

pub struct BondManager;

impl Bonding for BondManager {
	type ValidatorId = AccountId;
	fn bond_validator(validator: &Self::ValidatorId) {
		let active_epochs = EpochHistory::<Runtime>::active_epochs_for_validator(validator);
		if active_epochs.is_empty() {
			Flip::set_validator_bond(validator, 0u128);
		} else {
			Flip::set_validator_bond(
				validator,
				active_epochs
					.iter()
					.map(|bond| EpochHistory::<Runtime>::epoch_bond(*bond))
					.max()
					.expect("we expect at least one active epoch"),
			);
		}
	}
}
