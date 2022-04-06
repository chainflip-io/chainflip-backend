use cf_traits::EpochTransitionHandler;

use crate::{AccountId, Emissions, EthereumVault, Reputation, Witnesser};

pub struct ChainflipEpochTransitions;

impl EpochTransitionHandler for ChainflipEpochTransitions {
	type ValidatorId = AccountId;

	fn on_new_epoch(
		previous_epoch_validators: &[Self::ValidatorId],
		epoch_validators: &[Self::ValidatorId],
	) {
		<Emissions as EpochTransitionHandler>::on_new_epoch(
			previous_epoch_validators,
			epoch_validators,
		);
		<Witnesser as EpochTransitionHandler>::on_new_epoch(
			previous_epoch_validators,
			epoch_validators,
		);
		<Reputation as EpochTransitionHandler>::on_new_epoch(
			previous_epoch_validators,
			epoch_validators,
		);
		<EthereumVault as EpochTransitionHandler>::on_new_epoch(
			previous_epoch_validators,
			epoch_validators,
		);
	}
}
