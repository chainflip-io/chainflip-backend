use cf_primitives::EpochIndex;
use cf_traits::EpochTransitionHandler;

use crate::Refunding;

use crate::Witnesser;

pub struct ChainflipEpochTransitions;

impl EpochTransitionHandler for ChainflipEpochTransitions {
	fn on_expired_epoch(expired: EpochIndex) {
		Refunding::on_distribute_withheld_fees(expired);
		<Witnesser as EpochTransitionHandler>::on_expired_epoch(expired);
	}
}
