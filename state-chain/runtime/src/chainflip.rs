//! Configuration, utilities and helpers for the Chainflip runtime.
pub mod chain_instances;
mod missed_authorship_slots;
mod signer_nomination;
pub use missed_authorship_slots::MissedAuraSlots;
use pallet_cf_flip::Surplus;
pub use signer_nomination::RandomSignerNomination;

use crate::{
	AccountId, Auction, Authorship, BlockNumber, Call, EmergencyRotationPercentageRange, Emissions,
	Environment, Flip, FlipBalance, HeartbeatBlockInterval, Reputation, Runtime, System, Validator,
	Witnesser,
};
use cf_chains::{
	eth::{self, api::EthereumApi},
	ApiCall, ChainAbi, Ethereum, TransactionBuilder,
};
use cf_traits::{
	offence_reporting::{Offence, ReputationPoints},
	BackupValidators, BlockEmissions, BondRotation, Chainflip, ChainflipAccount,
	ChainflipAccountStore, EmergencyRotation, EmissionsTrigger, EpochInfo, EpochTransitionHandler,
	Heartbeat, Issuance, NetworkState, RewardsDistribution, StakeHandler, StakeTransfer,
};
use frame_support::weights::Weight;

use frame_support::{dispatch::DispatchErrorWithPostInfo, weights::PostDispatchInfo};

use pallet_cf_auction::HandleStakes;

use pallet_cf_validator::PercentageRange;
use sp_runtime::{
	helpers_128bit::multiply_by_rational,
	traits::{AtLeast32BitUnsigned, UniqueSaturatedFrom},
};
use sp_std::{cmp::min, marker::PhantomData, prelude::*};

use cf_traits::RuntimeUpgrade;

impl Chainflip for Runtime {
	type Call = Call;
	type Amount = FlipBalance;
	type ValidatorId = <Self as frame_system::Config>::AccountId;
	type KeyId = Vec<u8>;
	type EnsureWitnessed = pallet_cf_witnesser::EnsureWitnessed;
	type EpochInfo = Validator;
}

pub struct ChainflipEpochTransitions;

/// Trigger emissions on epoch transitions.
impl EpochTransitionHandler for ChainflipEpochTransitions {
	type ValidatorId = AccountId;
	type Amount = FlipBalance;

	fn on_new_epoch(
		old_validators: &[Self::ValidatorId],
		new_validators: &[Self::ValidatorId],
		new_bond: Self::Amount,
	) {
		// Calculate block emissions on every epoch
		<Emissions as BlockEmissions>::calculate_block_emissions();
		// Process any outstanding emissions.
		<Emissions as EmissionsTrigger>::trigger_emissions();
		// Update the bond of all validators for the new epoch
		<Flip as BondRotation>::update_validator_bonds(new_validators, new_bond);
		// Update the list of validators in the witnesser.
		<Witnesser as EpochTransitionHandler>::on_new_epoch(
			old_validators,
			new_validators,
			new_bond,
		);

		<AccountStateManager<Runtime> as EpochTransitionHandler>::on_new_epoch(
			old_validators,
			new_validators,
			new_bond,
		);

		<pallet_cf_online::Pallet<Runtime> as cf_traits::KeygenExclusionSet>::forgive_all();
	}
}

pub struct AccountStateManager<T>(PhantomData<T>);

impl<T: Chainflip> EpochTransitionHandler for AccountStateManager<T> {
	type ValidatorId = AccountId;
	type Amount = T::Amount;

	fn on_new_epoch(
		_old_validators: &[Self::ValidatorId],
		new_validators: &[Self::ValidatorId],
		_new_bid: Self::Amount,
	) {
		// Update the last active epoch for the new validating set
		let epoch_index = Validator::epoch_index();
		for validator in new_validators {
			ChainflipAccountStore::<Runtime>::update_last_active_epoch(validator, epoch_index);
		}
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

trait RewardDistribution {
	type EpochInfo: EpochInfo;
	type StakeTransfer: StakeTransfer;
	type ValidatorId;
	type BlockNumber;
	type FlipBalance: UniqueSaturatedFrom<Self::BlockNumber> + AtLeast32BitUnsigned;
	/// An implementation of the [Issuance] trait.
	type Issuance: Issuance;

	/// Distribute rewards
	fn distribute_rewards(backup_validators: &[Self::ValidatorId]) -> Weight;
}

struct BackupValidatorEmissions;

impl RewardDistribution for BackupValidatorEmissions {
	type EpochInfo = Validator;
	type StakeTransfer = Flip;
	type ValidatorId = AccountId;
	type BlockNumber = BlockNumber;
	type FlipBalance = FlipBalance;
	type Issuance = pallet_cf_flip::FlipIssuance<Runtime>;

	// This is called on each heartbeat interval
	fn distribute_rewards(backup_validators: &[Self::ValidatorId]) -> Weight {
		if backup_validators.is_empty() {
			return 0
		}
		// The current minimum active bid
		let minimum_active_bid = Self::EpochInfo::bond();
		// Our emission cap for this heartbeat interval
		let emissions_cap = Emissions::backup_validator_emission_per_block() *
			Self::FlipBalance::unique_saturated_from(HeartbeatBlockInterval::get());

		// Emissions for this heartbeat interval for the active set
		let validator_rewards = Emissions::validator_emission_per_block() *
			Self::FlipBalance::unique_saturated_from(HeartbeatBlockInterval::get());

		// The average validator emission
		let average_validator_reward: Self::FlipBalance = validator_rewards /
			Self::FlipBalance::unique_saturated_from(Self::EpochInfo::current_validators().len());

		let mut total_rewards = 0;

		// Calculate rewards for each backup validator and total rewards for capping
		let mut rewards: Vec<(Self::ValidatorId, Self::FlipBalance)> = backup_validators
			.iter()
			.map(|backup_validator| {
				let backup_validator_stake =
					Self::StakeTransfer::stakeable_balance(backup_validator);
				let reward_scaling_factor =
					min(1, (backup_validator_stake / minimum_active_bid) ^ 2);
				let reward = (reward_scaling_factor * average_validator_reward * 8) / 10;
				total_rewards += reward;
				(backup_validator.clone(), reward)
			})
			.collect();

		// Cap if needed
		if total_rewards > emissions_cap {
			rewards = rewards
				.into_iter()
				.map(|(validator_id, reward)| {
					(
						validator_id,
						multiply_by_rational(reward, emissions_cap, total_rewards)
							.unwrap_or_default(),
					)
				})
				.collect();
		}

		// Distribute rewards one by one
		// N.B. This could be more optimal
		for (validator_id, reward) in rewards {
			Flip::settle(&validator_id, Self::Issuance::mint(reward).into());
		}

		0
	}
}

pub struct ChainflipHeartbeat;

impl Heartbeat for ChainflipHeartbeat {
	type ValidatorId = AccountId;
	type BlockNumber = BlockNumber;

	fn heartbeat_submitted(validator_id: &Self::ValidatorId, block_number: Self::BlockNumber) {
		<Reputation as Heartbeat>::heartbeat_submitted(validator_id, block_number);
	}

	fn on_heartbeat_interval(network_state: NetworkState<Self::ValidatorId>) {
		// Reputation depends on heartbeats
		<Reputation as Heartbeat>::on_heartbeat_interval(network_state.clone());

		let backup_validators = <Auction as BackupValidators>::backup_validators();
		BackupValidatorEmissions::distribute_rewards(&backup_validators);

		// Check the state of the network and if we are within the emergency rotation range
		// then issue an emergency rotation request
		let PercentageRange { top, bottom } = EmergencyRotationPercentageRange::get();
		let percent_online = network_state.percentage_online() as u8;
		if percent_online >= bottom && percent_online <= top {
			<Validator as EmergencyRotation>::request_emergency_rotation();
		}
	}
}

/// Checks if the caller can execute free transactions
pub struct WaivedFees;

impl cf_traits::WaivedFees for WaivedFees {
	type AccountId = AccountId;
	type Call = Call;

	fn should_waive_fees(call: &Self::Call, caller: &Self::AccountId) -> bool {
		if matches!(call, Call::Governance(_)) {
			return super::Governance::members().contains(caller)
		}
		false
	}
}

pub struct OffencePenalty;

impl cf_traits::offence_reporting::OffencePenalty for OffencePenalty {
	fn penalty(condition: &Offence) -> (ReputationPoints, bool) {
		match condition {
			Offence::ParticipateSigningFailed => (15, true),
			Offence::ParticipateKeygenFailed => (15, true),
			Offence::InvalidTransactionAuthored => (15, false),
			Offence::TransactionFailedOnTransmission => (15, false),
			Offence::MissedAuthorshipSlot => (15, true),
		}
	}
}

pub struct EthTransactionBuilder;

impl TransactionBuilder<Ethereum, EthereumApi> for EthTransactionBuilder {
	fn build_transaction(signed_call: &EthereumApi) -> <Ethereum as ChainAbi>::UnsignedTransaction {
		let data = signed_call.encoded();
		match signed_call {
			EthereumApi::SetAggKeyWithAggKey(_) => eth::UnsignedTransaction {
				chain_id: Environment::ethereum_chain_id(),
				contract: Environment::key_manager_address().into(),
				data,
				..Default::default()
			},
			EthereumApi::RegisterClaim(_) => eth::UnsignedTransaction {
				chain_id: Environment::ethereum_chain_id(),
				contract: Environment::stake_manager_address().into(),
				data,
				..Default::default()
			},
			EthereumApi::UpdateFlipSupply(_) => eth::UnsignedTransaction {
				chain_id: Environment::ethereum_chain_id(),
				contract: Environment::stake_manager_address().into(),
				data,
				..Default::default()
			},
		}
	}
}

pub struct BlockAuthorRewardDistribution;

impl RewardsDistribution for BlockAuthorRewardDistribution {
	type Balance = FlipBalance;
	type Surplus = Surplus<Runtime>;

	fn distribute(rewards: Self::Surplus) {
		let current_block_author = Authorship::author();
		Flip::settle_imbalance(&current_block_author, rewards);
	}
}
pub struct RuntimeUpgradeManager;

impl RuntimeUpgrade for RuntimeUpgradeManager {
	fn do_upgrade(code: Vec<u8>) -> Result<PostDispatchInfo, DispatchErrorWithPostInfo> {
		System::set_code(frame_system::RawOrigin::Root.into(), code)
	}
}
