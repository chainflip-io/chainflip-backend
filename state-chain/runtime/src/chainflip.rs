//! Configuration, utilities and helpers for the Chainflip runtime.
use super::{AccountId, Emissions, FlipBalance, Reputation, Rewards, Validator, Witnesser};
use cf_traits::EmissionsTrigger;
use frame_support::debug;
use pallet_cf_validator::EpochTransitionHandler;
use sp_std::vec::Vec;

pub struct ChainflipEpochTransitions;

/// Trigger emissions on epoch transitions.
impl EpochTransitionHandler for ChainflipEpochTransitions {
	type ValidatorId = AccountId;
	type Amount = FlipBalance;

	fn on_new_epoch(new_validators: &Vec<Self::ValidatorId>, new_bond: Self::Amount) {
		// Process any outstanding emissions.
		<Emissions as EmissionsTrigger>::trigger_emissions();
		// Rollover the rewards.
		Rewards::rollover(new_validators).unwrap_or_else(|err| {
			debug::error!("Unable to process rewards rollover: {:?}!", err);
		});
		// Update the list of validators in reputation
		<Reputation as EpochTransitionHandler>::on_new_epoch(new_validators, new_bond);
		// Update the list of validators in the witnesser.
		<Witnesser as EpochTransitionHandler>::on_new_epoch(new_validators, new_bond)
	}
}

pub struct BasicSignerNomination;

impl pallet_cf_transaction_broadcast::SignerNomination for BasicSignerNomination {
	type SignerId = AccountId;

	fn nomination_with_seed(seed: u64) -> Self::SignerId {
		todo!()
	}

	fn threshold_nomination_with_seed(seed: u64) -> Vec<Self::SignerId> {
		todo!()
	}
}