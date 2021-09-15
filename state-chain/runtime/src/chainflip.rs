//! Configuration, utilities and helpers for the Chainflip runtime.
use super::{AccountId, Emissions, Flip, FlipBalance, Reputation, Rewards, Witnesser};
use cf_traits::EmissionsTrigger;
use frame_support::debug;
use pallet_cf_validator::EpochTransitionHandler;
use sp_std::vec::Vec;

pub struct ChainflipEpochTransitions;

/// Trigger emissions on epoch transitions.
impl EpochTransitionHandler for ChainflipEpochTransitions {
	type AccountId = AccountId;
	type Amount = FlipBalance;

	fn on_new_epoch(new_validators: &Vec<Self::AccountId>, new_bond: Self::Amount) {
		// Process any outstanding emissions.
		<Emissions as EmissionsTrigger>::trigger_emissions();
		// Rollover the rewards.
		Rewards::rollover(new_validators).unwrap_or_else(|err| {
			debug::error!("Unable to process rewards rollover: {:?}!", err);
		});
		// Update the the bond of all validators for the new epoch
		Flip::update_validator_bonds(new_validators, new_bond);
		// Update the list of validators in reputation
		<Reputation as EpochTransitionHandler>::on_new_epoch(new_validators, new_bond);
		// Update the list of validators in the witnesser.
		<Witnesser as EpochTransitionHandler>::on_new_epoch(new_validators, new_bond)
	}
}
