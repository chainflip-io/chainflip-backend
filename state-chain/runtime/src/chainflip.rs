//! Configuration, utilities and helpers for the Chainflip runtime.
pub mod address_derivation;
pub mod all_vaults_rotator;
mod backup_node_rewards;
pub mod chain_instances;
pub mod decompose_recompose;
pub mod epoch_transition;
mod missed_authorship_slots;
mod offences;
use cf_primitives::{chains::assets, KeyId, ETHEREUM_ETH_ADDRESS};
pub use offences::*;
mod signer_nomination;
use ethabi::Address as EthAbiAddress;
pub use missed_authorship_slots::MissedAuraSlots;
pub use signer_nomination::RandomSignerNomination;
use sp_core::U256;

use crate::{
	AccountId, Authorship, BlockNumber, Call, EmergencyRotationPercentageRange, Emissions,
	Environment, EthereumInstance, Flip, FlipBalance, Reputation, Runtime, System, Validator,
};
#[cfg(feature = "ibiza")]
use cf_chains::{
	dot::{api::PolkadotApi, Polkadot, PolkadotReplayProtection, PolkadotTransactionData},
	ChainCrypto,
};
#[cfg(feature = "ibiza")]
use codec::{Decode, Encode};
#[cfg(feature = "ibiza")]
use scale_info::TypeInfo;

use cf_chains::{
	eth::{
		self,
		api::{EthereumApi, EthereumReplayProtection},
		Ethereum,
	},
	ApiCall, ChainAbi, ChainEnvironment, ReplayProtectionProvider, TransactionBuilder,
};
use cf_traits::{
	BlockEmissions, Chainflip, EmergencyRotation, EpochInfo, EthEnvironmentProvider, Heartbeat,
	Issuance, NetworkState, RewardsDistribution, RuntimeUpgrade, VaultTransitionHandler,
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
	type KeyId = KeyId;
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

pub struct EthEnvironment;

impl ChainEnvironment<assets::eth::Asset, EthAbiAddress> for EthEnvironment {
	fn lookup(
		asset: assets::eth::Asset,
	) -> Result<EthAbiAddress, frame_support::error::LookupError> {
		Ok(match asset {
			assets::eth::Asset::Eth => ETHEREUM_ETH_ADDRESS.into(),
			assets::eth::Asset::Flip => Environment::flip_token_address().into(),
			assets::eth::Asset::Usdc => todo!(),
		})
	}
}

pub struct EthTransactionBuilder;

impl TransactionBuilder<Ethereum, EthereumApi<EthEnvironment>> for EthTransactionBuilder {
	fn build_transaction(
		signed_call: &EthereumApi<EthEnvironment>,
	) -> <Ethereum as ChainAbi>::Transaction {
		eth::Transaction {
			chain_id: Environment::ethereum_chain_id(),
			contract: match signed_call {
				EthereumApi::SetAggKeyWithAggKey(_) => Environment::key_manager_address().into(),
				EthereumApi::RegisterClaim(_) => Environment::stake_manager_address().into(),
				EthereumApi::UpdateFlipSupply(_) => Environment::flip_token_address().into(),
				EthereumApi::SetGovKeyWithAggKey(_) => Environment::key_manager_address().into(),
				EthereumApi::SetCommKeyWithAggKey(_) => Environment::key_manager_address().into(),
				EthereumApi::AllBatch(_) => Environment::eth_vault_address().into(),
				EthereumApi::_Phantom(..) => unreachable!(),
			},
			data: signed_call.chain_encoded(),
			..Default::default()
		}
	}

	fn refresh_unsigned_transaction(unsigned_tx: &mut <Ethereum as ChainAbi>::Transaction) {
		if let Some(chain_state) = ChainState::<Runtime, EthereumInstance>::get() {
			// double the last block's base fee. This way we know it'll be selectable for at least 6
			// blocks (12.5% increase on each block)
			let max_fee_per_gas =
				chain_state.base_fee.saturating_mul(2).saturating_add(chain_state.priority_fee);
			unsigned_tx.max_fee_per_gas = Some(U256::from(max_fee_per_gas));
			unsigned_tx.max_priority_fee_per_gas = Some(U256::from(chain_state.priority_fee));
		}
		// if we don't have ChainState, we leave it unmodified
	}
}

#[cfg(feature = "ibiza")]
pub struct DotTransactionBuilder;
#[cfg(feature = "ibiza")]
impl TransactionBuilder<Polkadot, PolkadotApi<DotEnvironment>> for DotTransactionBuilder {
	fn build_transaction(
		signed_call: &PolkadotApi<DotEnvironment>,
	) -> <Polkadot as ChainAbi>::Transaction {
		PolkadotTransactionData { encoded_extrinsic: signed_call.chain_encoded() }
	}

	fn refresh_unsigned_transaction(_unsigned_tx: &mut <Polkadot as ChainAbi>::Transaction) {
		todo!();
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

impl ReplayProtectionProvider<Ethereum> for EthEnvironment {
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

#[cfg(feature = "ibiza")]
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct DotEnvironment;

#[cfg(feature = "ibiza")]
impl ReplayProtectionProvider<Polkadot> for DotEnvironment {
	// Get the Environment values for vault_account, NetworkChoice and the next nonce for the
	// proxy_account
	fn replay_protection() -> PolkadotReplayProtection {
		PolkadotReplayProtection::new(
			Environment::next_polkadot_proxy_account_nonce(),
			0,
			Environment::get_polkadot_network_config(),
		) //Todo: Instead
		 // of 0, tip needs
		 // to be set here
	}
}

#[cfg(feature = "ibiza")]
impl ChainEnvironment<cf_chains::dot::api::SystemAccounts, AccountId> for DotEnvironment {
	fn lookup(
		_query: cf_chains::dot::api::SystemAccounts,
	) -> Result<AccountId, frame_support::error::LookupError> {
		todo!() //Pull from environment
	}
}

pub struct EthVaultTransitionHandler;
impl VaultTransitionHandler<Ethereum> for EthVaultTransitionHandler {}

#[cfg(feature = "ibiza")]
pub struct DotVaultTransitionHandler;
#[cfg(feature = "ibiza")]
impl VaultTransitionHandler<Polkadot> for DotVaultTransitionHandler {
	fn on_new_vault(new_key: <Polkadot as ChainCrypto>::AggKey) {
		Environment::set_new_proxy_account(new_key);
	}
}
