//! Configuration, utilities and helpers for the Chainflip runtime.
mod backup_node_rewards;
pub mod chain_instances;
pub mod epoch_transition;
mod missed_authorship_slots;
mod offences;
pub use offences::*;
mod signer_nomination;
pub use missed_authorship_slots::MissedAuraSlots;
pub use signer_nomination::RandomSignerNomination;
use sp_core::U256;

use crate::{
	AccountId, Authorship, BlockNumber, Call, EmergencyRotationPercentageRange, Emissions,
	Environment, EthereumInstance, Flip, FlipBalance, Reputation, Runtime, System, Validator,
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
use frame_support::traits::Get;
use pallet_cf_chain_tracking::ChainState;

use frame_support::{dispatch::DispatchErrorWithPostInfo, weights::PostDispatchInfo};

use pallet_cf_validator::PercentageRange;
use sp_runtime::traits::{UniqueSaturatedFrom, UniqueSaturatedInto};
use sp_std::prelude::*;

use backup_node_rewards::calculate_backup_rewards;

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

struct BackupNodeEmissions;

impl RewardsDistribution for BackupNodeEmissions {
	type Balance = FlipBalance;
	type Issuance = pallet_cf_flip::FlipIssuance<Runtime>;

	// This is called on each heartbeat interval
	fn distribute() {
		let backup_nodes = Validator::backup_nodes();
		if backup_nodes.is_empty() {
			return
		}

		let backup_nodes: Vec<_> = backup_nodes
			.iter()
			.map(|backup_node| (backup_node.clone(), Flip::staked_balance(backup_node)))
			.collect();

		// The current minimum active bid
		let minimum_active_bid = Validator::bond();
		let heartbeat_block_interval: FlipBalance =
			<<Runtime as pallet_cf_reputation::Config>::HeartbeatBlockInterval as Get<
				BlockNumber,
			>>::get()
			.unique_saturated_into();
		let backup_node_emission_per_block = Emissions::backup_node_emission_per_block();
		let current_authority_emission_per_block =
			Emissions::current_authority_emission_per_block();
		let current_authority_count =
			Self::Balance::unique_saturated_from(Validator::current_authority_count());

		let rewards = calculate_backup_rewards(
			backup_nodes,
			minimum_active_bid,
			heartbeat_block_interval,
			backup_node_emission_per_block,
			current_authority_emission_per_block,
			current_authority_count,
		);

		// Distribute rewards one by one
		// N.B. This could be more optimal
		for (validator_id, reward) in rewards {
			Flip::settle(&validator_id, Self::Issuance::mint(reward).into());
		}
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

		BackupNodeEmissions::distribute();

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
		eth::UnsignedTransaction {
			chain_id: Environment::ethereum_chain_id(),
			contract: match signed_call {
				EthereumApi::SetAggKeyWithAggKey(_) => Environment::key_manager_address().into(),
				EthereumApi::RegisterClaim(_) => Environment::stake_manager_address().into(),
				EthereumApi::UpdateFlipSupply(_) => Environment::flip_token_address().into(),
			},
			data: signed_call.abi_encoded(),
			..Default::default()
		}
	}

	fn refresh_unsigned_transaction(
		mut unsigned_tx: <Ethereum as ChainAbi>::UnsignedTransaction,
	) -> Option<<Ethereum as ChainAbi>::UnsignedTransaction> {
		let chain_data = ChainState::<Runtime, EthereumInstance>::get()?;

		// double the last block's base fee. This way we know it'll be selectable for at least 6
		// blocks (12.5% increase on each block)
		let max_fee_per_gas = chain_data.base_fee * 2 + chain_data.priority_fee;
		unsigned_tx.max_fee_per_gas = Some(U256::from(max_fee_per_gas));
		unsigned_tx.max_priority_fee_per_gas = Some(U256::from(chain_data.priority_fee));
		Some(unsigned_tx)
	}
}

pub struct BlockAuthorRewardDistribution;

impl RewardsDistribution for BlockAuthorRewardDistribution {
	type Balance = FlipBalance;
	type Issuance = pallet_cf_flip::FlipIssuance<Runtime>;

	fn distribute() {
		let reward_amount = Emissions::current_authority_emission_per_block();
		if reward_amount != 0 {
			// TODO: Check if it's ok to panic here.
			let current_block_author =
				Authorship::author().expect("A block without an author is invalid.");
			Flip::settle_imbalance(&current_block_author, Self::Issuance::mint(reward_amount));
		}
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
