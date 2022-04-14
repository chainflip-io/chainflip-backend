use cf_traits::EpochTransitionHandler;

use crate::{AccountId, Emissions, EthereumVault};

pub struct ChainflipEpochTransitions;

impl EpochTransitionHandler for ChainflipEpochTransitions {
	type ValidatorId = AccountId;

	fn on_new_epoch(epoch_validators: &[Self::ValidatorId]) {
		<Emissions as EpochTransitionHandler>::on_new_epoch(epoch_validators);
		<EthereumVault as EpochTransitionHandler>::on_new_epoch(epoch_validators);
	}
}
