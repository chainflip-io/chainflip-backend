//! Configuration, utilities and helpers for the Chainflip runtime.
use pallet_cf_auction::{HandleStakes, VaultRotationEventHandler};
use super::{
	AccountId, Emissions, Flip, FlipBalance, Online, Reputation, Rewards, Runtime, Validator,
	Witnesser,
};
use crate::EmergencyRotationPercentageTrigger;
use cf_traits::{BondRotation, ChainflipAccount, ChainflipAccountState, ChainflipAccountStore, EmergencyRotation, EmissionsTrigger, EpochInfo, Heartbeat, NetworkState, StakeTransfer, StakeHandler, VaultRotationHandler};
use frame_support::{debug, weights::Weight};
use pallet_cf_validator::EpochTransitionHandler;
use sp_std::vec::Vec;
use sp_std::cmp::min;

pub struct ChainflipEpochTransitions;

/// Trigger emissions on epoch transitions.
impl EpochTransitionHandler for ChainflipEpochTransitions {
	type ValidatorId = AccountId;
	type Amount = FlipBalance;

	fn on_new_epoch(new_validators: &[Self::ValidatorId], new_bond: Self::Amount) {
		// Process any outstanding emissions.
		<Emissions as EmissionsTrigger>::trigger_emissions();
		// Rollover the rewards.
		Rewards::rollover(new_validators).unwrap_or_else(|err| {
			debug::error!("Unable to process rewards rollover: {:?}!", err);
		});
		// Update the the bond of all validators for the new epoch
		<Flip as BondRotation>::update_validator_bonds(new_validators, new_bond);
		// Update the list of validators in reputation
		<Online as EpochTransitionHandler>::on_new_epoch(new_validators, new_bond);
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

trait RewardDistibrution {
	type EpochInfo: EpochInfo;
	type StakeTransfer: StakeTransfer;
	type ValidatorId;

	fn distribute_rewards(backup_validators: Vec<&Self::ValidatorId>) -> Weight;
}

struct BackupEmissions;
impl RewardDistibrution for BackupEmissions {
	type EpochInfo = Validator;
	type StakeTransfer = Flip;
	type ValidatorId = AccountId;

	fn distribute_rewards(backup_validators: Vec<&Self::ValidatorId>) -> Weight {
		let minimum_active_bid = Self::EpochInfo::bond();
		// BV emissions cap: 1% of total emissions.
		let emissions_cap = 0;
		// rAV: average validator reward earned by each active validator;
		let average_validator_reward = Rewards::rewards_due_each();

		// TODO map rewards to each and sum the total to calculate the capping factor

		for backup_validator in backup_validators {
			let backup_validator_stake = Self::StakeTransfer::stakeable_balance(backup_validator);
			let reward_scaling_factor = min(1, (backup_validator_stake / minimum_active_bid)^2);
			let backup_validator_reward = (reward_scaling_factor * average_validator_reward * 8) / 10;
		}

		// rBV: reward earned by a backup validator;
		//
		// F: reward scaling factor;
		// F =  min(1, (BVstake / MAB)^2);
		// rBV = 0.8 * F * rAV;
		//
		//
		// if the sum of all rBV > BV emission cap, then calculate capping factor CF:
		// 	CF = emission cap / sum(all rBV),
		// Then apply factor to rewards
		// rBV = rBV * capping factor
		0
	}
}

pub struct ChainflipHeartbeat;

impl Heartbeat for ChainflipHeartbeat {
	type ValidatorId = AccountId;

	fn heartbeat_submitted(validator_id: &Self::ValidatorId) -> Weight {
		<Reputation as Heartbeat>::heartbeat_submitted(validator_id)
	}

	fn on_heartbeat_interval(network_state: NetworkState<Self::ValidatorId>) -> Weight {
		// Reputation depends on heartbeats
		let mut weight = <Reputation as Heartbeat>::on_heartbeat_interval(network_state.clone());

		// We pay rewards to online backup validators on each heartbeat interval
		let backup_validators: Vec<&Self::ValidatorId> = network_state.online.iter().filter(|account_id| {
			ChainflipAccountStore::<Runtime>::get(*account_id).state == ChainflipAccountState::Backup
		}).collect();

		BackupEmissions::distribute_rewards(backup_validators);

		// Check the state of the network and if we are below the emergency rotation trigger
		// then issue an emergency rotation request
		if network_state.percentage_online() < EmergencyRotationPercentageTrigger::get() as u32 {
			weight += <Validator as EmergencyRotation>::request_emergency_rotation();
		}

		weight
	}
}