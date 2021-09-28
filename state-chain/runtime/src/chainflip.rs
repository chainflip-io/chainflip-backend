//! Configuration, utilities and helpers for the Chainflip runtime.
use super::{AccountId, Emissions, Flip, FlipBalance, Reputation, Rewards, Runtime, Validator, Witnesser};
use cf_traits::{BondRotation, EmergencyRotation, EmissionsTrigger, Heartbeat, NetworkState};
use frame_support::debug;
use pallet_cf_validator::{EpochTransitionHandler, EmergencyRotationOf};
use sp_std::vec::Vec;
use crate::EmergencyRotationPercentageTrigger;

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
		// Update the the bond of all validators for the new epoch
		<Flip as BondRotation>::update_validator_bonds(new_validators, new_bond);
		// Update the list of validators in reputation
		<Reputation as EpochTransitionHandler>::on_new_epoch(new_validators, new_bond);
		// Update the list of validators in the witnesser.
		<Witnesser as EpochTransitionHandler>::on_new_epoch(new_validators, new_bond)
	}
}

pub struct ChainflipHeartbeat;

impl Heartbeat for ChainflipHeartbeat {
	type ValidatorId = AccountId;

	fn on_heartbeat_interval(network_state: NetworkState<Self::ValidatorId>) {
		// We pay rewards to backup validators on each heartbeat interval

		// Check the state of the network and if we are below the emergency rotation trigger
		// then issue an emergency rotation request
		if network_state.percentage_online()
			< EmergencyRotationPercentageTrigger::get() as u32
		{
			EmergencyRotationOf::<Runtime>::request_emergency_rotation();
		}
	}
}

#[test]
fn test_this() {}
