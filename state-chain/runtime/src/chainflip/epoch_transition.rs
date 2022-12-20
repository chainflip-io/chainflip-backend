use cf_primitives::EpochIndex;
use cf_traits::EpochTransitionHandler;

use crate::{AccountId, EthereumVault, Witnesser};

pub struct ChainflipEpochTransitions;

impl EpochTransitionHandler for ChainflipEpochTransitions {
	type ValidatorId = AccountId;

	fn on_new_epoch(epoch_authorities: &[Self::ValidatorId]) {
		<EthereumVault as EpochTransitionHandler>::on_new_epoch(epoch_authorities);
	}

	fn on_expired_epoch(expired: EpochIndex) {
		<Witnesser as EpochTransitionHandler>::on_expired_epoch(expired);
	}
}
