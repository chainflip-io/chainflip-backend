use cf_traits::{BlockEmissions, EmissionsTrigger, EpochTransitionHandler, FlipBalance};

use crate::{AccountId, Emissions, Reputation, Runtime, Validator, Witnesser};
use cf_traits::{Chainflip, ChainflipAccount, ChainflipAccountStore, EpochInfo};

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

		<Reputation as EpochTransitionHandler>::on_new_epoch(
			old_validators,
			new_validators,
			new_bond,
		);
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
