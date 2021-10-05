//! Configuration, utilities and helpers for the Chainflip runtime.
use super::{AccountId, Emissions, Flip, FlipBalance, Reputation, Rewards, Witnesser};
use cf_traits::{BondRotation, EpochTransitionHandler, EmissionsTrigger, StakeHandler, VaultRotationHandler, RewardRollover};
use frame_support::debug;
use sp_std::vec::Vec;
use pallet_cf_auction::{HandleStakes, VaultRotationEventHandler};

pub struct ChainflipEpochTransitions;

/// Trigger emissions on epoch transitions.
impl EpochTransitionHandler for ChainflipEpochTransitions {
	type ValidatorId = AccountId;
	type Amount = FlipBalance;

	fn on_new_epoch(new_validators: &[Self::ValidatorId], new_bond: Self::Amount) {
		// Process any outstanding emissions.
		<Emissions as EmissionsTrigger>::trigger_emissions();
		// Rollover the rewards.
		<Rewards as RewardRollover>::rollover(new_validators).unwrap_or_else(|err| {
			debug::error!("Unable to process rewards rollover: {:?}!", err);
		});
		// Update the the bond of all validators for the new epoch
		<Flip as BondRotation>::update_validator_bonds(new_validators, new_bond);
		// Update the list of validators in reputation
		<Reputation as EpochTransitionHandler>::on_new_epoch(new_validators, new_bond);
		// Update the list of validators in the witnesser.
		<Witnesser as EpochTransitionHandler>::on_new_epoch(new_validators, new_bond)
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

pub struct ChainflipVaultRotationHandler;
impl VaultRotationHandler for ChainflipVaultRotationHandler {
	type ValidatorId = AccountId;

	fn vault_rotation_aborted() {
		VaultRotationEventHandler::<Runtime>::vault_rotation_aborted();
	}

	fn penalise(bad_validators: &[Self::ValidatorId]) {
		VaultRotationEventHandler::<Runtime>::penalise(bad_validators);
	}
}
