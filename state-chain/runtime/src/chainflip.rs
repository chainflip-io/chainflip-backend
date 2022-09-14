//! Configuration, utilities and helpers for the Chainflip runtime.
mod backup_node_rewards;
pub mod chain_instances;
pub mod decompose_recompose;
pub mod epoch_transition;
mod missed_authorship_slots;
mod offences;
use cf_primitives::Asset;
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
		ingress_address::get_create_2_address,
	},
	ApiCall, ChainAbi, Ethereum, TransactionBuilder,
};
use cf_traits::{
	AddressDerivationApi, BlockEmissions, Chainflip, EmergencyRotation, EpochInfo, Heartbeat,
	Issuance, NetworkState, ReplayProtectionProvider, RewardsDistribution, RuntimeUpgrade,
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

	fn distribute() {
		let backup_nodes =
			Validator::highest_staked_qualified_backup_node_bids().collect::<Vec<_>>();
		if backup_nodes.is_empty() {
			return
		}

		// Distribute rewards one by one
		// N.B. This could be more optimal
		for (validator_id, reward) in calculate_backup_rewards(
			backup_nodes,
			Validator::bond(),
			<<Runtime as pallet_cf_reputation::Config>::HeartbeatBlockInterval as Get<
				BlockNumber,
			>>::get()
			.unique_saturated_into(),
			Emissions::backup_node_emission_per_block(),
			Emissions::current_authority_emission_per_block(),
			Self::Balance::unique_saturated_from(Validator::current_authority_count()),
		) {
			Flip::settle(&validator_id, Self::Issuance::mint(reward).into());
		}
	}
}

pub struct ChainflipHeartbeat;

impl Heartbeat for ChainflipHeartbeat {
	type ValidatorId = AccountId;
	type BlockNumber = BlockNumber;

	fn on_heartbeat_interval(network_state: NetworkState<Self::ValidatorId>) {
		<Emissions as BlockEmissions>::calculate_block_emissions();

		// Reputation depends on heartbeats
		Reputation::penalise_offline_authorities(network_state.offline.clone());

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
				EthereumApi::SetGovKeyWithAggKey(_) => Environment::key_manager_address().into(),
				EthereumApi::SetCommKeyWithAggKey(_) => Environment::key_manager_address().into(),
				EthereumApi::AllBatch(_) => Environment::eth_vault_address().into(),
			},
			data: signed_call.abi_encoded(),
			..Default::default()
		}
	}

	fn refresh_unsigned_transaction(unsigned_tx: &mut <Ethereum as ChainAbi>::UnsignedTransaction) {
		if let Some(chain_state) = ChainState::<Runtime, EthereumInstance>::get() {
			// double the last block's base fee. This way we know it'll be selectable for at least 6
			// blocks (12.5% increase on each block)
			let max_fee_per_gas = chain_state.base_fee * 2 + chain_state.priority_fee;
			unsigned_tx.max_fee_per_gas = Some(U256::from(max_fee_per_gas));
			unsigned_tx.max_priority_fee_per_gas = Some(U256::from(chain_state.priority_fee));
		}
		// if we don't have ChainState, we leave it unmodified
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

pub struct AddressDerivation;

impl AddressDerivationApi for AddressDerivation {
	fn generate_address(
		ingress_asset: cf_primitives::ForeignChainAsset,
		intent_id: cf_primitives::IntentId,
	) -> cf_primitives::ForeignChainAddress {
		match ingress_asset.chain {
			cf_primitives::ForeignChain::Ethereum => {
				let asset_address = match ingress_asset.asset {
					Asset::Eth => vec![],
					_ => Environment::supported_eth_assets(ingress_asset.asset)
						.expect("unsupported asset!")
						.to_vec(),
				};
				cf_primitives::ForeignChainAddress::Eth(get_create_2_address(
					ingress_asset.asset,
					Environment::vault_contract_address(),
					asset_address,
					intent_id,
				))
			},
			cf_primitives::ForeignChain::Polkadot => todo!(),
		}
	}
}
