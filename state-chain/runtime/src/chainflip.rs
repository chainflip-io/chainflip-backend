//! Configuration, utilities and helpers for the Chainflip runtime.
pub mod chain_instances;
pub mod epoch_transition;
mod missed_authorship_slots;
mod offences;
pub use offences::*;
mod signer_nomination;
pub use missed_authorship_slots::MissedAuraSlots;
use pallet_cf_flip::Surplus;
pub use signer_nomination::RandomSignerNomination;

use crate::{
	AccountId, Authorship, BlockNumber, Call, EmergencyRotationPercentageRange, Emissions,
	Environment, Flip, FlipBalance, Reputation, Runtime, System, Validator,
};
use cf_chains::{
	eth::{
		self,
		api::{EthereumApi, EthereumReplayProtection},
	},
	ApiCall, ChainAbi, Ethereum, TransactionBuilder,
};
use cf_traits::{
	BackupNodes, Chainflip, EmergencyRotation, EpochInfo, Heartbeat, Issuance, NetworkState,
	ReplayProtectionProvider, RewardsDistribution, RuntimeUpgrade, StakeTransfer,
};
use frame_support::{traits::Get, weights::Weight};

use frame_support::{dispatch::DispatchErrorWithPostInfo, weights::PostDispatchInfo};

use pallet_cf_validator::PercentageRange;
use sp_runtime::{
	helpers_128bit::multiply_by_rational,
	traits::{AtLeast32BitUnsigned, UniqueSaturatedFrom, UniqueSaturatedInto},
};
use sp_std::{cmp::min, prelude::*};

impl Chainflip for Runtime {
	type Call = Call;
	type Amount = FlipBalance;
	type ValidatorId = <Self as frame_system::Config>::AccountId;
	type KeyId = Vec<u8>;
	type EnsureWitnessed = pallet_cf_witnesser::EnsureWitnessed;
	type EnsureWitnessedAtCurrentEpoch = pallet_cf_witnesser::EnsureWitnessedAtCurrentEpoch;
	type EpochInfo = Validator;
	type SystemState = pallet_cf_environment::SystemStateProvider<Runtime>;
}

trait RewardDistribution {
	type FlipBalance: UniqueSaturatedFrom<BlockNumber> + AtLeast32BitUnsigned;
	/// An implementation of the [Issuance] trait.
	type Issuance: Issuance;

	/// Distribute rewards
	fn distribute_rewards(backup_nodes: &[AccountId]) -> Weight;
}

struct BackupNodeEmissions;

impl RewardDistribution for BackupNodeEmissions {
	type FlipBalance = FlipBalance;
	type Issuance = pallet_cf_flip::FlipIssuance<Runtime>;

	// This is called on each heartbeat interval
	fn distribute_rewards(backup_nodes: &[AccountId]) -> Weight {
		if backup_nodes.is_empty() {
			return 0
		}
		// The current minimum active bid
		let minimum_active_bid = Validator::bond();
		let heartbeat_block_interval: FlipBalance =
			<<Runtime as pallet_cf_reputation::Config>::HeartbeatBlockInterval as Get<
				BlockNumber,
			>>::get()
			.unique_saturated_into();
		// Our emission cap for this heartbeat interval
		let emissions_cap =
			Emissions::backup_node_emission_per_block().saturating_mul(heartbeat_block_interval);

		// Emissions for this heartbeat interval for the active set
		let authority_rewards = Emissions::current_authority_emission_per_block()
			.saturating_mul(heartbeat_block_interval);

		// The average authority emission
		let average_authority_reward: Self::FlipBalance = authority_rewards /
			Self::FlipBalance::unique_saturated_from(Validator::current_authority_count());

		let mut total_rewards = 0;

		// Calculate rewards for each backup node and total rewards for capping
		let mut rewards: Vec<(AccountId, Self::FlipBalance)> = backup_nodes
			.iter()
			.map(|backup_node| {
				let backup_node_stake = Flip::stakeable_balance(backup_node);
				let reward_scaling_factor = min(1, (backup_node_stake / minimum_active_bid) ^ 2);
				let reward = (reward_scaling_factor * average_authority_reward * 8) / 10;
				total_rewards += reward;
				(backup_node.clone(), reward)
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

		let backup_nodes = <Validator as BackupNodes>::backup_nodes();
		BackupNodeEmissions::distribute_rewards(&backup_nodes);

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
				contract: Environment::flip_token_address().into(),
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
		// TODO: Check if it's ok to panic here.
		let current_block_author =
			Authorship::author().expect("A block without an author is invalid.");
		Flip::settle_imbalance(&current_block_author, rewards);
	}
}
pub struct RuntimeUpgradeManager;

impl RuntimeUpgrade for RuntimeUpgradeManager {
	fn do_upgrade(code: Vec<u8>) -> Result<PostDispatchInfo, DispatchErrorWithPostInfo> {
		System::set_code(frame_system::RawOrigin::Root.into(), code)
	}
}

pub struct EthReplayProtectionProvider;

impl ReplayProtectionProvider<Ethereum> for EthReplayProtectionProvider {
	// Get the Environment values for key_manager_address and chain_id, then use
	// the next global signature nonce
	fn replay_protection() -> EthereumReplayProtection {
		EthereumReplayProtection {
			key_manager_address: Environment::key_manager_address(),
			chain_id: Environment::ethereum_chain_id(),
			nonce: Environment::next_global_signature_nonce(),
		}
	}
}
