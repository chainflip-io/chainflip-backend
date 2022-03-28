use cf_traits::EpochTransitionHandler;

use crate::{AccountId, Emissions, Online, Witnesser};

pub struct ChainflipEpochTransitions;

impl EpochTransitionHandler for ChainflipEpochTransitions {
	type ValidatorId = AccountId;

	fn on_new_epoch(old_validators: &[Self::ValidatorId], new_validators: &[Self::ValidatorId]) {
		<Emissions as EpochTransitionHandler>::on_new_epoch(old_validators, new_validators);
		<Witnesser as EpochTransitionHandler>::on_new_epoch(old_validators, new_validators);
		<Online as EpochTransitionHandler>::on_new_epoch(old_validators, new_validators);
	}
}
