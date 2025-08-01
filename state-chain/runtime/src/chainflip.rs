// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

//! Configuration, utilities and helpers for the Chainflip runtime.
pub mod address_derivation;
pub mod backup_node_rewards;
pub mod cons_key_rotator;
pub mod decompose_recompose;
pub mod epoch_transition;
mod missed_authorship_slots;
pub mod multi_vault_activator;
mod offences;
pub mod pending_rotation_broadcasts;
mod signer_nomination;

// Election pallet implementations
pub mod bitcoin_block_processor;
#[macro_use]
pub mod elections;
pub mod bitcoin_elections;
pub mod generic_elections;
pub mod solana_elections;
pub mod vault_swaps;

use crate::{
	chainflip::{
		elections::TypesFor,
		generic_elections::{decode_and_get_latest_oracle_price, Chainlink, OraclePrice},
	},
	impl_transaction_builder_for_evm_chain, AccountId, AccountRoles, ArbitrumChainTracking,
	ArbitrumIngressEgress, AssethubBroadcaster, AssethubChainTracking, AssethubIngressEgress,
	Authorship, BitcoinChainTracking, BitcoinIngressEgress, BitcoinThresholdSigner, BlockNumber,
	Emissions, Environment, EthereumBroadcaster, EthereumChainTracking, EthereumIngressEgress,
	Flip, FlipBalance, Hash, PolkadotBroadcaster, PolkadotChainTracking, PolkadotIngressEgress,
	PolkadotThresholdSigner, Runtime, RuntimeCall, SolanaBroadcaster, SolanaChainTrackingProvider,
	SolanaIngressEgress, SolanaThresholdSigner, System, Validator, YEAR,
};
use backup_node_rewards::calculate_backup_rewards;
use cf_chains::{
	address::{
		decode_and_validate_address_for_asset, to_encoded_address, try_from_encoded_address,
		AddressConverter, AddressError, EncodedAddress, ForeignChainAddress,
	},
	arb::api::ArbitrumApi,
	assets::any::ForeignChainAndAsset,
	btc::{
		api::{BitcoinApi, SelectedUtxosAndChangeAmount, UtxoSelectionType},
		Bitcoin, BitcoinCrypto, BitcoinFeeInfo, BitcoinTransactionData, ScriptPubkey, Utxo, UtxoId,
	},
	ccm_checker::DecodedCcmAdditionalData,
	dot::{
		api::PolkadotApi, Polkadot, PolkadotAccountId, PolkadotCrypto, PolkadotReplayProtection,
		PolkadotTransactionData, ResetProxyAccountNonce, RuntimeVersion,
	},
	eth::{
		self,
		api::{EthereumApi, StateChainGatewayAddressProvider},
		deposit_address::ETHEREUM_ETH_ADDRESS,
		Ethereum,
	},
	evm::{
		api::{EvmChainId, EvmEnvironmentProvider, EvmReplayProtection},
		EvmCrypto, Transaction,
	},
	hub::{api::AssethubApi, OutputAccountId},
	instances::{
		ArbitrumInstance, AssethubInstance, BitcoinInstance, EthereumInstance, PolkadotInstance,
		SolanaInstance,
	},
	sol::{
		api::{
			AllNonceAccounts, AltWitnessingConsensusResult, ApiEnvironment, ComputePrice,
			CurrentAggKey, CurrentOnChainKey, DurableNonce, DurableNonceAndAccount,
			RecoverDurableNonce, SolanaApi, SolanaEnvironment, SolanaTransactionType,
		},
		SolAddress, SolAddressLookupTableAccount, SolAmount, SolApiEnvironment, SolanaCrypto,
		SolanaTransactionData, NONCE_AVAILABILITY_THRESHOLD_FOR_INITIATING_TRANSFER,
	},
	AnyChain, ApiCall, Arbitrum, Assethub, CcmChannelMetadataChecked, CcmDepositMetadataChecked,
	Chain, ChainCrypto, ChainEnvironment, ChainState, ChannelRefundParametersForChain,
	ForeignChain, ReplayProtectionProvider, RequiresSignatureRefresh, SetCommKeyWithAggKey,
	SetGovKeyWithAggKey, SetGovKeyWithAggKeyError, Solana, TransactionBuilder,
};
use cf_primitives::{
	chains::assets, AccountRole, Asset, AssetAmount, BasisPoints, Beneficiaries, ChannelId,
	DcaParameters,
};
use cf_traits::{
	AccountInfo, AccountRoleRegistry, BackupRewardsNotifier, BlockEmissions,
	BroadcastAnyChainGovKey, Broadcaster, CcmAdditionalDataHandler, Chainflip, CommKeyBroadcaster,
	DepositApi, EgressApi, EpochInfo, FetchesTransfersLimitProvider, Heartbeat,
	IngressEgressFeeApi, Issuance, KeyProvider, OnBroadcastReady, OnDeposit, QualifyNode,
	RewardsDistribution, RuntimeUpgrade, ScheduledEgressDetails,
};

use codec::{Decode, Encode};
use eth::Address as EvmAddress;
use frame_support::{
	dispatch::{DispatchErrorWithPostInfo, PostDispatchInfo},
	pallet_prelude::DispatchError,
	sp_runtime::{
		traits::{BlockNumberProvider, One, UniqueSaturatedFrom, UniqueSaturatedInto},
		FixedPointNumber, FixedU64,
	},
	traits::{Defensive, Get},
};
pub use missed_authorship_slots::MissedAuraSlots;
pub use offences::*;
use pallet_cf_flip::CallIndexer;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
pub use signer_nomination::RandomSignerNomination;
use sp_core::U256;
use sp_std::{collections::btree_set::BTreeSet, prelude::*};

impl Chainflip for Runtime {
	type RuntimeCall = RuntimeCall;
	type Amount = FlipBalance;
	type ValidatorId = <Self as frame_system::Config>::AccountId;
	type EnsureWitnessed = pallet_cf_witnesser::EnsureWitnessed;
	type EnsurePrewitnessed = pallet_cf_witnesser::EnsurePrewitnessed;
	type EnsureWitnessedAtCurrentEpoch = pallet_cf_witnesser::EnsureWitnessedAtCurrentEpoch;
	type EnsureGovernance = pallet_cf_governance::EnsureGovernance;
	type EpochInfo = Validator;
	type AccountRoleRegistry = AccountRoles;
	type FundingInfo = Flip;
}

struct BackupNodeEmissions;

impl RewardsDistribution for BackupNodeEmissions {
	type Balance = FlipBalance;
	type Issuance = pallet_cf_flip::FlipIssuance<Runtime>;

	fn distribute() {
		if Emissions::backup_node_emission_per_block() == 0 {
			return
		}

		let backup_nodes =
			Validator::highest_funded_qualified_backup_node_bids().collect::<Vec<_>>();
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
			<Emissions as BackupRewardsNotifier>::emit_event(&validator_id, reward);
		}
	}
}

pub struct ChainflipHeartbeat;

impl Heartbeat for ChainflipHeartbeat {
	type ValidatorId = AccountId;
	type BlockNumber = BlockNumber;

	fn on_heartbeat_interval() {
		<Emissions as BlockEmissions>::calculate_block_emissions();
		BackupNodeEmissions::distribute();
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

/// We are willing to pay at most 2x the base fee. This is approximately the theoretical
/// limit of the rate of increase of the base fee over 6 blocks (12.5% per block).
const ETHEREUM_BASE_FEE_MULTIPLIER: FixedU64 = FixedU64::from_rational(2, 1);
/// Arbitrum has smaller variability so we are willing to pay at most 1.5x the base fee.
const ARBITRUM_BASE_FEE_MULTIPLIER: FixedU64 = FixedU64::from_rational(3, 2);

pub trait EvmPriorityFee<C: Chain> {
	fn get_priority_fee(_tracked_data: &C::TrackedData) -> Option<U256> {
		None
	}
}

impl EvmPriorityFee<Ethereum> for EthTransactionBuilder {
	fn get_priority_fee(tracked_data: &<Ethereum as Chain>::TrackedData) -> Option<U256> {
		Some(U256::from(tracked_data.priority_fee))
	}
}

impl EvmPriorityFee<Arbitrum> for ArbTransactionBuilder {
	// Setting the priority fee to zero to prevent the CFE from setting a different value
	fn get_priority_fee(_tracked_data: &<Arbitrum as Chain>::TrackedData) -> Option<U256> {
		Some(U256::from(0))
	}
}

pub struct EthTransactionBuilder;
pub struct ArbTransactionBuilder;
impl_transaction_builder_for_evm_chain!(
	Ethereum,
	EthTransactionBuilder,
	EthereumApi<EvmEnvironment>,
	EthereumChainTracking,
	ETHEREUM_BASE_FEE_MULTIPLIER
);
impl_transaction_builder_for_evm_chain!(
	Arbitrum,
	ArbTransactionBuilder,
	ArbitrumApi<EvmEnvironment>,
	ArbitrumChainTracking,
	ARBITRUM_BASE_FEE_MULTIPLIER
);

#[macro_export]
macro_rules! impl_transaction_builder_for_evm_chain {
	( $chain: ident, $transaction_builder: ident, $chain_api: ident <$env: ident>, $chain_tracking: ident, $base_fee_multiplier: expr ) => {
		impl TransactionBuilder<$chain, $chain_api<$env>> for $transaction_builder {
			fn build_transaction(
				signed_call: &$chain_api<$env>,
			) -> Transaction {
				Transaction {
					chain_id: signed_call.replay_protection().chain_id,
					contract: signed_call.replay_protection().contract_address,
					data: signed_call.chain_encoded(),
					gas_limit: Self::calculate_gas_limit(signed_call),
					..Default::default()
				}
			}

			fn refresh_unsigned_data(unsigned_tx: &mut Transaction) {
				if let Some(ChainState { tracked_data, .. }) = $chain_tracking::chain_state() {
					let max_fee_per_gas = tracked_data.max_fee_per_gas($base_fee_multiplier);
					unsigned_tx.max_fee_per_gas = Some(U256::from(max_fee_per_gas));
					unsigned_tx.max_priority_fee_per_gas = $transaction_builder::get_priority_fee(&tracked_data);
				} else {
					log::warn!("No chain data for {}. This should never happen. Please check Chain Tracking data.", $chain::NAME);
				}
			}

			fn requires_signature_refresh(
				call: &$chain_api<$env>,
				_payload: &<EvmCrypto as ChainCrypto>::Payload,
				maybe_current_on_chain_key: Option<<EvmCrypto as ChainCrypto>::AggKey>
			) -> RequiresSignatureRefresh<EvmCrypto, $chain_api<$env>> {
				maybe_current_on_chain_key.map_or(RequiresSignatureRefresh::False,
					|current_on_chain_key| if call.signer().is_some_and(|signer|current_on_chain_key != signer ) {
						RequiresSignatureRefresh::True(None)
					} else {
						RequiresSignatureRefresh::False
					}
				)
			}

			/// Calculate the gas limit for this evm chain's call. For CCM calls the gas limit is calculated from the gas budget
			/// while for regular calls the gas limit is set to None and the engines will estimate the gas required on broadcast.
			fn calculate_gas_limit(call: &$chain_api<$env>) -> Option<U256> {
				if let (Some((gas_budget, message_length, transfer_asset)), Some(native_asset)) =
					(call.ccm_transfer_data(), <$env as EvmEnvironmentProvider<$chain>>::token_address($chain::GAS_ASSET)) {
						let gas_limit = $chain_tracking::chain_state()
						.or_else(||{
							log::warn!("No chain data for {}. This should never happen. Please check Chain Tracking data.", $chain::NAME);
							None
						})?
						.tracked_data
						.calculate_ccm_gas_limit(transfer_asset ==  native_asset, gas_budget, message_length);

						Some(gas_limit.into())
				} else {
					None
				}
			}
		}
	}
}

pub struct DotTransactionBuilder;
impl TransactionBuilder<Polkadot, PolkadotApi<DotEnvironment>> for DotTransactionBuilder {
	fn build_transaction(
		signed_call: &PolkadotApi<DotEnvironment>,
	) -> <Polkadot as Chain>::Transaction {
		PolkadotTransactionData { encoded_extrinsic: signed_call.chain_encoded() }
	}

	fn refresh_unsigned_data(_unsigned_tx: &mut <Polkadot as Chain>::Transaction) {
		// TODO: For now this is a noop until we actually have dot chain tracking
	}

	fn requires_signature_refresh(
		call: &PolkadotApi<DotEnvironment>,
		payload: &<<Polkadot as Chain>::ChainCrypto as ChainCrypto>::Payload,
		_maybe_current_onchain_key: Option<<PolkadotCrypto as ChainCrypto>::AggKey>,
	) -> RequiresSignatureRefresh<PolkadotCrypto, PolkadotApi<DotEnvironment>> {
		// Current key and signature are irrelevant. The only thing that can invalidate a polkadot
		// transaction is if the payload changes due to a runtime version update.
		if &call.threshold_signature_payload() != payload {
			RequiresSignatureRefresh::True(None)
		} else {
			RequiresSignatureRefresh::False
		}
	}
}

impl TransactionBuilder<Assethub, AssethubApi<HubEnvironment>> for DotTransactionBuilder {
	fn build_transaction(
		signed_call: &AssethubApi<HubEnvironment>,
	) -> <Assethub as Chain>::Transaction {
		PolkadotTransactionData { encoded_extrinsic: signed_call.chain_encoded() }
	}

	fn refresh_unsigned_data(_unsigned_tx: &mut <Assethub as Chain>::Transaction) {
		// The only relevant data here would be the tip, but this is typically zero,
		// so a noop here is ok for now. If we want to implement this, then let's do
		// it together with the same method for Polkadot above.
	}

	fn requires_signature_refresh(
		call: &AssethubApi<HubEnvironment>,
		payload: &<<Assethub as Chain>::ChainCrypto as ChainCrypto>::Payload,
		_maybe_current_onchain_key: Option<<PolkadotCrypto as ChainCrypto>::AggKey>,
	) -> RequiresSignatureRefresh<PolkadotCrypto, AssethubApi<HubEnvironment>> {
		// Current key and signature are irrelevant. The only thing that can invalidate a polkadot
		// transaction is if the payload changes due to a runtime version update.
		if &call.threshold_signature_payload() != payload {
			RequiresSignatureRefresh::True(None)
		} else {
			RequiresSignatureRefresh::False
		}
	}
}

pub struct BtcTransactionBuilder;
impl TransactionBuilder<Bitcoin, BitcoinApi<BtcEnvironment>> for BtcTransactionBuilder {
	fn build_transaction(
		signed_call: &BitcoinApi<BtcEnvironment>,
	) -> <Bitcoin as Chain>::Transaction {
		BitcoinTransactionData { encoded_transaction: signed_call.chain_encoded() }
	}

	fn refresh_unsigned_data(_unsigned_tx: &mut <Bitcoin as Chain>::Transaction) {
		// Since BTC txs are chained and the subsequent tx depends on the success of the previous
		// one, changing the BTC tx fee will mean all subsequent txs are also invalid and so
		// refreshing btc tx is not trivial. We leave it a no-op for now.
	}

	fn requires_signature_refresh(
		_call: &BitcoinApi<BtcEnvironment>,
		_payload: &<<Bitcoin as Chain>::ChainCrypto as ChainCrypto>::Payload,
		_maybe_current_onchain_key: Option<<BitcoinCrypto as ChainCrypto>::AggKey>,
	) -> RequiresSignatureRefresh<BitcoinCrypto, BitcoinApi<BtcEnvironment>> {
		// The payload for a Bitcoin transaction will never change and so it doesn't need to be
		// checked here. We also don't need to check for the signature here because even if we are
		// in the next epoch and the key has changed, the old signature for the btc tx is still
		// valid since its based on those old input UTXOs. In fact, we never have to resign btc
		// txs and the btc tx is always valid as long as the input UTXOs are valid. Therefore, we
		// don't have to check anything here and just rebroadcast.
		RequiresSignatureRefresh::False
	}
}

pub struct SolanaTransactionBuilder;
impl TransactionBuilder<Solana, SolanaApi<SolEnvironment>> for SolanaTransactionBuilder {
	fn build_transaction(
		signed_call: &SolanaApi<SolEnvironment>,
	) -> <Solana as Chain>::Transaction {
		SolanaTransactionData {
			serialized_transaction: signed_call.chain_encoded(),
			// skip_preflight when broadcasting ccm transfers to consume the nonce even if the
			// transaction reverts
			skip_preflight: matches!(
				signed_call.call_type,
				SolanaTransactionType::CcmTransfer { .. }
			),
		}
	}

	fn refresh_unsigned_data(_tx: &mut <Solana as Chain>::Transaction) {
		// It would only make sense to refresh the priority fee here but that would require
		// resigning. To not have two valid transactions we'd need to resign with the same
		// already used nonce which is unnecessarily cumbersome and not worth it. Having too
		// low fees might delay its inclusion but the transaction will remain valid.
	}

	fn calculate_gas_limit(_call: &SolanaApi<SolEnvironment>) -> Option<U256> {
		// In non-CCM broadcasts the gas_limit will be adequately set in the transaction
		// builder. In CCM broadcasts the gas_limit be set in the instruction builder.
		None
	}

	fn requires_signature_refresh(
		call: &SolanaApi<SolEnvironment>,
		_payload: &<<Solana as Chain>::ChainCrypto as ChainCrypto>::Payload,
		maybe_current_onchain_key: Option<<SolanaCrypto as ChainCrypto>::AggKey>,
	) -> RequiresSignatureRefresh<SolanaCrypto, SolanaApi<SolEnvironment>> {
		// The only reason to resign would be if the aggKey has been updated on chain (key rotation)
		// and this apicall is signed with the old key and is still pending. In this case, we need
		// to modify the apicall, by replacing the aggkey with the new key, in the key_accounts in
		// the tx's message to create a new valid threshold signing payload.
		maybe_current_onchain_key.map_or(RequiresSignatureRefresh::False, |current_on_chain_key| {
			match call.signer() {
				Some(signer) if signer != current_on_chain_key => {
					let mut modified_call = (*call).clone();
					SolanaThresholdSigner::active_epoch_key().map_or(
						RequiresSignatureRefresh::False,
						|active_epoch_key| {
							let current_aggkey = active_epoch_key.key;
							modified_call.transaction.message.map_static_account_keys(|key| {
								if key == signer.into() {
									current_aggkey.into()
								} else {
									key
								}
							});

							for sig in modified_call.transaction.signatures.iter_mut() {
								*sig = Default::default()
							}
							modified_call.signer = None;
							RequiresSignatureRefresh::True(Some(modified_call.clone()))
						},
					)
				},
				_ => RequiresSignatureRefresh::False,
			}
		})
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
pub struct EvmEnvironment;

impl<C: Chain<ReplayProtection = EvmReplayProtection, ReplayProtectionParams = EvmAddress>>
	ReplayProtectionProvider<C> for EvmEnvironment
where
	EvmEnvironment: EvmEnvironmentProvider<C>,
{
	fn replay_protection(
		contract_address: <C as Chain>::ReplayProtectionParams,
	) -> EvmReplayProtection {
		EvmReplayProtection {
			nonce: <Self as EvmEnvironmentProvider<C>>::next_nonce(),
			chain_id: <Self as EvmEnvironmentProvider<C>>::chain_id(),
			key_manager_address: <Self as EvmEnvironmentProvider<C>>::key_manager_address(),
			contract_address,
		}
	}
}

impl EvmEnvironmentProvider<Ethereum> for EvmEnvironment {
	fn token_address(asset: assets::eth::Asset) -> Option<EvmAddress> {
		match asset {
			assets::eth::Asset::Eth => Some(ETHEREUM_ETH_ADDRESS),
			erc20 => Environment::supported_eth_assets(erc20),
		}
	}

	fn vault_address() -> EvmAddress {
		Environment::eth_vault_address()
	}

	fn key_manager_address() -> EvmAddress {
		Environment::key_manager_address()
	}

	fn chain_id() -> cf_chains::evm::api::EvmChainId {
		Environment::ethereum_chain_id()
	}

	fn next_nonce() -> u64 {
		Environment::next_ethereum_signature_nonce()
	}
}

// state chain gateway address only exists for Ethereum and does not exist for any other Evm Chain
impl StateChainGatewayAddressProvider for EvmEnvironment {
	fn state_chain_gateway_address() -> EvmAddress {
		Environment::state_chain_gateway_address()
	}
}

impl EvmEnvironmentProvider<Arbitrum> for EvmEnvironment {
	fn token_address(asset: assets::arb::Asset) -> Option<EvmAddress> {
		match asset {
			assets::arb::Asset::ArbEth => Some(ETHEREUM_ETH_ADDRESS),
			assets::arb::Asset::ArbUsdc => Environment::supported_arb_assets(asset),
		}
	}

	fn vault_address() -> EvmAddress {
		Environment::arb_vault_address()
	}

	fn key_manager_address() -> EvmAddress {
		Environment::arb_key_manager_address()
	}

	fn chain_id() -> EvmChainId {
		Environment::arbitrum_chain_id()
	}

	fn next_nonce() -> u64 {
		Environment::next_arbitrum_signature_nonce()
	}
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct DotEnvironment;

impl ReplayProtectionProvider<Polkadot> for DotEnvironment {
	// Get the Environment values for vault_account, NetworkChoice and the next nonce for the
	// proxy_account
	fn replay_protection(reset_nonce: ResetProxyAccountNonce) -> PolkadotReplayProtection {
		PolkadotReplayProtection {
			genesis_hash: Environment::polkadot_genesis_hash(),
			// It should not be possible to get None here, since we never send
			// any transactions unless we have a vault account and associated
			// proxy.
			signer: <PolkadotThresholdSigner as KeyProvider<PolkadotCrypto>>::active_epoch_key()
				.map(|epoch_key| epoch_key.key)
				.defensive_unwrap_or_default(),
			nonce: Environment::next_polkadot_proxy_account_nonce(reset_nonce),
		}
	}
}

impl Get<RuntimeVersion> for DotEnvironment {
	fn get() -> RuntimeVersion {
		PolkadotChainTracking::chain_state().unwrap().tracked_data.runtime_version
	}
}

impl ChainEnvironment<cf_chains::dot::api::VaultAccount, PolkadotAccountId> for DotEnvironment {
	fn lookup(_: cf_chains::dot::api::VaultAccount) -> Option<PolkadotAccountId> {
		Environment::polkadot_vault_account()
	}
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct HubEnvironment;

impl ReplayProtectionProvider<Assethub> for HubEnvironment {
	// Get the Environment values for vault_account, NetworkChoice and the next nonce for the
	// proxy_account
	fn replay_protection(reset_nonce: ResetProxyAccountNonce) -> PolkadotReplayProtection {
		PolkadotReplayProtection {
			genesis_hash: Environment::assethub_genesis_hash(),
			// It should not be possible to get None here, since we never send
			// any transactions unless we have a vault account and associated
			// proxy.
			signer: <PolkadotThresholdSigner as KeyProvider<PolkadotCrypto>>::active_epoch_key()
				.map(|epoch_key| epoch_key.key)
				.defensive_unwrap_or_default(),
			nonce: Environment::next_assethub_proxy_account_nonce(reset_nonce),
		}
	}
}

impl Get<RuntimeVersion> for HubEnvironment {
	fn get() -> RuntimeVersion {
		AssethubChainTracking::chain_state().unwrap().tracked_data.runtime_version
	}
}

impl Get<OutputAccountId> for HubEnvironment {
	fn get() -> OutputAccountId {
		Environment::next_assethub_output_account_id()
	}
}

impl ChainEnvironment<cf_chains::hub::api::VaultAccount, PolkadotAccountId> for HubEnvironment {
	fn lookup(_: cf_chains::hub::api::VaultAccount) -> Option<PolkadotAccountId> {
		Environment::assethub_vault_account()
	}
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct BtcEnvironment;

impl ReplayProtectionProvider<Bitcoin> for BtcEnvironment {
	fn replay_protection(_params: ()) {}
}

impl ChainEnvironment<UtxoSelectionType, SelectedUtxosAndChangeAmount> for BtcEnvironment {
	fn lookup(utxo_selection_type: UtxoSelectionType) -> Option<SelectedUtxosAndChangeAmount> {
		Environment::select_and_take_bitcoin_utxos(utxo_selection_type)
	}
}

impl ChainEnvironment<(), cf_chains::btc::AggKey> for BtcEnvironment {
	fn lookup(_: ()) -> Option<cf_chains::btc::AggKey> {
		<BitcoinThresholdSigner as KeyProvider<BitcoinCrypto>>::active_epoch_key()
			.map(|epoch_key| epoch_key.key)
	}
}

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct SolEnvironment;

impl ChainEnvironment<ApiEnvironment, SolApiEnvironment> for SolEnvironment {
	fn lookup(_s: ApiEnvironment) -> Option<SolApiEnvironment> {
		Some(Environment::solana_api_environment())
	}
}

impl ChainEnvironment<CurrentAggKey, SolAddress> for SolEnvironment {
	fn lookup(_s: CurrentAggKey) -> Option<SolAddress> {
		let epoch = SolanaThresholdSigner::current_key_epoch()?;
		SolanaThresholdSigner::keys(epoch)
	}
}

impl ChainEnvironment<CurrentOnChainKey, SolAddress> for SolEnvironment {
	fn lookup(_s: CurrentOnChainKey) -> Option<SolAddress> {
		SolanaBroadcaster::current_on_chain_key()
	}
}

impl ChainEnvironment<ComputePrice, SolAmount> for SolEnvironment {
	fn lookup(_s: ComputePrice) -> Option<u64> {
		Some(SolanaChainTrackingProvider::priority_fee())
	}
}

impl ChainEnvironment<DurableNonce, DurableNonceAndAccount> for SolEnvironment {
	fn lookup(_s: DurableNonce) -> Option<DurableNonceAndAccount> {
		Environment::get_sol_nonce_and_account()
	}
}

impl ChainEnvironment<AllNonceAccounts, Vec<DurableNonceAndAccount>> for SolEnvironment {
	fn lookup(_s: AllNonceAccounts) -> Option<Vec<DurableNonceAndAccount>> {
		let nonce_accounts = Environment::get_all_sol_nonce_accounts();
		if nonce_accounts.is_empty() {
			None
		} else {
			Some(nonce_accounts)
		}
	}
}

impl RecoverDurableNonce for SolEnvironment {
	fn recover_durable_nonce(nonce_account: SolAddress) {
		Environment::recover_sol_durable_nonce(nonce_account)
	}
}

impl
	ChainEnvironment<
		BTreeSet<SolAddress>,
		AltWitnessingConsensusResult<Vec<SolAddressLookupTableAccount>>,
	> for SolEnvironment
{
	fn lookup(
		alts: BTreeSet<SolAddress>,
	) -> Option<AltWitnessingConsensusResult<Vec<SolAddressLookupTableAccount>>> {
		solana_elections::solana_alt_result(alts)
	}
}

impl SolanaEnvironment for SolEnvironment {}

pub struct TokenholderGovernanceBroadcaster;

impl TokenholderGovernanceBroadcaster {
	fn broadcast_gov_key<C, B>(
		maybe_old_key: Option<Vec<u8>>,
		new_key: Vec<u8>,
	) -> Result<(), SetGovKeyWithAggKeyError>
	where
		C: Chain,
		B: Broadcaster<C>,
		<B as Broadcaster<C>>::ApiCall: cf_chains::SetGovKeyWithAggKey<C::ChainCrypto>,
	{
		let maybe_old_key = if let Some(old_key) = maybe_old_key {
			Some(
				Decode::decode(&mut &old_key[..])
					.or(Err(SetGovKeyWithAggKeyError::FailedToDecodeKey))?,
			)
		} else {
			None
		};
		let api_call = SetGovKeyWithAggKey::<C::ChainCrypto>::new_unsigned(
			maybe_old_key,
			Decode::decode(&mut &new_key[..])
				.or(Err(SetGovKeyWithAggKeyError::FailedToDecodeKey))?,
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
	) -> Result<(), SetGovKeyWithAggKeyError> {
		match chain {
			ForeignChain::Ethereum =>
				Self::broadcast_gov_key::<Ethereum, EthereumBroadcaster>(maybe_old_key, new_key),
			ForeignChain::Polkadot =>
				Self::broadcast_gov_key::<Polkadot, PolkadotBroadcaster>(maybe_old_key, new_key),
			ForeignChain::Bitcoin => Err(SetGovKeyWithAggKeyError::UnsupportedChain),
			ForeignChain::Arbitrum => Err(SetGovKeyWithAggKeyError::UnsupportedChain),
			ForeignChain::Solana =>
				Self::broadcast_gov_key::<Solana, SolanaBroadcaster>(maybe_old_key, new_key),
			ForeignChain::Assethub =>
				Self::broadcast_gov_key::<Assethub, AssethubBroadcaster>(maybe_old_key, new_key),
		}
	}

	fn is_govkey_compatible(chain: ForeignChain, key: &[u8]) -> bool {
		match chain {
			ForeignChain::Ethereum =>
				Self::is_govkey_compatible::<<Ethereum as Chain>::ChainCrypto>(key),
			ForeignChain::Polkadot =>
				Self::is_govkey_compatible::<<Polkadot as Chain>::ChainCrypto>(key),
			ForeignChain::Bitcoin => false,
			ForeignChain::Arbitrum => false,
			ForeignChain::Solana =>
				Self::is_govkey_compatible::<<Solana as Chain>::ChainCrypto>(key),
			ForeignChain::Assethub =>
				Self::is_govkey_compatible::<<Assethub as Chain>::ChainCrypto>(key),
		}
	}
}

impl CommKeyBroadcaster for TokenholderGovernanceBroadcaster {
	fn broadcast(new_key: <<Ethereum as Chain>::ChainCrypto as ChainCrypto>::GovKey) {
		<EthereumBroadcaster as Broadcaster<Ethereum>>::threshold_sign_and_broadcast(
			SetCommKeyWithAggKey::<EvmCrypto>::new_unsigned(new_key),
		);
	}
}

#[macro_export]
macro_rules! impl_deposit_api_for_anychain {
	( $t: ident, $(($chain: ident, $pallet: ident)),+ ) => {
		impl DepositApi<AnyChain> for $t {
			type AccountId = <Runtime as frame_system::Config>::AccountId;
			type Amount = <Runtime as Chainflip>::Amount;

			fn request_liquidity_deposit_address(
				lp_account: Self::AccountId,
				source_asset: Asset,
				boost_fee: BasisPoints,
				refund_address: ForeignChainAddress,
			) -> Result<(ChannelId, ForeignChainAddress, <AnyChain as cf_chains::Chain>::ChainBlockNumber, FlipBalance), DispatchError> {
				match source_asset.into() {
					$(
						ForeignChainAndAsset::$chain(source_asset) =>
							$pallet::request_liquidity_deposit_address(
								lp_account,
								source_asset,
								boost_fee,
								refund_address,
							).map(|(channel, address, block_number, channel_opening_fee)| (channel, address, block_number.into(), channel_opening_fee)),
					)+
				}
			}

			fn request_swap_deposit_address(
				source_asset: Asset,
				destination_asset: Asset,
				destination_address: ForeignChainAddress,
				broker_commission: Beneficiaries<Self::AccountId>,
				broker_id: Self::AccountId,
				channel_metadata: Option<CcmChannelMetadataChecked>,
				boost_fee: BasisPoints,
				refund_parameters: ChannelRefundParametersForChain<AnyChain>,
				dca_parameters: Option<DcaParameters>,
			) -> Result<(ChannelId, ForeignChainAddress, <AnyChain as cf_chains::Chain>::ChainBlockNumber, FlipBalance), DispatchError> {
				match source_asset.into() {
					$(
						ForeignChainAndAsset::$chain(source_asset) => $pallet::request_swap_deposit_address(
							source_asset,
							destination_asset,
							destination_address,
							broker_commission,
							broker_id,
							channel_metadata,
							boost_fee,
							refund_parameters.try_map_address(|addr|addr.try_into()).map_err(|_|"Invalid Refund address")?,
							dca_parameters,
						).map(|(channel, address, block_number, channel_opening_fee)| (channel, address, block_number.into(), channel_opening_fee)),
					)+
				}
			}
		}
	}
}

#[macro_export]
macro_rules! impl_egress_api_for_anychain {
	( $t: ident, $(($chain: ident, $pallet: ident)),+ ) => {
		impl EgressApi<AnyChain> for $t {
			type EgressError = DispatchError;

			fn schedule_egress(
				asset: Asset,
				amount: <AnyChain as Chain>::ChainAmount,
				destination_address: <AnyChain as Chain>::ChainAccount,
				maybe_ccm_deposit_metadata: Option<CcmDepositMetadataChecked<ForeignChainAddress>>,
			) -> Result<ScheduledEgressDetails<AnyChain>, DispatchError> {
				match asset.into() {
					$(
						ForeignChainAndAsset::$chain(asset) => $pallet::schedule_egress(
							asset,
							amount.try_into().expect("Checked for amount compatibility"),
							destination_address
								.try_into()
								.expect("This address cast is ensured to succeed."),
							maybe_ccm_deposit_metadata,
						)
						.map(|ScheduledEgressDetails { egress_id, egress_amount, fee_withheld }| ScheduledEgressDetails { egress_id, egress_amount: egress_amount.into(), fee_withheld: fee_withheld.into() })
						.map_err(Into::into),
					)+
				}
			}
		}
	}
}

pub struct AnyChainIngressEgressHandler;
impl_deposit_api_for_anychain!(
	AnyChainIngressEgressHandler,
	(Ethereum, EthereumIngressEgress),
	(Polkadot, PolkadotIngressEgress),
	(Bitcoin, BitcoinIngressEgress),
	(Arbitrum, ArbitrumIngressEgress),
	(Solana, SolanaIngressEgress),
	(Assethub, AssethubIngressEgress)
);

impl_egress_api_for_anychain!(
	AnyChainIngressEgressHandler,
	(Ethereum, EthereumIngressEgress),
	(Polkadot, PolkadotIngressEgress),
	(Bitcoin, BitcoinIngressEgress),
	(Arbitrum, ArbitrumIngressEgress),
	(Solana, SolanaIngressEgress),
	(Assethub, AssethubIngressEgress)
);

pub struct DepositHandler;
impl OnDeposit<Ethereum> for DepositHandler {}
impl OnDeposit<Polkadot> for DepositHandler {}
impl OnDeposit<Bitcoin> for DepositHandler {
	fn on_deposit_made(utxo: Utxo) {
		Environment::add_bitcoin_utxo_to_list(utxo)
	}
}
impl OnDeposit<Arbitrum> for DepositHandler {}
impl OnDeposit<Solana> for DepositHandler {}
impl OnDeposit<Assethub> for DepositHandler {}

pub struct ChainAddressConverter;

impl AddressConverter for ChainAddressConverter {
	fn to_encoded_address(address: ForeignChainAddress) -> EncodedAddress {
		to_encoded_address(address, Environment::network_environment)
	}

	fn try_from_encoded_address(
		encoded_address: EncodedAddress,
	) -> Result<ForeignChainAddress, ()> {
		try_from_encoded_address(encoded_address, Environment::network_environment)
	}

	fn decode_and_validate_address_for_asset(
		encoded_address: EncodedAddress,
		asset: Asset,
	) -> Result<ForeignChainAddress, AddressError> {
		decode_and_validate_address_for_asset(
			encoded_address,
			asset,
			Environment::network_environment,
		)
	}
}

pub struct BroadcastReadyProvider;
impl OnBroadcastReady<Ethereum> for BroadcastReadyProvider {
	type ApiCall = EthereumApi<EvmEnvironment>;
}
impl OnBroadcastReady<Polkadot> for BroadcastReadyProvider {
	type ApiCall = PolkadotApi<DotEnvironment>;
}
impl OnBroadcastReady<Bitcoin> for BroadcastReadyProvider {
	type ApiCall = BitcoinApi<BtcEnvironment>;

	fn on_broadcast_ready(api_call: &Self::ApiCall) {
		if let BitcoinApi::BatchTransfer(batch_transfer) = api_call {
			let tx_id = batch_transfer.bitcoin_transaction.txid();
			let outputs = &batch_transfer.bitcoin_transaction.outputs;
			let btc_key = pallet_cf_threshold_signature::Pallet::<Runtime, BitcoinInstance>::keys(
				pallet_cf_threshold_signature::Pallet::<Runtime, BitcoinInstance>::current_key_epoch()
					.expect("We should always have an epoch set")).expect("We should always have a key set for the current epoch");
			for (i, output) in outputs.iter().enumerate() {
				if [
					ScriptPubkey::Taproot(btc_key.previous.unwrap_or_default()),
					ScriptPubkey::Taproot(btc_key.current),
				]
				.contains(&output.script_pubkey)
				{
					Environment::add_bitcoin_change_utxo(
						output.amount,
						UtxoId { tx_id, vout: i as u32 },
						batch_transfer.change_utxo_key,
					);
				}
			}
		}
	}
}

impl OnBroadcastReady<Arbitrum> for BroadcastReadyProvider {
	type ApiCall = ArbitrumApi<EvmEnvironment>;
}
impl OnBroadcastReady<Solana> for BroadcastReadyProvider {
	type ApiCall = SolanaApi<SolEnvironment>;
}
impl OnBroadcastReady<Assethub> for BroadcastReadyProvider {
	type ApiCall = AssethubApi<HubEnvironment>;
}

pub struct BitcoinFeeGetter;
impl cf_traits::GetBitcoinFeeInfo for BitcoinFeeGetter {
	fn bitcoin_fee_info() -> BitcoinFeeInfo {
		BitcoinChainTracking::chain_state().unwrap().tracked_data.btc_fee_info
	}
}

pub struct ValidatorRoleQualification;

impl QualifyNode<<Runtime as Chainflip>::ValidatorId> for ValidatorRoleQualification {
	fn is_qualified(id: &<Runtime as Chainflip>::ValidatorId) -> bool {
		AccountRoles::has_account_role(id, AccountRole::Validator)
	}
}

// Calculates the APY of a given account, returned in Basis Points (1 b.p. = 0.01%)
// Returns Some(APY) if the account is a Validator/backup validator.
// Otherwise returns None.
pub fn calculate_account_apy(account_id: &AccountId) -> Option<u32> {
	if pallet_cf_validator::CurrentAuthorities::<Runtime>::get().contains(account_id) {
		// Authority: reward is earned by authoring a block.
		Some(
			Emissions::current_authority_emission_per_block() * YEAR as u128 /
				pallet_cf_validator::CurrentAuthorities::<Runtime>::decode_non_dedup_len()
					.expect("Current authorities must exists and non-empty.") as u128,
		)
	} else {
		let backups_earning_rewards =
			Validator::highest_funded_qualified_backup_node_bids().collect::<Vec<_>>();
		if backups_earning_rewards.iter().any(|bid| bid.bidder_id == *account_id) {
			// Calculate backup validator reward for the current block, then scaled linearly into
			// YEAR.
			calculate_backup_rewards::<AccountId, FlipBalance>(
				backups_earning_rewards,
				Validator::bond(),
				One::one(),
				Emissions::backup_node_emission_per_block(),
				Emissions::current_authority_emission_per_block(),
				u128::from(Validator::current_authority_count()),
			)
			.into_iter()
			.find(|(id, _reward)| *id == *account_id)
			.map(|(_id, reward)| reward * YEAR as u128)
		} else {
			None
		}
	}
	.map(|reward_pa| {
		// Convert Permill to Basis Point.
		FixedU64::from_rational(reward_pa, Flip::balance(account_id))
			.checked_mul_int(10_000u32)
			.unwrap_or_default()
	})
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug, Encode, Decode)]
pub struct BlockUpdate<Data> {
	pub block_hash: Hash,
	pub block_number: BlockNumber,
	pub timestamp: u64,
	// NOTE: Flatten requires Data types to be struct or map
	// Also flatten is incompatible with u128, so AssetAmounts needs to be String type.
	#[serde(flatten)]
	pub data: Data,
}

#[macro_export]
macro_rules! impl_ingress_egress_fee_api_for_anychain {
	( $t: ident, $(($chain: ident, $pallet: ident)),+ ) => {
		impl IngressEgressFeeApi<AnyChain> for $t {
			fn accrue_withheld_fee(asset: Asset, fee: <AnyChain as Chain>::ChainAmount) {
				match asset.into() {
					$(
						ForeignChainAndAsset::$chain(asset) => $pallet::accrue_withheld_fee(
							asset,
							fee.try_into().expect("Checked for amount compatibility"),
						),
					)+
				}
			}
		}
	}
}

pub struct IngressEgressFeeHandler;
impl_ingress_egress_fee_api_for_anychain!(
	IngressEgressFeeHandler,
	(Ethereum, EthereumIngressEgress),
	(Polkadot, PolkadotIngressEgress),
	(Bitcoin, BitcoinIngressEgress),
	(Arbitrum, ArbitrumIngressEgress),
	(Solana, SolanaIngressEgress),
	(Assethub, AssethubIngressEgress)
);

pub struct SolanaLimit;
impl FetchesTransfersLimitProvider for SolanaLimit {
	fn maybe_transfers_limit() -> Option<usize> {
		// we need to leave one nonce for the fetch tx and one nonce reserved for rotation tx since
		// rotation tx can fail to build if all nonce accounts are occupied
		Some(
			Environment::get_number_of_available_sol_nonce_accounts(false)
				.saturating_sub(NONCE_AVAILABILITY_THRESHOLD_FOR_INITIATING_TRANSFER),
		)
	}

	fn maybe_ccm_limit() -> Option<usize> {
		// Subtract one extra nonce compared to the regular transfer limit to make sure that
		// CCMs will never block regular transfers.
		Some(Self::maybe_transfers_limit()?.saturating_sub(1))
	}

	fn maybe_fetches_limit() -> Option<usize> {
		// only fetch if we have more than once nonce account available since one nonce is
		// reserved for rotations. See above
		Some(if Environment::get_number_of_available_sol_nonce_accounts(false) > 0 {
			cf_chains::sol::MAX_SOL_FETCHES_PER_TX
		} else {
			0
		})
	}
}

pub struct EvmLimit;
impl FetchesTransfersLimitProvider for EvmLimit {
	fn maybe_transfers_limit() -> Option<usize> {
		Some(50)
	}

	fn maybe_ccm_limit() -> Option<usize> {
		// For ccm calls we don't batch
		None
	}

	fn maybe_fetches_limit() -> Option<usize> {
		Some(20)
	}
}

pub struct MinimumDepositProvider;
impl cf_traits::MinimumDeposit for MinimumDepositProvider {
	fn get(asset: Asset) -> AssetAmount {
		use pallet_cf_ingress_egress::MinimumDeposit;
		match asset.into() {
			ForeignChainAndAsset::Ethereum(asset) =>
				MinimumDeposit::<Runtime, EthereumInstance>::get(asset),
			ForeignChainAndAsset::Polkadot(asset) =>
				MinimumDeposit::<Runtime, PolkadotInstance>::get(asset),
			ForeignChainAndAsset::Bitcoin(asset) =>
				MinimumDeposit::<Runtime, BitcoinInstance>::get(asset).into(),
			ForeignChainAndAsset::Arbitrum(asset) =>
				MinimumDeposit::<Runtime, ArbitrumInstance>::get(asset),
			ForeignChainAndAsset::Solana(asset) =>
				MinimumDeposit::<Runtime, SolanaInstance>::get(asset).into(),
			ForeignChainAndAsset::Assethub(asset) =>
				MinimumDeposit::<Runtime, AssethubInstance>::get(asset),
		}
	}
}

pub struct LpOrderCallIndexer;
impl CallIndexer<RuntimeCall> for LpOrderCallIndexer {
	/// Calls are indexed by the pool's base asset.
	type CallIndex = Asset;

	fn call_index(call: &RuntimeCall) -> Option<Self::CallIndex> {
		match call {
			RuntimeCall::LiquidityPools(pallet_cf_pools::Call::set_limit_order {
				base_asset,
				..
			}) |
			RuntimeCall::LiquidityPools(pallet_cf_pools::Call::update_limit_order {
				base_asset,
				..
			}) |
			RuntimeCall::LiquidityPools(pallet_cf_pools::Call::set_range_order {
				base_asset,
				..
			}) |
			RuntimeCall::LiquidityPools(pallet_cf_pools::Call::update_range_order {
				base_asset,
				..
			}) => Some(*base_asset),
			_ => None,
		}
	}
}

// Timestamp of the header in seconds past the unix epoch.
pub fn get_header_timestamp(header: &crate::Header) -> Option<u64> {
	header
		.digest
		.logs()
		.iter()
		.find_map(missed_authorship_slots::extract_slot_from_digest_item)
		.map(|slot| slot.saturating_mul(cf_primitives::SECONDS_PER_BLOCK))
}

pub struct CfCcmAdditionalDataHandler;
impl CcmAdditionalDataHandler for CfCcmAdditionalDataHandler {
	fn handle_ccm_additional_data(ccm_data: DecodedCcmAdditionalData) {
		match ccm_data {
			DecodedCcmAdditionalData::NotRequired => {},
			DecodedCcmAdditionalData::Solana(versioned_solana_ccm_additional_data) => {
				versioned_solana_ccm_additional_data.alt_addresses().inspect(|alt_addresses| {
					solana_elections::initiate_solana_alt_election(BTreeSet::from_iter(
						alt_addresses.clone(),
					))
				});
			},
		}
	}
}

pub trait PriceFeedApi {
	fn get_price(asset: assets::any::Asset) -> Option<OraclePrice>;
}

#[allow(dead_code)]
struct ChainlinkOracle;
impl PriceFeedApi for ChainlinkOracle {
	fn get_price(asset: assets::any::Asset) -> Option<OraclePrice> {
		decode_and_get_latest_oracle_price::<TypesFor<Chainlink>>(asset)
	}
}
