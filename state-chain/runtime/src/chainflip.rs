//! Configuration, utilities and helpers for the Chainflip runtime.
use super::{AccountId, Emissions, Flip, FlipBalance, Reputation, Rewards, Runtime, Witnesser};
use cf_traits::{BondRotation, EmissionsTrigger, StakeHandler};
use frame_support::debug;
use pallet_cf_validator::EpochTransitionHandler;
use pallet_cf_auction::HandleStakes;

pub struct ChainflipEpochTransitions;

/// Trigger emissions on epoch transitions.
impl EpochTransitionHandler for ChainflipEpochTransitions {
	type ValidatorId = AccountId;
	type Amount = FlipBalance;

	fn on_new_epoch(_new_validators: &[Self::ValidatorId], _new_bond: Self::Amount) {
		// Process any outstanding emissions.
		<Emissions as EmissionsTrigger>::trigger_emissions();
		// Rollover the rewards.
		Rewards::rollover(_new_validators).unwrap_or_else(|err| {
			debug::error!("Unable to process rewards rollover: {:?}!", err);
		});
		// Update the the bond of all validators for the new epoch
		<Flip as BondRotation>::update_validator_bonds(_new_validators, _new_bond);
		// Update the list of validators in reputation
		<Reputation as EpochTransitionHandler>::on_new_epoch(_new_validators, _new_bond);
		// Update the list of validators in the witnesser.
		<Witnesser as EpochTransitionHandler>::on_new_epoch(_new_validators, _new_bond)
	}
}

pub struct ChainflipStakeHandler;
impl StakeHandler for ChainflipStakeHandler {
	type ValidatorId = AccountId;
	type Amount = FlipBalance;

	fn stake_updated(validator_id: &Self::ValidatorId, new_total: Self::Amount) {
		HandleStakes::<Runtime>::stake_updated(validator_id, new_total);
	}
}