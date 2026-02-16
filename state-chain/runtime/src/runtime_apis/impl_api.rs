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

use crate::{
	chainflip::{
		ethereum_sc_calls::*, witnessing::solana_elections::SolanaChainTrackingProvider, *,
	},
	runtime_apis::{
		custom_api::{runtime_decl_for_custom_runtime_api::CustomRuntimeApi as _, *},
		monitoring_api::*,
		types::*,
	},
	safe_mode::RuntimeSafeMode,
	*,
};
use cf_amm::{
	common::PoolPairsMap,
	math::{Amount, Tick},
	range_orders::Liquidity,
};
use cf_chains::{
	self,
	address::{AddressConverter, EncodedAddress, IntoForeignChainAddress},
	assets::any::AssetMap,
	btc::{api::BitcoinApi, ScriptPubkey},
	cf_parameters::build_and_encode_cf_parameters,
	eth::Address as EthereumAddress,
	evm::{api::EvmCall, U256},
	CcmChannelMetadataUnchecked, Chain, ChannelRefundParametersUncheckedEncoded,
	EvmVaultSwapExtraParameters, TransactionBuilder, VaultSwapExtraParameters,
	VaultSwapExtraParametersEncoded, VaultSwapInputEncoded,
};
use cf_primitives::{
	chains::*, AccountRole, Affiliates, Asset, AssetAmount, BasisPoints, BlockNumber, BroadcastId,
	ChannelId, DcaParameters, EpochIndex, FlipBalance, ForeignChain, IngressOrEgress,
	NetworkEnvironment, SemVer, STABLE_ASSET,
};
use cf_traits::{
	AdjustedFeeEstimationApi, AssetConverter, BalanceApi, ChainflipWithTargetChain, EpochKey,
	GetBlockHeight, KeyProvider, SwapLimits, SwapParameterValidation,
};
use codec::{Decode, Encode};
use core::ops::Range;
use frame_support::{
	genesis_builder_helper::build_state,
	pallet_prelude::{TransactionSource, TransactionValidity},
	sp_runtime::{
		traits::{Block as BlockT, NumberFor, Saturating, UniqueSaturatedInto},
		ApplyExtrinsicResult,
	},
};
use pallet_cf_elections::electoral_systems::oracle_price::{
	chainlink::{get_latest_oracle_prices, OraclePrice},
	price::PriceAsset,
};
use pallet_cf_funding::MinimumFunding;
use pallet_cf_governance::GovCallHash;
pub use pallet_cf_ingress_egress::ChannelAction;
pub use pallet_cf_lending_pools::{BoostConfiguration, BoostPoolDetails};
use pallet_cf_pools::{
	AskBidMap, HistoricalEarnedFees, PoolInfo, PoolLiquidity, PoolOrderbook, PoolOrders,
	PoolPriceV1, PoolPriceV2, UnidirectionalPoolDepth,
};
use pallet_cf_reputation::HeartbeatQualification;
use pallet_cf_swapping::{AffiliateDetails, BrokerPrivateBtcChannels, SwapLegInfo};
use pallet_cf_validator::{AssociationToOperator, DelegationAcceptance};
use scale_info::prelude::string::String;
use sp_api::impl_runtime_apis;
use sp_core::OpaqueMetadata;
use sp_std::{
	collections::{btree_map::BTreeMap, btree_set::BTreeSet},
	vec::Vec,
};

#[cfg(feature = "runtime-benchmarks")]
#[macro_use]
mod benches {
	frame_benchmarking::define_benchmarks!(
		[frame_benchmarking, BaselineBench::<Runtime>]
		[frame_system, SystemBench::<Runtime>]
		[pallet_timestamp, Timestamp]
		[pallet_cf_environment, Environment]
		[pallet_cf_flip, Flip]
		[pallet_cf_emissions, Emissions]
		[pallet_cf_funding, Funding]
		[pallet_session, SessionBench::<Runtime>]
		[pallet_cf_witnesser, Witnesser]
		[pallet_cf_validator, Validator]
		[pallet_cf_governance, Governance]
		[pallet_cf_tokenholder_governance, TokenholderGovernance]
		[pallet_cf_vaults, EthereumVault]
		[pallet_cf_reputation, Reputation]
		[pallet_cf_threshold_signature, EvmThresholdSigner]
		[pallet_cf_broadcast, EthereumBroadcaster]
		[pallet_cf_chain_tracking, EthereumChainTracking]
		[pallet_cf_swapping, Swapping]
		[pallet_cf_account_roles, AccountRoles]
		[pallet_cf_ingress_egress, EthereumIngressEgress]
		[pallet_cf_lp, LiquidityProvider]
		[pallet_cf_pools, LiquidityPools]
		[pallet_cf_cfe_interface, CfeInterface]
		[pallet_cf_asset_balances, AssetBalances]
		[pallet_cf_elections, SolanaElections]
		[pallet_cf_trading_strategy, TradingStrategy]
		[pallet_cf_lending_pools, LendingPools]
	);
}

impl_runtime_apis! {
	impl sp_api::Core<Block> for Runtime {
		fn version() -> RuntimeVersion {
			VERSION
		}

		fn execute_block(block: Block) {
			Executive::execute_block(block);
		}

		fn initialize_block(header: &<Block as BlockT>::Header) -> sp_runtime::ExtrinsicInclusionMode {
			Executive::initialize_block(header)
		}
	}

	impl sp_api::Metadata<Block> for Runtime {
		fn metadata() -> OpaqueMetadata {
			OpaqueMetadata::new(Runtime::metadata().into())
		}

		fn metadata_at_version(version: u32) -> Option<OpaqueMetadata> {
			Runtime::metadata_at_version(version)
		}

		fn metadata_versions() -> sp_std::vec::Vec<u32> {
			Runtime::metadata_versions()
		}
	}

	impl sp_block_builder::BlockBuilder<Block> for Runtime {
		fn apply_extrinsic(extrinsic: <Block as BlockT>::Extrinsic) -> ApplyExtrinsicResult {
			Executive::apply_extrinsic(extrinsic)
		}

		fn finalize_block() -> <Block as BlockT>::Header {
			Executive::finalize_block()
		}

		fn inherent_extrinsics(data: sp_inherents::InherentData) -> Vec<<Block as BlockT>::Extrinsic> {
			data.create_extrinsics()
		}

		fn check_inherents(
			block: Block,
			data: sp_inherents::InherentData,
		) -> sp_inherents::CheckInherentsResult {
			data.check_extrinsics(&block)
		}
	}

	impl sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block> for Runtime {
		fn validate_transaction(
			source: TransactionSource,
			tx: <Block as BlockT>::Extrinsic,
			block_hash: <Block as BlockT>::Hash,
		) -> TransactionValidity {
			Executive::validate_transaction(source, tx, block_hash)
		}
	}

	impl sp_offchain::OffchainWorkerApi<Block> for Runtime {
		fn offchain_worker(header: &<Block as BlockT>::Header) {
			Executive::offchain_worker(header)
		}
	}

	impl sp_consensus_aura::AuraApi<Block, AuraId> for Runtime {
		fn slot_duration() -> sp_consensus_aura::SlotDuration {
			sp_consensus_aura::SlotDuration::from_millis(Aura::slot_duration())
		}

		fn authorities() -> Vec<AuraId> {
			pallet_aura::Authorities::<Runtime>::get().into_inner()
		}
	}

	impl sp_session::SessionKeys<Block> for Runtime {
		fn generate_session_keys(seed: Option<Vec<u8>>) -> Vec<u8> {
			opaque::SessionKeys::generate(seed)
		}

		fn decode_session_keys(encoded: Vec<u8>) -> Option<Vec<(Vec<u8>, KeyTypeId)>> {
			opaque::SessionKeys::decode_into_raw_public_keys(&encoded)
		}
	}

	impl sp_consensus_grandpa::GrandpaApi<Block> for Runtime {
		fn grandpa_authorities() -> sp_consensus_grandpa::AuthorityList {
			Grandpa::grandpa_authorities()
		}

		fn current_set_id() -> sp_consensus_grandpa::SetId {
			Grandpa::current_set_id()
		}

		fn submit_report_equivocation_unsigned_extrinsic(
			equivocation_proof: sp_consensus_grandpa::EquivocationProof<
				<Block as BlockT>::Hash,
				NumberFor<Block>,
			>,
			key_owner_proof: sp_consensus_grandpa::OpaqueKeyOwnershipProof,
		) -> Option<()> {
			let key_owner_proof = key_owner_proof.decode()?;

			Grandpa::submit_unsigned_equivocation_report(equivocation_proof, key_owner_proof)
		}

		fn generate_key_ownership_proof(
			_set_id: sp_consensus_grandpa::SetId,
			authority_id: GrandpaId,
		) -> Option<sp_consensus_grandpa::OpaqueKeyOwnershipProof> {
			Historical::prove((sp_consensus_grandpa::KEY_TYPE, authority_id))
				.map(|p| p.encode())
				.map(sp_consensus_grandpa::OpaqueKeyOwnershipProof::new)
		}
	}

	impl frame_system_rpc_runtime_api::AccountNonceApi<Block, AccountId, Nonce> for Runtime {
		fn account_nonce(account: AccountId) -> Nonce {
			System::account_nonce(account)
		}
	}

	impl pallet_transaction_payment_rpc_runtime_api::TransactionPaymentApi<Block, Balance> for Runtime {
		fn query_info(
			uxt: <Block as BlockT>::Extrinsic,
			len: u32,
		) -> pallet_transaction_payment_rpc_runtime_api::RuntimeDispatchInfo<Balance> {
			TransactionPayment::query_info(uxt, len)
		}
		fn query_fee_details(
			uxt: <Block as BlockT>::Extrinsic,
			len: u32,
		) -> pallet_transaction_payment::FeeDetails<Balance> {
			TransactionPayment::query_fee_details(uxt, len)
		}
		fn query_weight_to_fee(weight: Weight) -> Balance {
			TransactionPayment::weight_to_fee(weight)
		}
		fn query_length_to_fee(length: u32) -> Balance {
			TransactionPayment::length_to_fee(length)
		}
	}

	impl
		pallet_transaction_payment_rpc_runtime_api::TransactionPaymentCallApi<
			Block,
			Balance,
			RuntimeCall,
		> for Runtime
	{
		fn query_call_info(
			call: RuntimeCall,
			len: u32,
		) -> pallet_transaction_payment::RuntimeDispatchInfo<Balance> {
			TransactionPayment::query_call_info(call, len)
		}
		fn query_call_fee_details(
			call: RuntimeCall,
			len: u32,
		) -> pallet_transaction_payment::FeeDetails<Balance> {
			TransactionPayment::query_call_fee_details(call, len)
		}
		fn query_weight_to_fee(weight: Weight) -> Balance {
			TransactionPayment::weight_to_fee(weight)
		}
		fn query_length_to_fee(length: u32) -> Balance {
			TransactionPayment::length_to_fee(length)
		}
	}

	#[cfg(feature = "try-runtime")]
	impl frame_try_runtime::TryRuntime<Block> for Runtime {
		fn on_runtime_upgrade(checks: frame_try_runtime::UpgradeCheckSelect) -> (Weight, Weight) {
			// NOTE: intentional unwrap: we don't want to propagate the error backwards, and want to
			// have a backtrace here. If any of the pre/post migration checks fail, we shall stop
			// right here and right now.
			let weight = Executive::try_runtime_upgrade(checks)
				.inspect_err(|e| log::error!("try_runtime_upgrade failed with: {:?}", e))
				.unwrap();
			(weight, BlockWeights::get().max_block)
		}

		fn execute_block(
			block: Block,
			state_root_check: bool,
			signature_check: bool,
			select: frame_try_runtime::TryStateSelect,
		) -> Weight {
			// NOTE: intentional unwrap: we don't want to propagate the error backwards, and want to
			// have a backtrace here.
			Executive::try_execute_block(block, state_root_check, signature_check, select)
				.expect("execute-block failed")
		}
	}

	impl sp_genesis_builder::GenesisBuilder<Block> for Runtime {
		fn build_state(config: Vec<u8>) -> sp_genesis_builder::Result {
			build_state::<RuntimeGenesisConfig>(config)
		}

		fn get_preset(_id: &Option<sp_genesis_builder::PresetId>) -> Option<Vec<u8>> {
			None
		}

		fn preset_names() -> Vec<sp_genesis_builder::PresetId> {
			Default::default()
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	impl frame_benchmarking::Benchmark<Block> for Runtime {
		fn benchmark_metadata(
			extra: bool,
		) -> (Vec<frame_benchmarking::BenchmarkList>, Vec<frame_support::traits::StorageInfo>) {
			use baseline::Pallet as BaselineBench;
			use cf_session_benchmarking::Pallet as SessionBench;
			use frame_benchmarking::{baseline, BenchmarkList, Benchmarking};
			use frame_support::traits::StorageInfoTrait;
			use frame_system_benchmarking::Pallet as SystemBench;

			let mut list = Vec::<BenchmarkList>::new();

			#[cfg(feature = "runtime-benchmarks")]
			list_benchmarks!(list, extra);

			let storage_info = AllPalletsWithSystem::storage_info();

			(list, storage_info)
		}

		#[expect(non_local_definitions)]
		fn dispatch_benchmark(
			config: frame_benchmarking::BenchmarkConfig,
		) -> Result<Vec<frame_benchmarking::BenchmarkBatch>, sp_runtime::RuntimeString> {
			use frame_benchmarking::{baseline, BenchmarkBatch, Benchmarking};
			use frame_support::traits::TrackedStorageKey;

			use baseline::Pallet as BaselineBench;
			use cf_session_benchmarking::Pallet as SessionBench;
			use frame_system_benchmarking::Pallet as SystemBench;

			impl cf_session_benchmarking::Config for Runtime {}
			impl frame_system_benchmarking::Config for Runtime {}
			impl baseline::Config for Runtime {}

			use frame_support::traits::WhitelistedStorageKeys;
			let whitelist: Vec<TrackedStorageKey> = AllPalletsWithSystem::whitelisted_storage_keys();

			let mut batches = Vec::<BenchmarkBatch>::new();
			let params = (&config, &whitelist);
			add_benchmarks!(params, batches);

			Ok(batches)
		}
	}



	// -- Monitoring API --

	impl crate::runtime_apis::monitoring_api::MonitoringRuntimeApi<Block> for Runtime {
		fn cf_authorities() -> AuthoritiesInfo {
			let mut authorities = pallet_cf_validator::CurrentAuthorities::<Runtime>::get();
			let mut result = AuthoritiesInfo {
				authorities: authorities.len() as u32,
				online_authorities: 0,
				backups: 0,
				online_backups: 0,
			};
			authorities.retain(HeartbeatQualification::<Runtime>::is_qualified);
			result.online_authorities = authorities.len() as u32;
			result
		}

		fn cf_external_chains_block_height() -> ExternalChainsBlockHeight {
			// safe to unwrap these value as stated on the storage item doc
			let btc = pallet_cf_chain_tracking::CurrentChainState::<Runtime, BitcoinInstance>::get().unwrap();
			let eth = pallet_cf_chain_tracking::CurrentChainState::<Runtime, EthereumInstance>::get().unwrap();
			let dot = pallet_cf_chain_tracking::CurrentChainState::<Runtime, PolkadotInstance>::get().unwrap();
			let arb = pallet_cf_chain_tracking::CurrentChainState::<Runtime, ArbitrumInstance>::get().unwrap();
			let sol = SolanaChainTrackingProvider::get_block_height();
			let hub = pallet_cf_chain_tracking::CurrentChainState::<Runtime, AssethubInstance>::get().unwrap();
			let trx = DummyTronChainTracking::get_block_height();

			ExternalChainsBlockHeight {
				bitcoin: btc.block_height,
				ethereum: eth.block_height,
				polkadot: dot.block_height.into(),
				solana: sol,
				arbitrum: arb.block_height,
				assethub: hub.block_height.into(),
				tron: trx,
			}
		}

		fn cf_btc_utxos() -> BtcUtxos {
			let utxos = pallet_cf_environment::BitcoinAvailableUtxos::<Runtime>::get();
			let mut btc_balance = utxos.iter().fold(0, |acc, elem| acc + elem.amount);
			//Sum the btc balance contained in the change utxos to the btc "free_balance"
			let btc_ceremonies = pallet_cf_threshold_signature::PendingCeremonies::<Runtime,BitcoinInstance>::iter_values().map(|ceremony|{
				ceremony.request_context.request_id
			}).collect::<Vec<_>>();
			let EpochKey { key, .. } = pallet_cf_threshold_signature::Pallet::<Runtime, BitcoinInstance>::active_epoch_key()
				.expect("We should always have a key for the current epoch");
			for ceremony in btc_ceremonies {
				if let RuntimeCall::BitcoinBroadcaster(pallet_cf_broadcast::pallet::Call::on_signature_ready{ api_call, ..}) = pallet_cf_threshold_signature::RequestCallback::<Runtime, BitcoinInstance>::get(ceremony).unwrap() {
					if let BitcoinApi::BatchTransfer(batch_transfer) = *api_call {
						for output in batch_transfer.bitcoin_transaction.outputs {
							if [
								ScriptPubkey::Taproot(key.previous.unwrap_or_default()),
								ScriptPubkey::Taproot(key.current),
							]
							.contains(&output.script_pubkey)
							{
								btc_balance += output.amount;
							}
						}
					}
				}
			}
			BtcUtxos {
				total_balance: btc_balance,
				count: utxos.len() as u32,
			}
		}

		fn cf_dot_aggkey() -> PolkadotAccountId {
			let epoch = PolkadotThresholdSigner::current_key_epoch().unwrap_or_default();
			PolkadotThresholdSigner::keys(epoch).unwrap_or_default()
		}

		fn cf_suspended_validators() -> Vec<(Offence, u32)> {
			let suspended_for_keygen = match pallet_cf_validator::Pallet::<Runtime>::current_rotation_phase() {
				pallet_cf_validator::RotationPhase::KeygensInProgress(rotation_state) |
				pallet_cf_validator::RotationPhase::KeyHandoversInProgress(rotation_state) |
				pallet_cf_validator::RotationPhase::ActivatingKeys(rotation_state) |
				pallet_cf_validator::RotationPhase::NewKeysActivated(rotation_state) => { rotation_state.banned.len() as u32 },
				_ => {0u32}
			};
			pallet_cf_reputation::Suspensions::<Runtime>::iter().map(|(key, _)| {
				if key == pallet_cf_threshold_signature::PalletOffence::FailedKeygen.into() {
					return (key, suspended_for_keygen);
				}
				(key, pallet_cf_reputation::Pallet::<Runtime>::validators_suspended_for(&[key]).len() as u32)
			}).collect()
		}
		fn cf_epoch_state() -> EpochState {
			EpochState {
				epoch_duration: Validator::epoch_duration(),
				current_epoch_started_at: Validator::current_epoch_started_at(),
				current_epoch_index: Validator::current_epoch(),
				min_active_bid: Validator::resolve_auction_iteratively()
					.ok()
					.map(|(auction_outcome, _)| auction_outcome.bond),
				rotation_phase: Validator::current_rotation_phase().to_str().to_owned(),
			}
		}
		fn cf_redemptions() -> RedemptionsInfo {
			let redemptions: Vec<_> = pallet_cf_funding::PendingRedemptions::<Runtime>::iter().collect();
			RedemptionsInfo {
				total_balance: redemptions.iter().fold(0, |acc, elem| acc + elem.1.total),
				count: redemptions.len() as u32,
			}
		}
		fn cf_pending_broadcasts_count() -> PendingBroadcasts {
			PendingBroadcasts {
				ethereum: pallet_cf_broadcast::PendingBroadcasts::<Runtime, EthereumInstance>::decode_non_dedup_len().unwrap_or(0) as u32,
				bitcoin: pallet_cf_broadcast::PendingBroadcasts::<Runtime, BitcoinInstance>::decode_non_dedup_len().unwrap_or(0) as u32,
				polkadot: pallet_cf_broadcast::PendingBroadcasts::<Runtime, PolkadotInstance>::decode_non_dedup_len().unwrap_or(0) as u32,
				arbitrum: pallet_cf_broadcast::PendingBroadcasts::<Runtime, ArbitrumInstance>::decode_non_dedup_len().unwrap_or(0) as u32,
				solana: pallet_cf_broadcast::PendingBroadcasts::<Runtime, SolanaInstance>::decode_non_dedup_len().unwrap_or(0) as u32,
				assethub: pallet_cf_broadcast::PendingBroadcasts::<Runtime, AssethubInstance>::decode_non_dedup_len().unwrap_or(0) as u32,
				tron: pallet_cf_broadcast::PendingBroadcasts::<Runtime, TronInstance>::decode_non_dedup_len().unwrap_or(0) as u32,
			}
		}
		fn cf_pending_tss_ceremonies_count() -> PendingTssCeremonies {
			PendingTssCeremonies {
				evm: pallet_cf_threshold_signature::PendingCeremonies::<Runtime, EvmInstance>::iter().collect::<Vec<_>>().len() as u32,
				bitcoin: pallet_cf_threshold_signature::PendingCeremonies::<Runtime, BitcoinInstance>::iter().collect::<Vec<_>>().len() as u32,
				polkadot: pallet_cf_threshold_signature::PendingCeremonies::<Runtime, PolkadotCryptoInstance>::iter().collect::<Vec<_>>().len() as u32,
				solana: pallet_cf_threshold_signature::PendingCeremonies::<Runtime, SolanaInstance>::iter().collect::<Vec<_>>().len() as u32,
			}
		}
		fn cf_pending_swaps_count() -> u32 {
			pallet_cf_swapping::ScheduledSwaps::<Runtime>::get().len() as u32
		}
		fn cf_open_deposit_channels_count() -> OpenDepositChannels {
			fn open_channels<BlockHeight, I: 'static>() -> u32
				where BlockHeight: GetBlockHeight<<Runtime as ChainflipWithTargetChain<I>>::TargetChain>, Runtime: pallet_cf_ingress_egress::Config<I>
			{
				pallet_cf_ingress_egress::DepositChannelLookup::<Runtime, I>::iter().filter(|(_key, elem)| elem.expires_at > BlockHeight::get_block_height()).collect::<Vec<_>>().len() as u32
			}

			OpenDepositChannels{
				ethereum: open_channels::<pallet_cf_chain_tracking::Pallet<Runtime, EthereumInstance>, EthereumInstance>(),
				bitcoin: open_channels::<pallet_cf_chain_tracking::Pallet<Runtime, BitcoinInstance>, BitcoinInstance>(),
				polkadot: open_channels::<pallet_cf_chain_tracking::Pallet<Runtime, PolkadotInstance>, PolkadotInstance>(),
				arbitrum: open_channels::<pallet_cf_chain_tracking::Pallet<Runtime, ArbitrumInstance>, ArbitrumInstance>(),
				solana: open_channels::<SolanaChainTrackingProvider, SolanaInstance>(),
				assethub: open_channels::<pallet_cf_chain_tracking::Pallet<Runtime, AssethubInstance>, AssethubInstance>(),
				tron: open_channels::<DummyTronChainTracking, TronInstance>(),
			}
		}
		fn cf_fee_imbalance() -> FeeImbalance<AssetAmount> {
			FeeImbalance {
				ethereum: pallet_cf_asset_balances::Pallet::<Runtime>::vault_imbalance(ForeignChain::Ethereum.gas_asset()),
				polkadot: pallet_cf_asset_balances::Pallet::<Runtime>::vault_imbalance(ForeignChain::Polkadot.gas_asset()),
				arbitrum: pallet_cf_asset_balances::Pallet::<Runtime>::vault_imbalance(ForeignChain::Arbitrum.gas_asset()),
				bitcoin: pallet_cf_asset_balances::Pallet::<Runtime>::vault_imbalance(ForeignChain::Bitcoin.gas_asset()),
				solana: pallet_cf_asset_balances::Pallet::<Runtime>::vault_imbalance(ForeignChain::Solana.gas_asset()),
				assethub: pallet_cf_asset_balances::Pallet::<Runtime>::vault_imbalance(ForeignChain::Assethub.gas_asset()),
				tron: pallet_cf_asset_balances::Pallet::<Runtime>::vault_imbalance(ForeignChain::Tron.gas_asset()),
			}
		}
		fn cf_build_version() -> LastRuntimeUpgradeInfo {
			let info = frame_system::LastRuntimeUpgrade::<Runtime>::get().expect("this has to be set");
			LastRuntimeUpgradeInfo {
				spec_version: info.spec_version.into(),
				spec_name: info.spec_name,
			}
		}
		fn cf_rotation_broadcast_ids() -> ActivateKeysBroadcastIds{
			ActivateKeysBroadcastIds{
				ethereum: pallet_cf_broadcast::IncomingKeyAndBroadcastId::<Runtime, EthereumInstance>::get().map(|val| val.1),
				bitcoin: pallet_cf_broadcast::IncomingKeyAndBroadcastId::<Runtime, BitcoinInstance>::get().map(|val| val.1),
				polkadot: pallet_cf_broadcast::IncomingKeyAndBroadcastId::<Runtime, PolkadotInstance>::get().map(|val| val.1),
				arbitrum: pallet_cf_broadcast::IncomingKeyAndBroadcastId::<Runtime, ArbitrumInstance>::get().map(|val| val.1),
				solana: {
					let broadcast_id = pallet_cf_broadcast::IncomingKeyAndBroadcastId::<Runtime, SolanaInstance>::get().map(|val| val.1);
					(broadcast_id, pallet_cf_broadcast::AwaitingBroadcast::<Runtime, SolanaInstance>::get(broadcast_id.unwrap_or_default()).map(|broadcast_data| broadcast_data.transaction_out_id))
				},
				assethub: pallet_cf_broadcast::IncomingKeyAndBroadcastId::<Runtime, AssethubInstance>::get().map(|val| val.1),
				tron: pallet_cf_broadcast::IncomingKeyAndBroadcastId::<Runtime, TronInstance>::get().map(|val| val.1),
			}
		}
		fn cf_sol_nonces() -> SolanaNonces{
			SolanaNonces {
				available: pallet_cf_environment::SolanaAvailableNonceAccounts::<Runtime>::get(),
				unavailable: pallet_cf_environment::SolanaUnavailableNonceAccounts::<Runtime>::iter_keys().collect()
			}
		}
		fn cf_sol_aggkey() -> SolAddress{
			let epoch = SolanaThresholdSigner::current_key_epoch().unwrap_or_default();
			SolanaThresholdSigner::keys(epoch).unwrap_or_default()
		}
		fn cf_sol_onchain_key() -> SolAddress{
			SolanaBroadcaster::current_on_chain_key().unwrap_or_default()
		}
		fn cf_monitoring_data() -> MonitoringDataV2 {
			MonitoringDataV2 {
				external_chains_height: Self::cf_external_chains_block_height(),
				btc_utxos: Self::cf_btc_utxos(),
				epoch: Self::cf_epoch_state(),
				pending_redemptions: Self::cf_redemptions(),
				pending_broadcasts: Self::cf_pending_broadcasts_count(),
				pending_tss: Self::cf_pending_tss_ceremonies_count(),
				open_deposit_channels: Self::cf_open_deposit_channels_count(),
				fee_imbalance: Self::cf_fee_imbalance(),
				authorities: Self::cf_authorities(),
				build_version: Self::cf_build_version(),
				suspended_validators: Self::cf_suspended_validators(),
				pending_swaps: Self::cf_pending_swaps_count(),
				dot_aggkey: Self::cf_dot_aggkey(),
				flip_supply: {
					let flip = Self::cf_flip_supply();
					FlipSupply { total_supply: flip.0, offchain_supply: flip.1}
				},
				sol_aggkey: Self::cf_sol_aggkey(),
				sol_onchain_key: Self::cf_sol_onchain_key(),
				sol_nonces: Self::cf_sol_nonces(),
				activating_key_broadcast_ids: Self::cf_rotation_broadcast_ids(),
			}
		}
		fn cf_accounts_info(accounts: BoundedVec<AccountId, ConstU32<10>>) -> Vec<ValidatorInfo> {
			accounts.iter().map(|account_id| {
				Self::cf_validator_info(account_id)
			}).collect()
		}
		fn cf_simulate_auction() -> Result<
			(
				AuctionOutcome<AccountId, AssetAmount>,
				BTreeMap<AccountId, DelegationSnapshot<AccountId, AssetAmount>>,
				Vec<AccountId>,
				AssetAmount
			),
			DispatchErrorWithMessage
		>
		{
			let next_auction = Validator::resolve_auction_iteratively()
				.map_err(|e| DispatchErrorWithMessage::from(<pallet_cf_validator::Error<Runtime>>::from(e)))?;
			let next_set: BTreeSet<AccountId> = next_auction.0.winners.iter().cloned().collect();
			let current_set: BTreeSet<AccountId> = Validator::current_authorities().into_iter().collect();
			let current_mab = Validator::bond();
			Ok((next_auction.0, next_auction.1, next_set.difference(&current_set).cloned().collect(), current_mab))
		}
	}


	// -- Elections API --

	impl crate::runtime_apis::elections_api::ElectoralRuntimeApi<Block> for Runtime {
		fn cf_solana_electoral_data(account_id: AccountId) -> Vec<u8> {
			SolanaElections::electoral_data(&account_id).encode()
		}

		fn cf_solana_filter_votes(account_id: AccountId, proposed_votes: Vec<u8>) -> Vec<u8> {
			SolanaElections::filter_votes(&account_id, Decode::decode(&mut &proposed_votes[..]).unwrap_or_default()).encode()
		}

		fn cf_bitcoin_electoral_data(account_id: AccountId) -> Vec<u8> {
			BitcoinElections::electoral_data(&account_id).encode()
		}

		fn cf_bitcoin_filter_votes(account_id: AccountId, proposed_votes: Vec<u8>) -> Vec<u8> {
			BitcoinElections::filter_votes(&account_id, Decode::decode(&mut &proposed_votes[..]).unwrap_or_default()).encode()
		}

		fn cf_generic_electoral_data(account_id: AccountId) -> Vec<u8> {
			GenericElections::electoral_data(&account_id).encode()
		}

		fn cf_generic_filter_votes(account_id: AccountId, proposed_votes: Vec<u8>) -> Vec<u8> {
			GenericElections::filter_votes(&account_id, Decode::decode(&mut &proposed_votes[..]).unwrap_or_default()).encode()
		}

		fn cf_ethereum_electoral_data(account_id: AccountId) -> Vec<u8> {
			EthereumElections::electoral_data(&account_id).encode()
		}

		fn cf_ethereum_filter_votes(account_id: AccountId, proposed_votes: Vec<u8>) -> Vec<u8> {
			EthereumElections::filter_votes(&account_id, Decode::decode(&mut &proposed_votes[..]).unwrap_or_default()).encode()
		}

		fn cf_arbitrum_electoral_data(account_id: AccountId) -> Vec<u8> {
			ArbitrumElections::electoral_data(&account_id).encode()
		}

		fn cf_arbitrum_filter_votes(account_id: AccountId, proposed_votes: Vec<u8>) -> Vec<u8> {
			ArbitrumElections::filter_votes(&account_id, Decode::decode(&mut &proposed_votes[..]).unwrap_or_default()).encode()
		}

		fn cf_tron_electoral_data(_account_id: AccountId) -> Vec<u8> {
			// Tron doesn't have elections, return empty data
			Vec::new()
		}

		fn cf_tron_filter_votes(_account_id: AccountId, _proposed_votes: Vec<u8>) -> Vec<u8> {
			// Tron doesn't have elections, return empty data
			Vec::new()
		}
	}

	// -- Custom API --


	impl runtime_apis::custom_api::CustomRuntimeApi<Block> for Runtime {
		fn cf_is_auction_phase() -> bool {
			Validator::is_auction_phase()
		}
		fn cf_eth_flip_token_address() -> EthereumAddress {
			Environment::supported_eth_assets(cf_primitives::chains::assets::eth::Asset::Flip).expect("FLIP token address should exist")
		}
		fn cf_eth_state_chain_gateway_address() -> EthereumAddress {
			Environment::state_chain_gateway_address()
		}
		fn cf_eth_key_manager_address() -> EthereumAddress {
			Environment::key_manager_address()
		}
		fn cf_eth_chain_id() -> u64 {
			Environment::ethereum_chain_id()
		}
		fn cf_eth_vault() -> ([u8; 33], BlockNumber) {
			let epoch_index = Self::cf_current_epoch();
			// We should always have a Vault for the current epoch, but in case we do
			// not, just return an empty Vault.
			(EvmThresholdSigner::keys(epoch_index).unwrap_or_default().to_pubkey_compressed(), EthereumVault::vault_start_block_numbers(epoch_index).unwrap().unique_saturated_into())
		}
		fn cf_auction_parameters() -> (u32, u32) {
			let auction_params = Validator::auction_parameters();
			(auction_params.min_size, auction_params.max_size)
		}
		fn cf_min_funding() -> u128 {
			MinimumFunding::<Runtime>::get().unique_saturated_into()
		}
		fn cf_current_epoch() -> u32 {
			Validator::current_epoch()
		}
		fn cf_current_compatibility_version() -> SemVer {
			Environment::current_release_version()
		}
		fn cf_epoch_duration() -> u32 {
			Validator::epoch_duration()
		}
		fn cf_current_epoch_started_at() -> u32 {
			Validator::current_epoch_started_at()
		}
		fn cf_authority_emission_per_block() -> u128 {
			Emissions::current_authority_emission_per_block()
		}
		fn cf_backup_emission_per_block() -> u128 {
			0 // Backups don't exist any more.
		}
		fn cf_flip_supply() -> (u128, u128) {
			(Flip::total_issuance(), Flip::offchain_funds())
		}
		fn cf_accounts() -> Vec<(AccountId, Vec<u8>)> {
			let mut vanity_names = AccountRoles::vanity_names();
			frame_system::Account::<Runtime>::iter_keys()
				.map(|account_id| {
					let vanity_name = vanity_names.remove(&account_id).unwrap_or_default().into();
					(account_id, vanity_name)
				})
				.collect()
		}
		fn cf_free_balances(account_id: AccountId) -> AssetMap<AssetAmount> {
			AssetBalances::free_balances(&account_id)
		}
		fn cf_lp_total_balances(account_id: AccountId) -> AssetMap<AssetAmount> {
			let free_balances = AssetBalances::free_balances(&account_id);
			let open_order_balances = LiquidityPools::open_order_balances(&account_id);

			let boost_pools_balances = AssetMap::from_fn(|asset| {
				LendingPools::boost_pool_account_balance(&account_id, asset)
			});

			let trading_strategies_balances = {

				let mut asset_map = AssetMap::<AssetAmount>::default();

				for TradingStrategyInfo { balance, .. } in Self::cf_get_trading_strategies(Some(account_id.clone())) {
					for (asset, amount) in balance {
						asset_map[asset].saturating_accrue(amount);
					}
				}

				asset_map
			};

			let lending_supply_balances = AssetMap::from_fn(|asset| {

				pallet_cf_lending_pools::GeneralLendingPools::<Runtime>::get(asset).and_then(|pool| {

					pool.get_supply_position_for_account(&account_id).ok()

				}).unwrap_or_default()

			});

			let lending_collateral_balances = pallet_cf_lending_pools::LoanAccounts::<Runtime>::get(&account_id).map(|loan_account| {
				let mut asset_map = AssetMap::<AssetAmount>::default();

				for (asset, amount) in loan_account.get_total_collateral() {
					asset_map[asset].saturating_accrue(amount);
				}

				asset_map
			}).unwrap_or_default();


			free_balances
				.saturating_add(open_order_balances)
				.saturating_add(boost_pools_balances)
				.saturating_add(trading_strategies_balances)
				.saturating_add(lending_supply_balances)
				.saturating_add(lending_collateral_balances)
		}
		fn cf_account_flip_balance(account_id: &AccountId) -> u128 {
			pallet_cf_flip::Account::<Runtime>::get(account_id).total()
		}
		fn cf_common_account_info(
			account_id: &AccountId,
		) -> RpcAccountInfoCommonItems<FlipBalance> {
			let flip_account = pallet_cf_flip::Account::<Runtime>::get(account_id);
			let upcoming_delegation_status = pallet_cf_validator::DelegationChoice::<Runtime>::get(account_id)
				.map(|(operator, max_bid)| DelegationInfo { operator, bid: core::cmp::min(flip_account.total(), max_bid) });
			let current_delegation_status = pallet_cf_validator::DelegationSnapshots::<Runtime>::iter_prefix(Validator::current_epoch())
				.find_map(|(operator, snapshot)| snapshot.delegators.get(account_id).map(|&bid|
					DelegationInfo { operator, bid }
				));

			RpcAccountInfoCommonItems {
				vanity_name: pallet_cf_account_roles::VanityNames::<Runtime>::get().get(account_id)
					.cloned()
					.unwrap_or_default()
					.into(),
				flip_balance: flip_account.total(),
				asset_balances: AssetBalances::free_balances(account_id),
				bond: flip_account.bond(),
				estimated_redeemable_balance: pallet_cf_funding::Redemption::<Runtime>::for_rpc(
					account_id,
				).map(|redemption| redemption.redeem_amount).unwrap_or_default(),
				bound_redeem_address: pallet_cf_funding::BoundRedeemAddress::<Runtime>::get(account_id),
				restricted_balances: pallet_cf_funding::RestrictedBalances::<Runtime>::get(account_id),
				current_delegation_status,
				upcoming_delegation_status,
			}
		}
		fn cf_validator_info(account_id: &AccountId) -> ValidatorInfo {
			let keyholder_epochs = pallet_cf_validator::HistoricalActiveEpochs::<Runtime>::get(account_id).into_iter().map(|(e, _)| e).collect();
			let is_qualified = <<Runtime as pallet_cf_validator::Config>::KeygenQualification as QualifyNode<_>>::is_qualified(account_id);
			let is_current_authority = pallet_cf_validator::CurrentAuthorities::<Runtime>::get().contains(account_id);
			let is_bidding = Validator::is_bidding(account_id);
			let bound_redeem_address = pallet_cf_funding::BoundRedeemAddress::<Runtime>::get(account_id);
			let apy_bp = calculate_account_apy(account_id);
			let reputation_info = pallet_cf_reputation::Reputations::<Runtime>::get(account_id);
			let account_info = pallet_cf_flip::Account::<Runtime>::get(account_id);
			let restricted_balances = pallet_cf_funding::RestrictedBalances::<Runtime>::get(account_id);
			let estimated_redeemable_balance = pallet_cf_funding::Redemption::<Runtime>::for_rpc(
				account_id,
			).map(|redemption| redemption.redeem_amount).unwrap_or_default();
			ValidatorInfo {
				balance: account_info.total(),
				bond: account_info.bond(),
				last_heartbeat: pallet_cf_reputation::LastHeartbeat::<Runtime>::get(account_id).unwrap_or(0),
				reputation_points: reputation_info.reputation_points,
				keyholder_epochs,
				is_current_authority,
				is_current_backup: false,
				is_qualified: is_bidding && is_qualified,
				is_online: HeartbeatQualification::<Runtime>::is_qualified(account_id),
				is_bidding,
				bound_redeem_address,
				apy_bp,
				restricted_balances,
				estimated_redeemable_balance,
				operator: pallet_cf_validator::OperatorChoice::<Runtime>::get(account_id),
			}
		}

		fn cf_operator_info(account_id: &AccountId) -> OperatorInfo<FlipBalance> {
			let settings= pallet_cf_validator::OperatorSettingsLookup::<Runtime>::get(account_id).unwrap_or_default();
			let exceptions = pallet_cf_validator::Exceptions::<Runtime>::get(account_id).into_iter().collect();
			let (allowed, blocked) = match &settings.delegation_acceptance {
				DelegationAcceptance::Allow => (Default::default(), exceptions),
				DelegationAcceptance::Deny => (exceptions, Default::default()),
			};
			OperatorInfo {
				managed_validators: pallet_cf_validator::Pallet::<Runtime>::get_all_associations_by_operator(
					account_id,
					AssociationToOperator::Validator,
					|account_id, _| pallet_cf_flip::Pallet::<Runtime>::balance(account_id)
				),
				settings,
				allowed,
				blocked,
				delegators: pallet_cf_validator::Pallet::<Runtime>::get_all_associations_by_operator(
					account_id,
					AssociationToOperator::Delegator,
					|account_id, max_bid| pallet_cf_flip::Pallet::<Runtime>::balance(account_id).min(max_bid.unwrap_or(u128::MAX))
				),
				active_delegation: pallet_cf_validator::DelegationSnapshots::<Runtime>::get(
					Validator::current_epoch(),
					account_id,
				),
			}
		}

		fn cf_penalties() -> Vec<(Offence, RuntimeApiPenalty)> {
			pallet_cf_reputation::Penalties::<Runtime>::iter_keys()
				.map(|offence| {
					let penalty = pallet_cf_reputation::Penalties::<Runtime>::get(offence);
					(offence, RuntimeApiPenalty {
						reputation_points: penalty.reputation,
						suspension_duration_blocks: penalty.suspension
					})
				})
				.collect()
		}
		fn cf_suspensions() -> Vec<(Offence, Vec<(u32, AccountId)>)> {
			pallet_cf_reputation::Suspensions::<Runtime>::iter_keys()
				.map(|offence| {
					let suspension = pallet_cf_reputation::Suspensions::<Runtime>::get(offence);
					(offence, suspension.into())
				})
				.collect()
		}
		fn cf_generate_gov_key_call_hash(
			call: Vec<u8>,
		) -> GovCallHash {
			Governance::compute_gov_key_call_hash::<_>(call).0
		}

		fn cf_auction_state() -> AuctionState {
			let auction_params = Validator::auction_parameters();
			AuctionState {
				epoch_duration: Validator::epoch_duration(),
				current_epoch_started_at: Validator::current_epoch_started_at(),
				redemption_period_as_percentage: Validator::redemption_period_as_percentage().deconstruct(),
				min_funding: MinimumFunding::<Runtime>::get().unique_saturated_into(),
				min_bid: pallet_cf_validator::MinimumAuctionBid::<Runtime>::get().unique_saturated_into(),
				auction_size_range: (auction_params.min_size, auction_params.max_size),
				min_active_bid: Validator::resolve_auction_iteratively()
				.ok()
				.map(|(auction_outcome, _)| auction_outcome.bond)
			}
		}

		fn cf_pool_price(
			from: Asset,
			to: Asset,
		) -> Option<PoolPriceV1> {
			LiquidityPools::current_price(from, to)
		}

		fn cf_pool_price_v2(base_asset: Asset, quote_asset: Asset) -> Result<PoolPriceV2, DispatchErrorWithMessage> {
			Ok(
				LiquidityPools::pool_price(base_asset, quote_asset)?
					.map_sell_and_buy_prices(|price| price.sqrt_price)
			)
		}

		/// Simulates a swap and return the intermediate (if any) and final output.
		///
		/// If no swap rate can be calculated, returns None. This can happen if the pools are not
		/// provisioned, or if the input amount amount is too high or too low to give a meaningful
		/// output.
		///
		/// Note: This function must only be called through RPC, because RPC has its own storage buffer
		/// layer and would not affect on-chain storage.
		fn cf_pool_simulate_swap(
			input_asset: Asset,
			output_asset: Asset,
			input_amount: AssetAmount,
			broker_commission: BasisPoints,
			dca_parameters: Option<DcaParameters>,
			ccm_data: Option<CcmData>,
			exclude_fees: BTreeSet<FeeTypes>,
			additional_orders: Option<Vec<SimulateSwapAdditionalOrder>>,
			is_internal: Option<bool>,
		) -> Result<SimulatedSwapInformation, DispatchErrorWithMessage> {
			chainflip::simulate_swap::simulate_swap(
				input_asset,
				output_asset,
				input_amount,
				broker_commission,
				dca_parameters,
				ccm_data,
				exclude_fees,
				additional_orders,
				is_internal,
			)
		}

		fn cf_pool_info(base_asset: Asset, quote_asset: Asset) -> Result<PoolInfo, DispatchErrorWithMessage> {
			LiquidityPools::pool_info(base_asset, quote_asset).map_err(Into::into)
		}

		fn cf_lp_events() -> Vec<pallet_cf_pools::Event<Runtime>> {
			System::read_events_no_consensus().filter_map(|event_record| {
				if let RuntimeEvent::LiquidityPools(pools_event) = event_record.event {
					Some(pools_event)
				} else {
					None
				}
			}).collect()

		}

		fn cf_pool_depth(base_asset: Asset, quote_asset: Asset, tick_range: Range<cf_amm::math::Tick>) -> Result<AskBidMap<UnidirectionalPoolDepth>, DispatchErrorWithMessage> {
			LiquidityPools::pool_depth(base_asset, quote_asset, tick_range).map_err(Into::into)
		}

		fn cf_pool_liquidity(base_asset: Asset, quote_asset: Asset) -> Result<PoolLiquidity, DispatchErrorWithMessage> {
			LiquidityPools::pool_liquidity(base_asset, quote_asset).map_err(Into::into)
		}

		fn cf_required_asset_ratio_for_range_order(
			base_asset: Asset,
			quote_asset: Asset,
			tick_range: Range<cf_amm::math::Tick>,
		) -> Result<PoolPairsMap<Amount>, DispatchErrorWithMessage> {
			LiquidityPools::required_asset_ratio_for_range_order(base_asset, quote_asset, tick_range).map_err(Into::into)
		}

		fn cf_pool_orderbook(
			base_asset: Asset,
			quote_asset: Asset,
			orders: u32,
		) -> Result<PoolOrderbook, DispatchErrorWithMessage> {
			LiquidityPools::pool_orderbook(base_asset, quote_asset, orders).map_err(Into::into)
		}

		fn cf_pool_orders(
			base_asset: Asset,
			quote_asset: Asset,
			lp: Option<AccountId>,
			filled_orders: bool,
		) -> Result<PoolOrders<Runtime>, DispatchErrorWithMessage> {
			LiquidityPools::pool_orders(base_asset, quote_asset, lp, filled_orders).map_err(Into::into)
		}

		fn cf_pool_range_order_liquidity_value(
			base_asset: Asset,
			quote_asset: Asset,
			tick_range: Range<Tick>,
			liquidity: Liquidity,
		) -> Result<PoolPairsMap<Amount>, DispatchErrorWithMessage> {
			LiquidityPools::pool_range_order_liquidity_value(base_asset, quote_asset, tick_range, liquidity).map_err(Into::into)
		}

		fn cf_network_environment() -> NetworkEnvironment {
			Environment::network_environment()
		}

		fn cf_max_swap_amount(asset: Asset) -> Option<AssetAmount> {
			Swapping::maximum_swap_amount(asset)
		}

		fn cf_min_deposit_amount(asset: Asset) -> AssetAmount {
			<chainflip::MinimumDepositProvider as cf_traits::MinimumDeposit>::get(asset)
		}

		fn cf_egress_dust_limit(generic_asset: Asset) -> AssetAmount {
			use pallet_cf_ingress_egress::EgressDustLimit;

			match generic_asset.into() {
				any::ForeignChainAndAsset::Ethereum(asset) => EgressDustLimit::<Runtime, EthereumInstance>::get(asset),
				any::ForeignChainAndAsset::Polkadot(asset) => EgressDustLimit::<Runtime, PolkadotInstance>::get(asset),
				any::ForeignChainAndAsset::Bitcoin(asset) => EgressDustLimit::<Runtime, BitcoinInstance>::get(asset),
				any::ForeignChainAndAsset::Arbitrum(asset) => EgressDustLimit::<Runtime, ArbitrumInstance>::get(asset),
				any::ForeignChainAndAsset::Solana(asset) => EgressDustLimit::<Runtime, SolanaInstance>::get(asset),
				any::ForeignChainAndAsset::Assethub(asset) => EgressDustLimit::<Runtime, AssethubInstance>::get(asset),
				any::ForeignChainAndAsset::Tron(asset) => EgressDustLimit::<Runtime, TronInstance>::get(asset),
			}
		}

		fn cf_ingress_fee(generic_asset: Asset) -> Option<AssetAmount> {
			match generic_asset.into() {
				any::ForeignChainAndAsset::Ethereum(asset) => {
					Some(pallet_cf_swapping::Pallet::<Runtime>::calculate_input_for_gas_output::<Ethereum>(
						asset,
						pallet_cf_chain_tracking::Pallet::<Runtime, EthereumInstance>::estimate_fee(asset, IngressOrEgress::IngressDepositChannel)
					))
				},
				any::ForeignChainAndAsset::Polkadot(asset) => Some(pallet_cf_chain_tracking::Pallet::<Runtime, PolkadotInstance>::estimate_fee(asset, IngressOrEgress::IngressDepositChannel)),
				any::ForeignChainAndAsset::Bitcoin(asset) => Some(pallet_cf_chain_tracking::Pallet::<Runtime, BitcoinInstance>::estimate_fee(asset, IngressOrEgress::IngressDepositChannel).into()),
				any::ForeignChainAndAsset::Arbitrum(asset) => {
					Some(pallet_cf_swapping::Pallet::<Runtime>::calculate_input_for_gas_output::<Arbitrum>(
						asset,
						pallet_cf_chain_tracking::Pallet::<Runtime, ArbitrumInstance>::estimate_fee(asset, IngressOrEgress::IngressDepositChannel)
					))
				},
				any::ForeignChainAndAsset::Solana(asset) => {
					Some(pallet_cf_swapping::Pallet::<Runtime>::calculate_input_for_gas_output::<Solana>(
						asset,
						SolanaChainTrackingProvider::estimate_fee(asset, IngressOrEgress::IngressDepositChannel)
					).into())
				},
				any::ForeignChainAndAsset::Assethub(asset) => {
					Some(pallet_cf_swapping::Pallet::<Runtime>::calculate_input_for_gas_output::<Assethub>(
						asset,
						pallet_cf_chain_tracking::Pallet::<Runtime, AssethubInstance>::estimate_fee(asset, IngressOrEgress::IngressDepositChannel)
					))
				},
				any::ForeignChainAndAsset::Tron(asset) => {
					Some(pallet_cf_swapping::Pallet::<Runtime>::calculate_input_for_gas_output::<Tron>(
						asset,
						DummyTronChainTracking::estimate_fee(asset, IngressOrEgress::IngressDepositChannel)
					))
				},
			}
		}
		fn cf_egress_fee(generic_asset: Asset) -> Option<AssetAmount> {
			match generic_asset.into() {
				any::ForeignChainAndAsset::Ethereum(asset) => {
					Some(pallet_cf_swapping::Pallet::<Runtime>::calculate_input_for_gas_output::<Ethereum>(
						asset,
						pallet_cf_chain_tracking::Pallet::<Runtime, EthereumInstance>::estimate_fee(asset, IngressOrEgress::Egress)
					))
				},
				any::ForeignChainAndAsset::Polkadot(asset) => Some(pallet_cf_chain_tracking::Pallet::<Runtime, PolkadotInstance>::estimate_fee(asset, IngressOrEgress::Egress)),
				any::ForeignChainAndAsset::Bitcoin(asset) => Some(pallet_cf_chain_tracking::Pallet::<Runtime, BitcoinInstance>::estimate_fee(asset, IngressOrEgress::Egress).into()),
				any::ForeignChainAndAsset::Arbitrum(asset) => {
					Some(pallet_cf_swapping::Pallet::<Runtime>::calculate_input_for_gas_output::<Arbitrum>(
						asset,
						pallet_cf_chain_tracking::Pallet::<Runtime, ArbitrumInstance>::estimate_fee(asset, IngressOrEgress::Egress)
					))
				},
				any::ForeignChainAndAsset::Solana(asset) => {
					Some(pallet_cf_swapping::Pallet::<Runtime>::calculate_input_for_gas_output::<Solana>(
						asset,
						SolanaChainTrackingProvider::estimate_fee(asset, IngressOrEgress::Egress)
					).into())
				},
				any::ForeignChainAndAsset::Assethub(asset) => {
					Some(pallet_cf_swapping::Pallet::<Runtime>::calculate_input_for_gas_output::<Assethub>(
						asset,
						pallet_cf_chain_tracking::Pallet::<Runtime, AssethubInstance>::estimate_fee(asset, IngressOrEgress::Egress)
					))
				},
				any::ForeignChainAndAsset::Tron(asset) => {
					Some(pallet_cf_swapping::Pallet::<Runtime>::calculate_input_for_gas_output::<Tron>(
						asset,
						DummyTronChainTracking::estimate_fee(asset, IngressOrEgress::Egress)
					))
				},
			}
		}

		fn cf_witness_safety_margin(chain: ForeignChain) -> Option<u64> {
			match chain {
				ForeignChain::Bitcoin => pallet_cf_ingress_egress::Pallet::<Runtime, BitcoinInstance>::witness_safety_margin(),
				ForeignChain::Ethereum => pallet_cf_ingress_egress::Pallet::<Runtime, EthereumInstance>::witness_safety_margin(),
				ForeignChain::Polkadot => pallet_cf_ingress_egress::Pallet::<Runtime, PolkadotInstance>::witness_safety_margin().map(Into::into),
				ForeignChain::Arbitrum => pallet_cf_ingress_egress::Pallet::<Runtime, ArbitrumInstance>::witness_safety_margin(),
				ForeignChain::Solana => pallet_cf_ingress_egress::Pallet::<Runtime, SolanaInstance>::witness_safety_margin(),
				ForeignChain::Tron => pallet_cf_ingress_egress::Pallet::<Runtime, TronInstance>::witness_safety_margin(),
				ForeignChain::Assethub => pallet_cf_ingress_egress::Pallet::<Runtime, AssethubInstance>::witness_safety_margin().map(Into::into),
			}
		}

		fn cf_liquidity_provider_info(
			account_id: AccountId,
		) -> LiquidityProviderInfo {
			let refund_addresses = ForeignChain::iter().map(|chain| {
				(chain, pallet_cf_lp::LiquidityRefundAddress::<Runtime>::get(&account_id, chain))
			}).collect();

			LiquidityPools::sweep(&account_id).unwrap();

			LiquidityProviderInfo {
				refund_addresses,
				balances: Asset::all().map(|asset|
					(asset, pallet_cf_asset_balances::FreeBalances::<Runtime>::get(&account_id, asset))
				).collect(),
				earned_fees: AssetMap::from_iter(HistoricalEarnedFees::<Runtime>::iter_prefix(&account_id)),
				boost_balances: AssetMap::from_fn(|asset| {
					let pool_details = Self::cf_boost_pool_details(asset);

					pool_details.into_iter().filter_map(|(fee_tier, details)| {
						let available_balance = details.available_amounts.into_iter().find_map(|(id, amount)| {
							if id == account_id {
								Some(amount)
							} else {
								None
							}
						}).unwrap_or(0);

						let owed_amount = details.pending_boosts.into_iter().flat_map(|(_, pending_deposits)| {
							pending_deposits.into_iter().filter_map(|(id, amount)| {
								if id == account_id {
									Some(amount.total)
								} else {
									None
								}
							})
						}).sum();

						let total_balance = available_balance + owed_amount;

						if total_balance == 0 {
							return None
						}

						Some(LiquidityProviderBoostPoolInfo {
							fee_tier,
							total_balance,
							available_balance,
							in_use_balance: owed_amount,
							is_withdrawing: details.pending_withdrawals.keys().any(|id| *id == account_id),
						})
					}).collect()
				}),
				lending_positions: Asset::all()
					.filter_map(|asset| {
						pallet_cf_lending_pools::GeneralLendingPools::<Runtime>::get(asset)
							.and_then(|pool| {
								pool.lender_shares.get(&account_id).map(|share| {
									(*share * pool.total_amount, pool.available_amount)
								})
							})
							.map(|(total_amount, available_amount)| {
								LendingPosition {
									asset,
									total_amount,
									available_amount: core::cmp::min(total_amount, available_amount),
								}
							})
					})
					.collect(),
				collateral_balances: pallet_cf_lending_pools::LoanAccounts::<Runtime>::get(&account_id)
					.map(|loan_account| {
						loan_account.get_total_collateral().iter().map(|(asset, amount)| (*asset, *amount)).collect()
					})
					.unwrap_or_default(),
			}
		}

		fn cf_broker_info(
			account_id: AccountId,
		) -> BrokerInfo<<Bitcoin as Chain>::ChainAccount> {
			use crate::chainflip::address_derivation::btc::derive_btc_vault_deposit_addresses;
			let account_info = pallet_cf_flip::Account::<Runtime>::get(&account_id);
			BrokerInfo {
				earned_fees: Asset::all().map(|asset|
					(asset, AssetBalances::get_balance(&account_id, asset))
				).collect(),
				btc_vault_deposit_address: BrokerPrivateBtcChannels::<Runtime>::get(&account_id)
					.map(|channel| derive_btc_vault_deposit_addresses(channel).current),
				affiliates: pallet_cf_swapping::AffiliateAccountDetails::<Runtime>::iter_prefix(&account_id).collect(),
				bond: account_info.bond(),
				bound_fee_withdrawal_address: Swapping::bound_broker_withdrawal_address(account_id),
			}
		}

		fn cf_account_role(account_id: AccountId) -> Option<AccountRole> {
			pallet_cf_account_roles::AccountRoles::<Runtime>::get(account_id)
		}

		fn cf_redemption_tax() -> AssetAmount {
			pallet_cf_funding::RedemptionTax::<Runtime>::get()
		}

		fn cf_swap_retry_delay_blocks() -> u32 {
			pallet_cf_swapping::SwapRetryDelay::<Runtime>::get()
		}

		fn cf_swap_limits() -> SwapLimits {
			pallet_cf_swapping::Pallet::<Runtime>::get_swap_limits()
		}

		fn cf_minimum_chunk_size(asset: Asset) -> AssetAmount {
			Swapping::minimum_chunk_size(asset)
		}

		fn cf_scheduled_swaps(base_asset: Asset, quote_asset: Asset) -> Vec<(SwapLegInfo, BlockNumber)> {
			assert_eq!(quote_asset, STABLE_ASSET, "Only USDC is supported as quote asset");
			Swapping::get_scheduled_swap_legs(base_asset)
		}

		fn cf_failed_call_ethereum(broadcast_id: BroadcastId) -> Option<<cf_chains::Ethereum as cf_chains::Chain>::Transaction> {
			if EthereumIngressEgress::get_failed_call(broadcast_id).is_some() {
				EthereumBroadcaster::threshold_signature_data(broadcast_id).map(|api_call|{
					chainflip::EthTransactionBuilder::build_transaction(&api_call)
				})
			} else {
				None
			}
		}

		fn cf_failed_call_arbitrum(broadcast_id: BroadcastId) -> Option<<cf_chains::Arbitrum as cf_chains::Chain>::Transaction> {
			if ArbitrumIngressEgress::get_failed_call(broadcast_id).is_some() {
				ArbitrumBroadcaster::threshold_signature_data(broadcast_id).map(|api_call|{
					chainflip::ArbTransactionBuilder::build_transaction(&api_call)
				})
			} else {
				None
			}
		}

		fn cf_failed_call_tron(broadcast_id: BroadcastId) -> Option<<cf_chains::Tron as cf_chains::Chain>::Transaction> {
			if TronIngressEgress::get_failed_call(broadcast_id).is_some() {
				TronBroadcaster::threshold_signature_data(broadcast_id).map(|api_call|{
					chainflip::TronTransactionBuilder::build_transaction(&api_call)
				})
			} else {
				None
			}
		}

		fn cf_witness_count(hash: pallet_cf_witnesser::CallHash, epoch_index: Option<EpochIndex>) -> Option<FailingWitnessValidators> {
			let mut result: FailingWitnessValidators = FailingWitnessValidators {
				failing_count: 0,
				validators: vec![],
			};
			let voting_validators = Witnesser::count_votes(epoch_index.unwrap_or(<Runtime as Chainflip>::EpochInfo::current_epoch()), hash);
			let vanity_names: BTreeMap<AccountId, BoundedVec<u8, _>> = pallet_cf_account_roles::VanityNames::<Runtime>::get();
			voting_validators?.iter().for_each(|(val, voted)| {
				let vanity = vanity_names.get(val).cloned().unwrap_or_default();
				if !voted {
					result.failing_count += 1;
				}
				result.validators.push((val.clone(), String::from_utf8_lossy(&vanity).into(), *voted));
			});

			Some(result)
		}

		fn cf_channel_opening_fee(chain: ForeignChain) -> FlipBalance {
			match chain {
				ForeignChain::Ethereum => pallet_cf_ingress_egress::Pallet::<Runtime, EthereumInstance>::channel_opening_fee(),
				ForeignChain::Polkadot => pallet_cf_ingress_egress::Pallet::<Runtime, PolkadotInstance>::channel_opening_fee(),
				ForeignChain::Bitcoin => pallet_cf_ingress_egress::Pallet::<Runtime, BitcoinInstance>::channel_opening_fee(),
				ForeignChain::Arbitrum => pallet_cf_ingress_egress::Pallet::<Runtime, ArbitrumInstance>::channel_opening_fee(),
				ForeignChain::Solana => pallet_cf_ingress_egress::Pallet::<Runtime, SolanaInstance>::channel_opening_fee(),
				ForeignChain::Assethub => pallet_cf_ingress_egress::Pallet::<Runtime, AssethubInstance>::channel_opening_fee(),
				ForeignChain::Tron => pallet_cf_ingress_egress::Pallet::<Runtime, TronInstance>::channel_opening_fee(),
			}
		}

		fn cf_ingress_delay(chain: ForeignChain) -> u32 {
			match chain {
				ForeignChain::Ethereum => pallet_cf_ingress_egress::IngressDelayBlocks::<Runtime, EthereumInstance>::get(),
				ForeignChain::Polkadot => pallet_cf_ingress_egress::IngressDelayBlocks::<Runtime, PolkadotInstance>::get(),
				ForeignChain::Bitcoin => pallet_cf_ingress_egress::IngressDelayBlocks::<Runtime, BitcoinInstance>::get(),
				ForeignChain::Arbitrum => pallet_cf_ingress_egress::IngressDelayBlocks::<Runtime, ArbitrumInstance>::get(),
				ForeignChain::Solana => pallet_cf_ingress_egress::IngressDelayBlocks::<Runtime, SolanaInstance>::get(),
				ForeignChain::Assethub => pallet_cf_ingress_egress::IngressDelayBlocks::<Runtime, AssethubInstance>::get(),
				ForeignChain::Tron => pallet_cf_ingress_egress::IngressDelayBlocks::<Runtime, TronInstance>::get(),
			}
		}

		fn cf_boost_delay(chain: ForeignChain) -> u32 {
			match chain {
				ForeignChain::Ethereum => pallet_cf_ingress_egress::BoostDelayBlocks::<Runtime, EthereumInstance>::get(),
				ForeignChain::Polkadot => pallet_cf_ingress_egress::BoostDelayBlocks::<Runtime, PolkadotInstance>::get(),
				ForeignChain::Bitcoin => pallet_cf_ingress_egress::BoostDelayBlocks::<Runtime, BitcoinInstance>::get(),
				ForeignChain::Arbitrum => pallet_cf_ingress_egress::BoostDelayBlocks::<Runtime, ArbitrumInstance>::get(),
				ForeignChain::Solana => pallet_cf_ingress_egress::BoostDelayBlocks::<Runtime, SolanaInstance>::get(),
				ForeignChain::Assethub => pallet_cf_ingress_egress::BoostDelayBlocks::<Runtime, AssethubInstance>::get(),
				ForeignChain::Tron => pallet_cf_ingress_egress::BoostDelayBlocks::<Runtime, TronInstance>::get(),
			}
		}

		fn cf_boost_config() -> BoostConfiguration {
			pallet_cf_lending_pools::BoostConfig::<Runtime>::get()
		}

		fn cf_boost_pools_depth() -> Vec<BoostPoolDepth> {

			pallet_cf_lending_pools::boost_pools_iter::<Runtime>().map(|(asset, tier, core_pool)| {

				BoostPoolDepth {
					asset,
					tier,
					available_amount: core_pool.get_available_amount()
				}

			}).collect()

		}

		fn cf_boost_pool_details(asset: Asset) -> BTreeMap<u16, BoostPoolDetails<AccountId>> {
			pallet_cf_lending_pools::get_boost_pool_details::<Runtime>(asset)
		}

		fn cf_safe_mode_statuses() -> RuntimeSafeMode {
			pallet_cf_environment::RuntimeSafeMode::<Runtime>::get()
		}

		fn cf_pools() -> Vec<PoolPairsMap<Asset>> {
			LiquidityPools::pools()
		}

		fn cf_validate_dca_params(number_of_chunks: u32, chunk_interval: u32) -> Result<(), DispatchErrorWithMessage> {
			pallet_cf_swapping::Pallet::<Runtime>::validate_dca_params(&DcaParameters{number_of_chunks, chunk_interval}).map_err(Into::into)
		}

		fn cf_validate_refund_params(
			input_asset: Asset,
			output_asset: Asset,
			retry_duration: BlockNumber,
			max_oracle_price_slippage: Option<BasisPoints>,
		) -> Result<(), DispatchErrorWithMessage> {
			pallet_cf_swapping::Pallet::<Runtime>::validate_refund_params(
				input_asset,
				output_asset,
				retry_duration,
				max_oracle_price_slippage,
			)
			.map_err(Into::into)
		}

		fn cf_request_swap_parameter_encoding(
			broker: AccountId,
			source_asset: Asset,
			destination_asset: Asset,
			destination_address: EncodedAddress,
			broker_commission: BasisPoints,
			extra_parameters: VaultSwapExtraParametersEncoded,
			channel_metadata: Option<CcmChannelMetadataUnchecked>,
			boost_fee: BasisPoints,
			affiliate_fees: Affiliates<AccountId>,
			dca_parameters: Option<DcaParameters>,
		) -> Result<VaultSwapDetails<<Bitcoin as Chain>::ChainAccount>, DispatchErrorWithMessage> {
			let source_chain = ForeignChain::from(source_asset);
			let destination_chain = ForeignChain::from(destination_asset);


			// Validate refund params
			let (retry_duration, max_oracle_price_slippage) = match &extra_parameters {
				VaultSwapExtraParametersEncoded::Bitcoin { retry_duration, max_oracle_price_slippage, .. } => {
					let max_oracle_price_slippage = match max_oracle_price_slippage {
						Some(slippage) if *slippage == u8::MAX => None,
						Some(slippage) => Some((*slippage).into()),
						None => None,
					};
					(*retry_duration, max_oracle_price_slippage)
				},
				VaultSwapExtraParametersEncoded::Ethereum(EvmVaultSwapExtraParameters { refund_parameters, .. }) => {
					refund_parameters.clone().try_map_refund_address_to_foreign_chain_address::<ChainAddressConverter>()?.into_checked(None, source_asset)?;
					(refund_parameters.retry_duration, refund_parameters.max_oracle_price_slippage)
				},
				VaultSwapExtraParametersEncoded::Arbitrum(EvmVaultSwapExtraParameters { refund_parameters, .. }) => {
					refund_parameters.clone().try_map_refund_address_to_foreign_chain_address::<ChainAddressConverter>()?.into_checked(None, source_asset)?;
					(refund_parameters.retry_duration, refund_parameters.max_oracle_price_slippage)
				},
				VaultSwapExtraParametersEncoded::Solana { refund_parameters, .. } => {
					refund_parameters.clone().try_map_refund_address_to_foreign_chain_address::<ChainAddressConverter>()?.into_checked(None, source_asset)?;
					(refund_parameters.retry_duration, refund_parameters.max_oracle_price_slippage)
				},
			};

			let checked_ccm = crate::chainflip::vault_swaps::validate_parameters(
				&broker,
				source_asset,
				&destination_address,
				destination_asset,
				&dca_parameters,
				boost_fee,
				broker_commission,
				&affiliate_fees,
				retry_duration,
				&channel_metadata,
				max_oracle_price_slippage,
			)?;

			// Conversion implicitly verifies address validity.
			frame_support::ensure!(
				ChainAddressConverter::try_from_encoded_address(destination_address.clone())
					.map_err(|_| pallet_cf_swapping::Error::<Runtime>::InvalidDestinationAddress)?
					.chain() == destination_chain
				,
				"Destination address and asset are on different chains."
			);

			// Convert boost fee.
			let boost_fee: u8 = boost_fee
				.try_into()
				.map_err(|_| pallet_cf_swapping::Error::<Runtime>::BoostFeeTooHigh)?;

			// Validate broker fee
			if broker_commission < pallet_cf_swapping::Pallet::<Runtime>::get_minimum_vault_swap_fee_for_broker(&broker) {
				return Err(DispatchErrorWithMessage::from("Broker commission is too low"));
			}
			let _beneficiaries = pallet_cf_swapping::Pallet::<Runtime>::assemble_and_validate_broker_fees(
				broker.clone(),
				broker_commission,
				affiliate_fees.clone(),
			)?;

			// Encode swap
			match (source_chain, extra_parameters) {
				(
					ForeignChain::Bitcoin,
					VaultSwapExtraParameters::Bitcoin {
						min_output_amount,
						retry_duration,
						max_oracle_price_slippage,
					}
				) => {
					crate::chainflip::vault_swaps::bitcoin_vault_swap(
						broker,
						destination_asset,
						destination_address,
						broker_commission,
						min_output_amount,
						retry_duration,
						boost_fee,
						affiliate_fees,
						dca_parameters,
						max_oracle_price_slippage,
					)
				},
				(
					ForeignChain::Ethereum,
					VaultSwapExtraParametersEncoded::Ethereum(extra_params)
				)|
				(
					ForeignChain::Arbitrum,
					VaultSwapExtraParametersEncoded::Arbitrum(extra_params)
				) => {
					crate::chainflip::vault_swaps::evm_vault_swap(
						broker,
						source_asset,
						extra_params.input_amount,
						destination_asset,
						destination_address,
						broker_commission,
						extra_params.refund_parameters,
						boost_fee,
						affiliate_fees,
						dca_parameters,
						checked_ccm,
					)
				},
				(
					ForeignChain::Solana,
					VaultSwapExtraParameters::Solana {
						from,
						seed,
						input_amount,
						refund_parameters,
						from_token_account,
					}
				) => crate::chainflip::vault_swaps::solana_vault_swap(
					broker,
					input_amount,
					source_asset,
					destination_asset,
					destination_address,
					broker_commission,
					refund_parameters,
					checked_ccm,
					boost_fee,
					affiliate_fees,
					dca_parameters,
					from,
					seed,
					from_token_account,
				),
				_ => Err(DispatchErrorWithMessage::from(
					"Incompatible or unsupported source_asset and extra_parameters"
				)),
			}
		}

		fn cf_decode_vault_swap_parameter(
			broker: AccountId,
			vault_swap: VaultSwapDetails<String>,
		) -> Result<VaultSwapInputEncoded, DispatchErrorWithMessage> {
			match vault_swap {
				VaultSwapDetails::Bitcoin {
					nulldata_payload,
					deposit_address: _,
				} => {
					crate::chainflip::vault_swaps::decode_bitcoin_vault_swap(
						broker,
						nulldata_payload,
					)
				},
				VaultSwapDetails::Solana {
					instruction,
				} => {
					crate::chainflip::vault_swaps::decode_solana_vault_swap(
						instruction.into(),
					)
				},
				_ => Err(DispatchErrorWithMessage::from(
					"Decoding Vault Swap only supports Bitcoin and Solana"
				)),
			}
		}

		fn cf_encode_cf_parameters(
			broker: AccountId,
			source_asset: Asset,
			destination_address: EncodedAddress,
			destination_asset: Asset,
			refund_parameters: ChannelRefundParametersUncheckedEncoded,
			dca_parameters: Option<DcaParameters>,
			boost_fee: BasisPoints,
			broker_commission: BasisPoints,
			affiliate_fees: Affiliates<AccountId>,
			channel_metadata: Option<CcmChannelMetadataUnchecked>,
		) -> Result<Vec<u8>, DispatchErrorWithMessage> {
			// Validate the parameters
			let checked_ccm = crate::chainflip::vault_swaps::validate_parameters(
				&broker,
				source_asset,
				&destination_address,
				destination_asset,
				&dca_parameters,
				boost_fee,
				broker_commission,
				&affiliate_fees,
				refund_parameters.retry_duration,
				&channel_metadata,
				refund_parameters.max_oracle_price_slippage,
			)?;

			let boost_fee: u8 = boost_fee
				.try_into()
				.map_err(|_| pallet_cf_swapping::Error::<Runtime>::BoostFeeTooHigh)?;

			let affiliate_and_fees = crate::chainflip::vault_swaps::to_affiliate_and_fees(&broker, affiliate_fees)?
				.try_into()
				.map_err(|_| "Too many affiliates.")?;

			macro_rules! build_and_encode_cf_parameters_for_chain {
				($chain:ty) => {
					build_and_encode_cf_parameters::<<$chain as cf_chains::Chain>::ChainAccount>(
						refund_parameters.try_map_address(|addr| {
							Ok::<_, DispatchErrorWithMessage>(
								ChainAddressConverter::try_from_encoded_address(addr)
									.and_then(|addr| addr.try_into().map_err(|_| ()))
									.map_err(|_| "Invalid refund address")?,
							)
						})?,
						dca_parameters,
						boost_fee,
						broker,
						broker_commission,
						affiliate_and_fees,
						checked_ccm.as_ref(),
					)
				}
			}

			Ok(match ForeignChain::from(source_asset) {
				ForeignChain::Ethereum => build_and_encode_cf_parameters_for_chain!(Ethereum),
				ForeignChain::Arbitrum => build_and_encode_cf_parameters_for_chain!(Arbitrum),
				ForeignChain::Tron => build_and_encode_cf_parameters_for_chain!(Tron),
				ForeignChain::Solana => build_and_encode_cf_parameters_for_chain!(Solana),
				_ => Err(DispatchErrorWithMessage::from("Unsupported source chain for encoding cf_parameters"))?,
			})
		}

		fn cf_get_preallocated_deposit_channels(account_id: <Runtime as frame_system::Config>::AccountId, chain: ForeignChain) -> Vec<ChannelId> {

			fn preallocated_deposit_channels_for_chain<T: pallet_cf_ingress_egress::Config<I>, I: 'static>(
				account_id: &<T as frame_system::Config>::AccountId,
			) -> Vec<ChannelId>
			{
				pallet_cf_ingress_egress::PreallocatedChannels::<T, I>::get(account_id).iter()
					.map(|channel| channel.channel_id)
					.collect()
			}

			match chain {
				ForeignChain::Bitcoin => preallocated_deposit_channels_for_chain::<Runtime, BitcoinInstance>(&account_id),
				ForeignChain::Ethereum => preallocated_deposit_channels_for_chain::<Runtime, EthereumInstance>(&account_id),
				ForeignChain::Polkadot => preallocated_deposit_channels_for_chain::<Runtime, PolkadotInstance>(&account_id),
				ForeignChain::Arbitrum => preallocated_deposit_channels_for_chain::<Runtime, ArbitrumInstance>(&account_id),
				ForeignChain::Solana => preallocated_deposit_channels_for_chain::<Runtime, SolanaInstance>(&account_id),
				ForeignChain::Assethub => preallocated_deposit_channels_for_chain::<Runtime, AssethubInstance>(&account_id),
				ForeignChain::Tron => preallocated_deposit_channels_for_chain::<Runtime, TronInstance>(&account_id),
			}
		}

		fn cf_get_open_deposit_channels(account_id: Option<<Runtime as frame_system::Config>::AccountId>) -> ChainAccounts {
			fn open_deposit_channels_for_account<T: pallet_cf_ingress_egress::Config<I>, I: 'static>(
				account_id: Option<&<T as frame_system::Config>::AccountId>
			) -> Vec<(EncodedAddress, Asset)>
			{
				let network_environment = Environment::network_environment();
				pallet_cf_ingress_egress::DepositChannelLookup::<T, I>::iter_values()
					.filter(|channel_details| account_id.is_none() || Some(&channel_details.owner) == account_id)
					.map(|channel_details|
						(
							channel_details.deposit_channel.address
								.into_foreign_chain_address()
								.to_encoded_address(network_environment),
							channel_details.deposit_channel.asset.into()
						)
					)
					.collect::<Vec<_>>()
			}

			ChainAccounts {
				chain_accounts: [
					open_deposit_channels_for_account::<Runtime, BitcoinInstance>(account_id.as_ref()),
					open_deposit_channels_for_account::<Runtime, EthereumInstance>(account_id.as_ref()),
					open_deposit_channels_for_account::<Runtime, ArbitrumInstance>(account_id.as_ref()),
					open_deposit_channels_for_account::<Runtime, SolanaInstance>(account_id.as_ref()),
					open_deposit_channels_for_account::<Runtime, TronInstance>(account_id.as_ref()),
				].into_iter().flatten().collect()
			}
		}

		fn cf_all_open_deposit_channels() -> Vec<OpenedDepositChannels> {
			use sp_std::collections::btree_set::BTreeSet;

			#[expect(clippy::type_complexity)]
			fn open_deposit_channels_for_chain_instance<T: pallet_cf_ingress_egress::Config<I>, I: 'static>()
				-> BTreeMap<(<T as frame_system::Config>::AccountId, ChannelActionType), Vec<(EncodedAddress, Asset)>>
			{
				let network_environment = Environment::network_environment();
				pallet_cf_ingress_egress::DepositChannelLookup::<T, I>::iter_values()
					.fold(BTreeMap::new(), |mut acc, channel_details| {
						acc.entry((channel_details.owner.clone(), channel_details.action.into()))
							.or_default()
							.push(
								(
									channel_details.deposit_channel.address
										.into_foreign_chain_address()
										.to_encoded_address(network_environment),
									channel_details.deposit_channel.asset.into()
								)
							);
						acc
					})
			}

			let btc_chain_accounts = open_deposit_channels_for_chain_instance::<Runtime, BitcoinInstance>();
			let eth_chain_accounts = open_deposit_channels_for_chain_instance::<Runtime, EthereumInstance>();
			let arb_chain_accounts = open_deposit_channels_for_chain_instance::<Runtime, ArbitrumInstance>();
			let sol_chain_accounts = open_deposit_channels_for_chain_instance::<Runtime, SolanaInstance>();
			let accounts = btc_chain_accounts.keys()
				.chain(eth_chain_accounts.keys())
				.chain(arb_chain_accounts.keys())
				.chain(sol_chain_accounts.keys())
				.cloned().collect::<BTreeSet<_>>();

			accounts.into_iter().map(|key| {
				let (account_id, channel_action_type) = key.clone();
				(account_id, channel_action_type, ChainAccounts {
					chain_accounts: [
						btc_chain_accounts.get(&key).cloned().unwrap_or_default(),
						eth_chain_accounts.get(&key).cloned().unwrap_or_default(),
						arb_chain_accounts.get(&key).cloned().unwrap_or_default(),
						sol_chain_accounts.get(&key).cloned().unwrap_or_default(),
					].into_iter().flatten().collect()
				})
			}).collect()
		}

		fn cf_transaction_screening_events() -> TransactionScreeningEvents {
			fn extract_screening_events<
				T: pallet_cf_ingress_egress::Config<I, AccountId = <Runtime as frame_system::Config>::AccountId>,
				I: 'static
			>(
				event: pallet_cf_ingress_egress::Event::<T, I>,
			) -> Vec<BrokerRejectionEventFor<T::TargetChain>> {
				match event {
					pallet_cf_ingress_egress::Event::TransactionRejectionRequestExpired { account_id, tx_id } =>
						vec![TransactionScreeningEvent::TransactionRejectionRequestExpired { account_id, tx_id }],
					pallet_cf_ingress_egress::Event::TransactionRejectionRequestReceived { account_id, tx_id, expires_at: _ } =>
						vec![TransactionScreeningEvent::TransactionRejectionRequestReceived { account_id, tx_id }],
					pallet_cf_ingress_egress::Event::TransactionRejectedByBroker { broadcast_id, tx_id: deposit_details } =>
						vec![TransactionScreeningEvent::TransactionRejectedByBroker { refund_broadcast_id: broadcast_id, deposit_details }],
					pallet_cf_ingress_egress::Event::ChannelRejectionRequestReceived { account_id, deposit_address } =>
						vec![TransactionScreeningEvent::ChannelRejectionRequestReceived { account_id, deposit_address }],
					_ => Default::default(),
				}
			}

			let mut btc_events: Vec<BrokerRejectionEventFor<cf_chains::Bitcoin>> = Default::default();
			let mut eth_events: Vec<BrokerRejectionEventFor<cf_chains::Ethereum>> = Default::default();
			let mut arb_events: Vec<BrokerRejectionEventFor<cf_chains::Arbitrum>> = Default::default();
			let mut sol_events: Vec<BrokerRejectionEventFor<cf_chains::Solana>> = Default::default();
			for event_record in System::read_events_no_consensus() {
				match event_record.event {
					RuntimeEvent::BitcoinIngressEgress(event) => btc_events.extend(extract_screening_events::<Runtime, BitcoinInstance>(event)),
					RuntimeEvent::EthereumIngressEgress(event) => eth_events.extend(extract_screening_events::<Runtime, EthereumInstance>(event)),
					RuntimeEvent::ArbitrumIngressEgress(event) => arb_events.extend(extract_screening_events::<Runtime, ArbitrumInstance>(event)),
					RuntimeEvent::SolanaIngressEgress(event) => sol_events.extend(extract_screening_events::<Runtime, SolanaInstance>(event)),
					_ => {},
				}
			}

			TransactionScreeningEvents {
				btc_events,
				eth_events,
				arb_events,
				sol_events,
			}
		}

		fn cf_affiliate_details(
			broker: AccountId,
			affiliate: Option<AccountId>,
		) -> Vec<(AccountId, AffiliateDetails)>{
			if let Some(affiliate) = affiliate {
				pallet_cf_swapping::AffiliateAccountDetails::<Runtime>::get(&broker, &affiliate)
					.map(|details| (affiliate, details))
					.into_iter()
					.collect()
			} else {
				pallet_cf_swapping::AffiliateAccountDetails::<Runtime>::iter_prefix(&broker).collect()
			}
		}

		fn cf_vault_addresses() -> VaultAddresses {
			use crate::chainflip::address_derivation::btc::{
				derive_btc_vault_deposit_addresses,
				BitcoinPrivateBrokerDepositAddresses
			};
			use cf_chains::btc::deposit_address::DepositAddress;

			let bitcoin_agg_key = <BtcEnvironment as ChainEnvironment<_, cf_chains::btc::AggKey>>::lookup(());
			let solana_api_environment = Environment::solana_api_environment();
			VaultAddresses {
				ethereum: EncodedAddress::Eth(Environment::eth_vault_address().into()),
				arbitrum: EncodedAddress::Arb(Environment::arb_vault_address().into()),
				bitcoin: BrokerPrivateBtcChannels::<Runtime>::iter()
					.map(|(account_id, channel_id)| {
						let BitcoinPrivateBrokerDepositAddresses { previous: _, current } = derive_btc_vault_deposit_addresses(channel_id)
							.with_encoded_addresses();
						(account_id, current)
					})
					.collect(),

				sol_vault_program: solana_api_environment.vault_program.into(),
				sol_swap_endpoint_program_data_account: solana_api_environment.swap_endpoint_program_data_account.into(),
				usdc_token_mint_pubkey: Environment::solana_api_environment().usdc_token_mint_pubkey.into(),
				usdt_token_mint_pubkey: Environment::solana_api_environment().usdt_token_mint_pubkey.into(),
				solana_sol_vault: <SolEnvironment as ChainEnvironment<_, SolAddress>>::lookup(cf_chains::sol::api::CurrentAggKey).map(Into::into),
				solana_usdc_token_vault_ata: solana_api_environment.usdc_token_vault_ata.into(),
				solana_usdt_token_vault_ata: solana_api_environment.usdt_token_vault_ata.into(),
				solana_vault_swap_account: sol_prim::address_derivation::derive_swap_endpoint_native_vault_account(
					solana_api_environment.swap_endpoint_program
				).ok().map(|account| account.address.into()),
				bitcoin_vault: bitcoin_agg_key.map(|agg_key| {
					let vault_address = DepositAddress::new(agg_key.current, 0);
					EncodedAddress::from_chain_account::<Bitcoin>(
						vault_address.script_pubkey(),
						Environment::network_environment(),
					)
				}),
				predicted_seconds_until_next_vault_rotation: {
					let started = pallet_cf_validator::CurrentEpochStartedAt::<Runtime>::get();
					let duration = pallet_cf_validator::EpochDuration::<Runtime>::get();
					let current_height = crate::System::block_number();
					let blocks_left = started.saturating_add(duration).saturating_sub(current_height);
					blocks_left as u64 * cf_primitives::SECONDS_PER_BLOCK
				}
			}
		}

		fn cf_get_trading_strategies(lp_id: Option<AccountId>) -> Vec<TradingStrategyInfo<AssetAmount>> {
			type Strategies = pallet_cf_trading_strategy::Strategies::<Runtime>;
			type Strategy = pallet_cf_trading_strategy::TradingStrategy;

			fn to_strategy_info(lp_id: AccountId, strategy_id: AccountId, strategy: Strategy) -> TradingStrategyInfo<AssetAmount> {

				let free_balances = AssetBalances::free_balances(&strategy_id);
				let open_order_balances = LiquidityPools::open_order_balances(&strategy_id);

				let total_balances = free_balances.saturating_add(open_order_balances);

				let supported_assets = strategy.supported_assets();
				let supported_asset_balances = total_balances.iter()
					.filter(|(asset, _amount)| supported_assets.contains(asset))
					.map(|(asset, amount)| (asset, *amount));

				TradingStrategyInfo {
					lp_id,
					strategy_id,
					strategy,
					balance: supported_asset_balances.collect(),
				}

			}

			if let Some(lp_id) = &lp_id {
				Strategies::iter_prefix(lp_id).map(|(strategy_id, strategy)| to_strategy_info(lp_id.clone(), strategy_id, strategy)).collect()
			} else {
				Strategies::iter().map(|(lp_id, strategy_id, strategy)| to_strategy_info(lp_id, strategy_id, strategy)).collect()
			}

		}

		fn cf_trading_strategy_limits() -> TradingStrategyLimits{
			TradingStrategyLimits{
				minimum_deployment_amount: AssetMap::from_iter(pallet_cf_trading_strategy::MinimumDeploymentAmountForStrategy::<Runtime>::get().into_iter()
					.map(|(asset, balance)| (asset, Some(balance)))),
				minimum_added_funds_amount: AssetMap::from_iter(pallet_cf_trading_strategy::MinimumAddedFundsToStrategy::<Runtime>::get().into_iter()
					.map(|(asset, balance)| (asset, Some(balance)))),
			}
		}

		fn cf_network_fees() -> NetworkFees{
			let regular_network_fee = pallet_cf_swapping::NetworkFee::<Runtime>::get();
			let internal_swap_network_fee = pallet_cf_swapping::InternalSwapNetworkFee::<Runtime>::get();
			NetworkFees {
				regular_network_fee: NetworkFeeDetails{
					rates: AssetMap::from_fn(|asset|{
						pallet_cf_swapping::NetworkFeeForAsset::<Runtime>::get(asset).unwrap_or(regular_network_fee.rate)
					}),
					standard_rate_and_minimum: regular_network_fee,
				},
				internal_swap_network_fee: NetworkFeeDetails{
					rates: AssetMap::from_fn(|asset|{
						pallet_cf_swapping::InternalSwapNetworkFeeForAsset::<Runtime>::get(asset).unwrap_or(internal_swap_network_fee.rate)
					}),
					standard_rate_and_minimum: internal_swap_network_fee,
				},
			}
		}

		fn cf_oracle_prices(base_and_quote_asset: Option<(PriceAsset, PriceAsset)>,) -> Vec<OraclePrice> {
			if let Some(state) = pallet_cf_elections::ElectoralUnsynchronisedState::<Runtime, ()>::get() {
				get_latest_oracle_prices(&state.0, base_and_quote_asset)
			} else {
				vec![]
			}
		}

		fn cf_lending_pools(asset: Option<Asset>) -> Vec<RpcLendingPool<AssetAmount>> {
			pallet_cf_lending_pools::get_lending_pools::<Runtime>(asset)
		}

		fn cf_loan_accounts(borrower_id: Option<AccountId>) -> Vec<RpcLoanAccount<AccountId, AssetAmount>> {
			pallet_cf_lending_pools::get_loan_accounts::<Runtime>(borrower_id)
		}

		fn cf_lending_pool_supply_balances(
			asset: Option<Asset>,
		) -> Vec<LendingPoolAndSupplyPositions<AccountId, AssetAmount>> {

			if let Some(asset) = asset {
				pallet_cf_lending_pools::GeneralLendingPools::<Runtime>::get(asset).map(|pool| {
					pool.get_all_supply_positions()
				}).into_iter().map(|positions| LendingPoolAndSupplyPositions { asset, positions }).collect()
			} else {
				pallet_cf_lending_pools::GeneralLendingPools::<Runtime>::iter().map(|(asset, pool)| {
					LendingPoolAndSupplyPositions { asset, positions: pool.get_all_supply_positions() }
				}).collect()
			}
		}

		fn cf_lending_config() -> RpcLendingConfig {
			let config = pallet_cf_lending_pools::LendingConfig::<Runtime>::get();
			RpcLendingConfig {
				ltv_thresholds: config.ltv_thresholds,
				network_fee_contributions: config.network_fee_contributions,
				fee_swap_interval_blocks: config.fee_swap_interval_blocks,
				interest_payment_interval_blocks: config.interest_payment_interval_blocks,
				fee_swap_threshold_usd: config.fee_swap_threshold_usd.into(),
				interest_collection_threshold_usd: config.interest_collection_threshold_usd.into(),
				soft_liquidation_swap_chunk_size_usd: config.soft_liquidation_swap_chunk_size_usd.into(),
				hard_liquidation_swap_chunk_size_usd: config.hard_liquidation_swap_chunk_size_usd.into(),
				soft_liquidation_max_oracle_slippage: config.soft_liquidation_max_oracle_slippage,
				hard_liquidation_max_oracle_slippage: config.hard_liquidation_max_oracle_slippage,
				fee_swap_max_oracle_slippage: config.fee_swap_max_oracle_slippage,
				minimum_loan_amount_usd: config.minimum_loan_amount_usd.into(),
				minimum_supply_amount_usd: config.minimum_supply_amount_usd.into(),
				minimum_update_loan_amount_usd: config.minimum_update_loan_amount_usd.into(),
				minimum_update_collateral_amount_usd: config.minimum_update_collateral_amount_usd.into(),
			}
		}

		fn cf_evm_calldata(
			caller: EthereumAddress,
			call: EthereumSCApi<FlipBalance>,
		) -> Result<EvmCallDetails, DispatchErrorWithMessage> {
			use cf_chains::evm::api::sc_utils::deposit_flip_to_sc_gateway_and_call::{DepositToSCGatewayAndCall};
			use cf_chains::evm::api::sc_utils::sc_call::SCCall;

			let caller_id = EthereumAccount(caller).into_account_id();
			let required_deposit = match call {
				EthereumSCApi::Delegation { call: DelegationApi::Delegate { increase: DelegationAmount::Some(ref increase), .. } } => {
					pallet_cf_validator::DelegationChoice::<Runtime>::get(&caller_id).map(|(_, bid)| bid).unwrap_or_default()
						.saturating_add(*increase)
						.saturating_sub(pallet_cf_flip::Pallet::<Runtime>::balance(&caller_id))
				},
				_ => 0,
			};
			Ok(EvmCallDetails {
				calldata: if required_deposit > 0 {
					DepositToSCGatewayAndCall::new(required_deposit, call.encode()).abi_encoded_payload()
				} else {
					SCCall::new(call.encode()).abi_encoded_payload()
				},
				value: U256::zero(),
				to: Environment::eth_sc_utils_address(),
				source_token_address: if required_deposit > 0 {
					Some(
						Environment::supported_eth_assets(cf_primitives::chains::assets::eth::Asset::Flip)
							.ok_or(DispatchErrorWithMessage::from(
								"flip token address not found on the state chain: {e}",
							))?
					)
				} else {
					None
				},
			})
		}
		fn cf_active_delegations(operator: Option<AccountId>) -> Vec<DelegationSnapshot<AccountId, FlipBalance>> {
			let current_epoch = Validator::current_epoch();
			if let Some(account_id) = operator {
				pallet_cf_validator::DelegationSnapshots::<Runtime>::get(current_epoch, &account_id).into_iter().collect()
			} else {
				pallet_cf_validator::DelegationSnapshots::<Runtime>::iter_prefix_values(current_epoch)
					.collect()
			}
		}

		fn cf_encode_non_native_call(
			call: Vec<u8>,
			blocks_to_expiry: BlockNumber,
			nonce_or_account: NonceOrAccount,
			encoding: EncodingType,
		) -> Result<(EncodedNonNativeCall, TransactionMetadata), DispatchErrorWithMessage> {
			use pallet_cf_environment::{EthEncodingType, SolEncodingType, build_domain_data, DOMAIN_OFFCHAIN_PREFIX};
			use pallet_cf_environment::submit_runtime_call::ChainflipExtrinsic;
			use ethereum_eip712::build_eip712_data::build_eip712_typed_data;
			use ethereum_eip712::eip712::TypedData;

			let spec_version = <Runtime as frame_system::Config>::Version::get().spec_version;
			let current_block_number = <frame_system::Pallet<Runtime>>::block_number();
			let chainflip_network = <pallet_cf_environment::ChainflipNetworkName::<Runtime>>::get();

			// Ensure it is a valid RuntimeCall
			let runtime_call =
				match RuntimeCall::decode(&mut &call[..]) {
					Ok(rc) => rc,
					Err(_) => {
						return Err(DispatchErrorWithMessage::from(
							"Failed to deserialize into a RuntimeCall",
						));
					},
				};

			let transaction_metadata = TransactionMetadata {
				expiry_block: current_block_number.saturating_add(blocks_to_expiry),
				nonce: match nonce_or_account {
					NonceOrAccount::Nonce(nonce) => nonce,
					NonceOrAccount::Account(account) => System::account_nonce(account),
				},
			};
			let encoded_data = match encoding {
				EncodingType::Eth(EthEncodingType::PersonalSign) =>
					// Encode domain without the prefix because EVM wallets automatically
					// prefix the calldata when using personal_sign
					EncodedNonNativeCall::String(build_domain_data(
						runtime_call.clone(),
						&chainflip_network,
						&transaction_metadata,
						spec_version,
					)),
				EncodingType::Eth(EthEncodingType::Eip712) => {
					let chainflip_extrinsic = ChainflipExtrinsic { call: runtime_call, transaction_metadata };
					let typed_data: TypedData =
						build_eip712_typed_data(
							chainflip_extrinsic,
							chainflip_network.as_str().to_owned(),
							spec_version,
						)
						.map_err(|_| {
							DispatchErrorWithMessage::from(
								"Failed to build eip712 typed data"
							)
						})?;
					EncodedNonNativeCall::Eip712(typed_data)
				},
				EncodingType::Sol(SolEncodingType::Domain) => {
					let raw_payload = build_domain_data(
						runtime_call,
						&chainflip_network,
						&transaction_metadata,
						spec_version,
					);
					EncodedNonNativeCall::String(
						[DOMAIN_OFFCHAIN_PREFIX, &raw_payload].concat()
					)
				},
			};

			Ok((encoded_data, transaction_metadata))
		}
	}
}
