use cf_traits::{EpochTransitionHandler, FlipBalance};

use crate::{AccountId, Emissions, Online, Witnesser};

pub struct ChainflipEpochTransitions;

impl EpochTransitionHandler for ChainflipEpochTransitions {
	type ValidatorId = AccountId;
	type Amount = FlipBalance;

	fn on_new_epoch(
		old_validators: &[Self::ValidatorId],
		new_validators: &[Self::ValidatorId],
		_new_bond: Self::Amount,
	) {
		<Emissions as EpochTransitionHandler>::on_new_epoch(old_validators, new_validators, ());
		<Witnesser as EpochTransitionHandler>::on_new_epoch(old_validators, new_validators, ());
		<Online as EpochTransitionHandler>::on_new_epoch(old_validators, new_validators, ());
	}
}
