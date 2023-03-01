//! Configuration, utilities and helpers for the Chainflip runtime.
pub mod address_derivation;
pub mod all_vaults_rotator;
mod backup_node_rewards;
pub mod chain_instances;
pub mod decompose_recompose;
pub mod epoch_transition;
mod missed_authorship_slots;
mod offences;
mod signer_nomination;

use crate::{
	AccountId, Authorship, BlockNumber, EmergencyRotationPercentageRange, Emissions, Environment,
	EthereumBroadcaster, EthereumInstance, Flip, FlipBalance, PolkadotBroadcaster, Reputation,
	Runtime, RuntimeCall, System, Validator,
};
use cf_chains::{
	dot::{
		api::PolkadotApi, Polkadot, PolkadotAccountId, PolkadotReplayProtection,
		PolkadotTransactionData,
	},
	eth::{
		self,
		api::{EthereumApi, EthereumReplayProtection},
		Ethereum,
	},
	AnyChain, ApiCall, Chain, ChainAbi, ChainCrypto, ChainEnvironment, ForeignChain,
	ReplayProtectionProvider, SetCommKeyWithAggKey, SetGovKeyWithAggKey, TransactionBuilder,
};
use cf_primitives::{
	chains::assets, liquidity::U256, Asset, AssetAmount, ForeignChainAddress, IntentId, KeyId,
	ETHEREUM_ETH_ADDRESS,
};
use cf_traits::{
	BlockEmissions, BroadcastAnyChainGovKey, Broadcaster, Chainflip, CommKeyBroadcaster, EgressApi,
	EmergencyRotation, EpochInfo, EpochKey, EthEnvironmentProvider, Heartbeat, IngressApi,
	Issuance, NetworkState, RewardsDistribution, RuntimeUpgrade, VaultTransitionHandler,
};
use codec::{Decode, Encode};
use ethabi::Address as EthAbiAddress;
use frame_support::{
	dispatch::{DispatchError, DispatchErrorWithPostInfo, PostDispatchInfo},
	traits::Get,
};
pub use missed_authorship_slots::MissedAuraSlots;
pub use offences::*;
use pallet_cf_chain_tracking::ChainState;
use pallet_cf_validator::PercentageRange;
use scale_info::TypeInfo;
pub use signer_nomination::RandomSignerNomination;
use sp_runtime::traits::{BlockNumberProvider, UniqueSaturatedFrom, UniqueSaturatedInto};
use sp_std::prelude::*;

use backup_node_rewards::calculate_backup_rewards;

impl Chainflip for Runtime {
	type RuntimeCall = RuntimeCall;
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
	type RuntimeCall = RuntimeCall;

	fn should_waive_fees(call: &Self::RuntimeCall, caller: &Self::AccountId) -> bool {
		if matches!(call, RuntimeCall::Governance(_)) {
			return super::Governance::members().contains(caller)
		}
		false
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
				EthereumApi::UpdateFlipSupply(_) => Environment::token_address(Asset::Flip)
					.expect("FLIP token address should exist")
					.into(),
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

	fn is_valid_for_rebroadcast(_call: &EthereumApi<EthEnvironment>) -> bool {
		// Nothing to validate for Ethereum
		true
	}
}

pub struct DotTransactionBuilder;
impl TransactionBuilder<Polkadot, PolkadotApi<DotEnvironment>> for DotTransactionBuilder {
	fn build_transaction(
		signed_call: &PolkadotApi<DotEnvironment>,
	) -> <Polkadot as ChainAbi>::Transaction {
		PolkadotTransactionData { encoded_extrinsic: signed_call.chain_encoded() }
	}

	fn refresh_unsigned_transaction(_unsigned_tx: &mut <Polkadot as ChainAbi>::Transaction) {
		// TODO: For now this is a noop until we actually have dot chain tracking
	}

	fn is_valid_for_rebroadcast(call: &PolkadotApi<DotEnvironment>) -> bool {
		// If we know the Polkadot runtime has been upgraded then we know this transaction will
		// fail.
		call.runtime_version_used() == Environment::polkadot_runtime_version()
	}
}

pub struct BlockAuthorRewardDistribution;

impl RewardsDistribution for BlockAuthorRewardDistribution {
	type Balance = FlipBalance;
	type Issuance = pallet_cf_flip::FlipIssuance<Runtime>;

	fn distribute() {
		let reward_amount = Emissions::current_authority_emission_per_block();
		if reward_amount != 0 {
			if let Some(current_block_author) = Authorship::author() {
				Flip::settle(&current_block_author, Self::Issuance::mint(reward_amount).into());
			} else {
				log::warn!("No block author for block {}.", System::current_block_number());
			}
		}
	}
}
pub struct RuntimeUpgradeManager;

impl RuntimeUpgrade for RuntimeUpgradeManager {
	fn do_upgrade(code: Vec<u8>) -> Result<PostDispatchInfo, DispatchErrorWithPostInfo> {
		System::set_code(frame_system::RawOrigin::Root.into(), code)
	}
}
pub struct EthEnvironment;

impl ReplayProtectionProvider<Ethereum> for EthEnvironment {
	// Get the Environment values for key_manager_address and chain_id, then use
	// the next global signature nonce
	fn replay_protection() -> EthereumReplayProtection {
		EthereumReplayProtection {
			key_manager_address: Environment::key_manager_address(),
			chain_id: Environment::ethereum_chain_id(),
			nonce: Environment::next_ethereum_signature_nonce(),
		}
	}
}

impl ChainEnvironment<assets::eth::Asset, EthAbiAddress> for EthEnvironment {
	fn lookup(asset: assets::eth::Asset) -> Option<EthAbiAddress> {
		Some(match asset {
			assets::eth::Asset::Eth => ETHEREUM_ETH_ADDRESS.into(),
			erc20 => Environment::token_address(erc20.into())?.into(),
		})
	}
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct DotEnvironment;

impl ReplayProtectionProvider<Polkadot> for DotEnvironment {
	// Get the Environment values for vault_account, NetworkChoice and the next nonce for the
	// proxy_account
	fn replay_protection() -> PolkadotReplayProtection {
		PolkadotReplayProtection::new(
			Environment::next_polkadot_proxy_account_nonce(),
			// TODO: Instead of 0, tip needs to be set here
			0,
			Environment::polkadot_runtime_version(),
			<Runtime as pallet_cf_environment::Config>::PolkadotGenesisHash::get(),
		)
	}
}

impl ChainEnvironment<cf_chains::dot::api::SystemAccounts, PolkadotAccountId> for DotEnvironment {
	fn lookup(query: cf_chains::dot::api::SystemAccounts) -> Option<PolkadotAccountId> {
		use crate::PolkadotVault;
		use cf_traits::{KeyProvider, KeyState};
		use sp_runtime::{traits::IdentifyAccount, MultiSigner};
		match query {
			cf_chains::dot::api::SystemAccounts::Proxy => {
				match <PolkadotVault as KeyProvider<Polkadot>>::current_epoch_key() {
					EpochKey { key, key_state, .. } if key_state == KeyState::Active =>
						Some(MultiSigner::Sr25519(key.0).into_account()),
					_ => None,
				}
			},

			cf_chains::dot::api::SystemAccounts::Vault => Environment::get_polkadot_vault_account(),
		}
	}
}

pub struct EthVaultTransitionHandler;
impl VaultTransitionHandler<Ethereum> for EthVaultTransitionHandler {}

pub struct DotVaultTransitionHandler;
impl VaultTransitionHandler<Polkadot> for DotVaultTransitionHandler {
	fn on_new_vault() {
		Environment::reset_polkadot_proxy_account_nonce();
	}
}

pub struct AnyChainIngressEgressHandler;

impl EgressApi<AnyChain> for AnyChainIngressEgressHandler {
	fn schedule_egress(
		asset: Asset,
		amount: AssetAmount,
		egress_address: <AnyChain as Chain>::ChainAccount,
	) -> cf_primitives::EgressId {
		match asset.into() {
			ForeignChain::Ethereum => crate::EthereumIngressEgress::schedule_egress(
				asset.try_into().expect("Checked for asset compatibility"),
				amount,
				egress_address
					.try_into()
					.expect("Caller must ensure for account is of the compatible type."),
			),
			ForeignChain::Polkadot => crate::PolkadotIngressEgress::schedule_egress(
				asset.try_into().expect("Checked for asset compatibility"),
				amount,
				egress_address
					.try_into()
					.expect("Caller must ensure for account is of the compatible type."),
			),
			ForeignChain::Bitcoin => todo!("Bitcoin egress"),
		}
	}
}

pub struct TokenholderGovernanceBroadcaster;

impl TokenholderGovernanceBroadcaster {
	fn broadcast_gov_key<C, B>(maybe_old_key: Option<Vec<u8>>, new_key: Vec<u8>) -> Result<(), ()>
	where
		C: ChainAbi,
		B: Broadcaster<C>,
		<B as Broadcaster<C>>::ApiCall: cf_chains::SetGovKeyWithAggKey<C>,
	{
		let maybe_old_key = if let Some(old_key) = maybe_old_key {
			Some(Decode::decode(&mut &old_key[..]).or(Err(()))?)
		} else {
			None
		};
		let api_call = SetGovKeyWithAggKey::<C>::new_unsigned(
			maybe_old_key,
			Decode::decode(&mut &new_key[..]).or(Err(()))?,
		)?;
		B::threshold_sign_and_broadcast(api_call);
		Ok(())
	}

	fn is_govkey_compatible<C: ChainCrypto>(key: &[u8]) -> bool {
		C::GovKey::decode(&mut &key[..]).is_ok()
	}
}

impl BroadcastAnyChainGovKey for TokenholderGovernanceBroadcaster {
	fn broadcast_gov_key(
		chain: ForeignChain,
		maybe_old_key: Option<Vec<u8>>,
		new_key: Vec<u8>,
	) -> Result<(), ()> {
		match chain {
			ForeignChain::Ethereum =>
				Self::broadcast_gov_key::<Ethereum, EthereumBroadcaster>(maybe_old_key, new_key),
			ForeignChain::Polkadot =>
				Self::broadcast_gov_key::<Polkadot, PolkadotBroadcaster>(maybe_old_key, new_key),
			ForeignChain::Bitcoin => todo!("Bitcoin govkey broadcast"),
		}
	}

	fn is_govkey_compatible(chain: ForeignChain, key: &[u8]) -> bool {
		match chain {
			ForeignChain::Ethereum => Self::is_govkey_compatible::<Ethereum>(key),
			ForeignChain::Polkadot => Self::is_govkey_compatible::<Polkadot>(key),
			ForeignChain::Bitcoin => todo!("Bitcoin govkey compatibility"),
		}
	}
}

impl CommKeyBroadcaster for TokenholderGovernanceBroadcaster {
	fn broadcast(new_key: <Ethereum as ChainCrypto>::GovKey) {
		EthereumBroadcaster::threshold_sign_and_broadcast(
			SetCommKeyWithAggKey::<Ethereum>::new_unsigned(new_key),
		);
	}
}

impl IngressApi<AnyChain> for AnyChainIngressEgressHandler {
	type AccountId = <Runtime as frame_system::Config>::AccountId;

	fn register_liquidity_ingress_intent(
		lp_account: Self::AccountId,
		ingress_asset: Asset,
	) -> Result<(IntentId, ForeignChainAddress), DispatchError> {
		match ingress_asset.into() {
			ForeignChain::Ethereum =>
				crate::EthereumIngressEgress::register_liquidity_ingress_intent(
					lp_account,
					ingress_asset.try_into().unwrap(),
				),
			ForeignChain::Polkadot =>
				crate::PolkadotIngressEgress::register_liquidity_ingress_intent(
					lp_account,
					ingress_asset.try_into().unwrap(),
				),
			ForeignChain::Bitcoin => todo!("Cannot register liquidity ingress intent for Bitcoin"),
		}
	}

	fn register_swap_intent(
		ingress_asset: Asset,
		egress_asset: Asset,
		egress_address: ForeignChainAddress,
		relayer_commission_bps: u16,
		relayer_id: Self::AccountId,
	) -> Result<(IntentId, ForeignChainAddress), DispatchError> {
		match ingress_asset.into() {
			ForeignChain::Ethereum => crate::EthereumIngressEgress::register_swap_intent(
				ingress_asset.try_into().unwrap(),
				egress_asset,
				egress_address,
				relayer_commission_bps,
				relayer_id,
			),
			ForeignChain::Polkadot => crate::PolkadotIngressEgress::register_swap_intent(
				ingress_asset.try_into().unwrap(),
				egress_asset,
				egress_address,
				relayer_commission_bps,
				relayer_id,
			),
			ForeignChain::Bitcoin => todo!("Cannot register swap intent for Bitcoin"),
		}
	}
}
