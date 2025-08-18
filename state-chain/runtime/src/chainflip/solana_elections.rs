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
	chainflip::ReportFailedLivenessCheck, constants::common::LIVENESS_CHECK_DURATION, AccountId,
	Environment, Runtime, SolanaBroadcaster, SolanaChainTracking, SolanaIngressEgress,
	SolanaThresholdSigner,
};

use cf_chains::{
	address::EncodedAddress,
	assets::{any::Asset, sol::Asset as SolAsset},
	instances::{ChainInstanceAlias, SolanaInstance},
	sol::{
		api::{
			AltWitnessingConsensusResult, SolanaApi, SolanaTransactionBuildingError,
			SolanaTransactionType, VaultSwapAccountAndSender,
		},
		compute_units_costs::MIN_COMPUTE_PRICE,
		sol_tx_core::SlotNumber,
		SolAddress, SolAddressLookupTableAccount, SolAmount, SolHash, SolSignature, SolTrackedData,
		SolanaCrypto,
	},
	CcmDepositMetadataUnchecked, Chain, ChannelRefundParametersForChain, FeeEstimationApi,
	FetchAndCloseSolanaVaultSwapAccounts, ForeignChainAddress, Solana,
};
use cf_primitives::{AffiliateShortId, Affiliates, Beneficiary, DcaParameters};
use cf_runtime_utilities::log_or_panic;
use cf_traits::{
	AdjustedFeeEstimationApi, Broadcaster, Chainflip, ElectionEgressWitnesser, GetBlockHeight,
	IngressSource, SolanaNonceWatch,
};
use codec::{Decode, Encode};
use frame_system::pallet_prelude::BlockNumberFor;
use pallet_cf_elections::{
	electoral_system::{ElectoralReadAccess, ElectoralSystem, ElectoralSystemTypes},
	electoral_systems::{
		self,
		blockchain::delta_based_ingress::BackoffSettings,
		composite::{tags::G, tuple_7_impls::Hooks, CompositeRunner},
		exact_value::ExactValueHook,
		monotonic_change::OnChangeHook,
		monotonic_median::MedianChangeHook,
		solana_vault_swap_accounts::{FromSolOrNot, SolanaVaultSwapAccountsHook},
	},
	CorruptStorageError, ElectionIdentifier, InitialState, InitialStateOf, RunnerStorageAccess,
};
use pallet_cf_ingress_egress::VaultDepositWitness;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_runtime::DispatchResult;
use sp_std::{collections::btree_set::BTreeSet, vec::Vec};

#[cfg(feature = "runtime-benchmarks")]
use cf_chains::benchmarking_value::BenchmarkValue;
use electoral_systems::liveness::Liveness;

use super::SolEnvironment;

type Instance = <Solana as ChainInstanceAlias>::Instance;

pub type SolanaElectoralSystemRunner = CompositeRunner<
	(
		SolanaBlockHeightTracking,
		SolanaIngressTracking,
		SolanaNonceTracking,
		SolanaEgressWitnessing,
		SolanaLiveness,
		SolanaVaultSwapTracking,
		SolanaAltWitnessing,
	),
	<Runtime as Chainflip>::ValidatorId,
	BlockNumberFor<Runtime>,
	RunnerStorageAccess<Runtime, SolanaInstance>,
	SolanaElectionHooks,
>;

/// Creates an initial state to initialize the pallet with.
pub fn initial_state(
	vault_program: SolAddress,
	usdc_token_mint_pubkey: SolAddress,
	swap_endpoint_data_account_address: SolAddress,
	shared_data_reference_lifetime: BlockNumberFor<Runtime>,
) -> InitialStateOf<Runtime, Instance> {
	InitialState {
		unsynchronised_state: (
			// The initial chain tracking value does not matter, as we don't care about the vault
			// start blocks.
			Default::default(),
			(),
			(),
			(),
			(),
			0u32,
			(),
		),
		unsynchronised_settings: ((), (), (), (), (), (), ()),
		settings: (
			(),
			(
				SolanaIngressSettings { vault_program, usdc_token_mint_pubkey },
				BackoffSettings { backoff_after_blocks: 600, backoff_frequency: 100 },
			),
			(),
			(),
			LIVENESS_CHECK_DURATION,
			SolanaVaultSwapsSettings { swap_endpoint_data_account_address, usdc_token_mint_pubkey },
			(),
		),
		shared_data_reference_lifetime,
	}
}

pub type SolanaBlockHeightTracking = electoral_systems::monotonic_median::MonotonicMedian<
	<Solana as Chain>::ChainBlockNumber,
	(),
	SolanaBlockHeightTrackingHook,
	<Runtime as Chainflip>::ValidatorId,
	BlockNumberFor<Runtime>,
>;

pub type SolanaIngressTracking =
	electoral_systems::blockchain::delta_based_ingress::DeltaBasedIngress<
		pallet_cf_ingress_egress::Pallet<Runtime, Instance>,
		SolanaIngressSettings,
		<Runtime as Chainflip>::ValidatorId,
		BlockNumberFor<Runtime>,
	>;

pub type SolanaNonceTracking = electoral_systems::monotonic_change::MonotonicChange<
	SolAddress,
	SolHash,
	SlotNumber,
	(),
	SolanaNonceTrackingHook,
	<Runtime as Chainflip>::ValidatorId,
	BlockNumberFor<Runtime>,
>;

pub type SolanaEgressWitnessing = electoral_systems::exact_value::ExactValue<
	SolSignature,
	TransactionSuccessDetails,
	(),
	SolanaEgressWitnessingHook,
	<Runtime as Chainflip>::ValidatorId,
	BlockNumberFor<Runtime>,
>;

pub type SolanaLiveness = Liveness<
	<Solana as Chain>::ChainBlockNumber,
	SolHash,
	ReportFailedLivenessCheck<Solana>,
	<Runtime as Chainflip>::ValidatorId,
	BlockNumberFor<Runtime>,
>;

pub type SolanaVaultSwapTracking =
	electoral_systems::solana_vault_swap_accounts::SolanaVaultSwapAccounts<
		VaultSwapAccountAndSender,
		SolanaVaultSwapDetails,
		BlockNumberFor<Runtime>,
		SolanaVaultSwapsSettings,
		SolanaVaultSwapsHandler,
		<Runtime as Chainflip>::ValidatorId,
		SolanaTransactionBuildingError,
	>;

#[derive(
	Serialize,
	Deserialize,
	Default,
	Debug,
	PartialEq,
	Eq,
	Clone,
	Encode,
	Decode,
	TypeInfo,
	Ord,
	PartialOrd,
)]
pub struct SolanaAltWitnessingIdentifier(pub BTreeSet<SolAddress>);

pub type SolanaAltWitnessing = electoral_systems::exact_value::ExactValue<
	SolanaAltWitnessingIdentifier,
	AltWitnessingConsensusResult<Vec<SolAddressLookupTableAccount>>,
	(),
	SolanaAltWitnessingHook,
	<Runtime as Chainflip>::ValidatorId,
	BlockNumberFor<Runtime>,
>;

pub fn solana_alt_result(
	alts: BTreeSet<SolAddress>,
) -> Option<AltWitnessingConsensusResult<Vec<SolAddressLookupTableAccount>>> {
	SolanaAltWitnessing::take_election_result::<
		DerivedElectoralAccess<
			_,
			SolanaAltWitnessing,
			RunnerStorageAccess<Runtime, SolanaInstance>,
		>,
	>(SolanaAltWitnessingIdentifier(alts))
}

pub type SolanaAltWitnessingElectoralAccess =
	DerivedElectoralAccess<G, SolanaAltWitnessing, RunnerStorageAccess<Runtime, SolanaInstance>>;

pub struct SolanaAltWitnessingHook;

impl
	ExactValueHook<
		SolanaAltWitnessingIdentifier,
		AltWitnessingConsensusResult<Vec<SolAddressLookupTableAccount>>,
	> for SolanaAltWitnessingHook
{
	type StorageKey = SolanaAltWitnessingIdentifier;
	type StorageValue = AltWitnessingConsensusResult<Vec<SolAddressLookupTableAccount>>;

	fn on_consensus(
		alt_identifier: SolanaAltWitnessingIdentifier,
		alts: AltWitnessingConsensusResult<Vec<SolAddressLookupTableAccount>>,
	) -> Option<(
		SolanaAltWitnessingIdentifier,
		AltWitnessingConsensusResult<Vec<SolAddressLookupTableAccount>>,
	)> {
		Some((alt_identifier, alts))
	}
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Encode, Decode, TypeInfo)]
pub struct TransactionSuccessDetails {
	pub tx_fee: u64,
	// It is possible for a contract call to be reverted due to contract's internal error.
	// This field is set to `true` if the contract call executed successfully without error.
	pub transaction_successful: bool,
}

pub struct SolanaEgressWitnessingHook;

impl ExactValueHook<SolSignature, TransactionSuccessDetails> for SolanaEgressWitnessingHook {
	type StorageKey = ();
	type StorageValue = ();

	fn on_consensus(
		signature: SolSignature,
		TransactionSuccessDetails { tx_fee, transaction_successful }: TransactionSuccessDetails,
	) -> Option<((), ())> {
		use cf_traits::KeyProvider;
		if !transaction_successful {
			// On CCM failure, we need to refund the user using their fallback info.
			if let Some((broadcast_id, ccm_tx)) =
				SolanaBroadcaster::pending_api_call_from_out_id(signature)
			{
				// Only Ccm calls support fallback.
				if let SolanaTransactionType::CcmTransfer { fallback } = ccm_tx.call_type {
					SolanaIngressEgress::do_ccm_fallback(broadcast_id, fallback);
				}
			} else {
				log::error!("Ccm fallback failed: Reported Solana contract call revert, but the ApiCall does not exist in storage. Tx_out_id: : {:?}", signature);
			}
		}

		if let Err(err) = SolanaBroadcaster::egress_success(
			pallet_cf_witnesser::RawOrigin::CurrentEpochWitnessThreshold.into(),
			signature,
			// Assign any owed fees to the current key.
			SolanaThresholdSigner::active_epoch_key().map(|e| e.key).unwrap_or_default(),
			tx_fee,
			(),
			signature,
		) {
			log::error!(
				"Failed to execute egress success: TxOutId: {:?}, Error: {:?}",
				signature,
				err
			)
		}

		None
	}
}

pub struct SolanaNonceTrackingHook;

impl OnChangeHook<SolAddress, SolHash> for SolanaNonceTrackingHook {
	fn on_change(nonce_account: SolAddress, durable_nonce: SolHash) {
		Environment::update_sol_nonce(nonce_account, durable_nonce);
	}
}

pub struct SolanaBlockHeightTrackingHook;

impl MedianChangeHook<<Solana as Chain>::ChainBlockNumber> for SolanaBlockHeightTrackingHook {
	fn on_change(block_height: <Solana as Chain>::ChainBlockNumber) {
		if let Err(err) = SolanaChainTracking::inner_update_chain_state(cf_chains::ChainState {
			block_height,
			tracked_data: SolTrackedData {
				priority_fee: SolanaChainTrackingProvider::priority_fee(),
			},
		}) {
			log::error!("Failed to update chain state: {:?}", err);
		}
	}
}

pub struct SolanaElectionHooks;

impl
	Hooks<
		SolanaBlockHeightTracking,
		SolanaIngressTracking,
		SolanaNonceTracking,
		SolanaEgressWitnessing,
		SolanaLiveness,
		SolanaVaultSwapTracking,
		SolanaAltWitnessing,
	> for SolanaElectionHooks
{
	fn on_finalize(
		(
			block_height_identifiers,
			ingress_identifiers,
			nonce_tracking_identifiers,
			egress_witnessing_identifiers,
			liveness_identifiers,
			vault_swap_identifiers,
			alt_witnessing_identifiers,
		): (
			Vec<
				ElectionIdentifier<
					<SolanaBlockHeightTracking as ElectoralSystemTypes>::ElectionIdentifierExtra,
				>,
			>,
			Vec<
				ElectionIdentifier<
					<SolanaIngressTracking as ElectoralSystemTypes>::ElectionIdentifierExtra,
				>,
			>,
			Vec<
				ElectionIdentifier<
					<SolanaNonceTracking as ElectoralSystemTypes>::ElectionIdentifierExtra,
				>,
			>,
			Vec<
				ElectionIdentifier<
					<SolanaEgressWitnessing as ElectoralSystemTypes>::ElectionIdentifierExtra,
				>,
			>,
			Vec<
				ElectionIdentifier<
					<SolanaLiveness as ElectoralSystemTypes>::ElectionIdentifierExtra,
				>,
			>,
			Vec<
				ElectionIdentifier<
					<SolanaVaultSwapTracking as ElectoralSystemTypes>::ElectionIdentifierExtra,
				>,
			>,
			Vec<
				ElectionIdentifier<
					<SolanaAltWitnessing as ElectoralSystemTypes>::ElectionIdentifierExtra,
				>,
			>,
		),
	) -> Result<(), CorruptStorageError> {
		let current_sc_block_number = crate::System::block_number();
		let block_height = SolanaBlockHeightTracking::on_finalize::<
			DerivedElectoralAccess<
				_,
				SolanaBlockHeightTracking,
				RunnerStorageAccess<Runtime, SolanaInstance>,
			>,
		>(block_height_identifiers, &())?;
		SolanaLiveness::on_finalize::<
			DerivedElectoralAccess<_, SolanaLiveness, RunnerStorageAccess<Runtime, SolanaInstance>>,
		>(liveness_identifiers, &(current_sc_block_number, block_height))?;
		SolanaNonceTracking::on_finalize::<
			DerivedElectoralAccess<
				_,
				SolanaNonceTracking,
				RunnerStorageAccess<Runtime, SolanaInstance>,
			>,
		>(nonce_tracking_identifiers, &())?;
		SolanaEgressWitnessing::on_finalize::<
			DerivedElectoralAccess<
				_,
				SolanaEgressWitnessing,
				RunnerStorageAccess<Runtime, SolanaInstance>,
			>,
		>(egress_witnessing_identifiers, &())?;
		SolanaIngressTracking::on_finalize::<
			DerivedElectoralAccess<
				_,
				SolanaIngressTracking,
				RunnerStorageAccess<Runtime, SolanaInstance>,
			>,
		>(ingress_identifiers, &block_height)?;
		SolanaVaultSwapTracking::on_finalize::<
			DerivedElectoralAccess<
				_,
				SolanaVaultSwapTracking,
				RunnerStorageAccess<Runtime, SolanaInstance>,
			>,
		>(vault_swap_identifiers, &current_sc_block_number)?;
		SolanaAltWitnessing::on_finalize::<
			DerivedElectoralAccess<
				_,
				SolanaAltWitnessing,
				RunnerStorageAccess<Runtime, SolanaInstance>,
			>,
		>(alt_witnessing_identifiers, &())?;
		Ok(())
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize)]
pub struct SolanaIngressSettings {
	pub vault_program: SolAddress,
	pub usdc_token_mint_pubkey: SolAddress,
}

#[cfg(feature = "runtime-benchmarks")]
impl BenchmarkValue for SolanaIngressSettings {
	fn benchmark_value() -> Self {
		Self {
			vault_program: SolAddress([0xf0; 32]),
			usdc_token_mint_pubkey: SolAddress([0xf1; 32]),
		}
	}
}

use pallet_cf_elections::electoral_systems::composite::tuple_7_impls::DerivedElectoralAccess;

pub struct SolanaChainTrackingProvider;
impl GetBlockHeight<Solana> for SolanaChainTrackingProvider {
	fn get_block_height() -> <Solana as Chain>::ChainBlockNumber {
		DerivedElectoralAccess::<
			_,
			SolanaBlockHeightTracking,
			RunnerStorageAccess<Runtime, SolanaInstance>,
		>::unsynchronised_state()
		.unwrap_or_else(|err| {
			log_or_panic!("Failed to obtain Solana block height: '{err:?}'.");
			// We use default in error case as it is preferable to panicking, and in
			// solana's case having lower than true chain tracking is not a problem
			// as the engines do not use the vault start block numbers to "go back".
			Default::default()
		})
	}
}

// TODO: Look at removing this
impl SolanaChainTrackingProvider {
	pub fn priority_fee() -> <Solana as Chain>::ChainAmount {
		MIN_COMPUTE_PRICE
	}

	// TODO: Delete this.
	fn with_tracked_data_then_apply_fee_multiplier<
		F: FnOnce(SolTrackedData) -> <Solana as Chain>::ChainAmount,
	>(
		f: F,
	) -> <Solana as Chain>::ChainAmount {
		f(SolTrackedData { priority_fee: SolanaChainTrackingProvider::priority_fee() })
	}
}
impl AdjustedFeeEstimationApi<Solana> for SolanaChainTrackingProvider {
	fn estimate_ingress_fee(
		asset: <Solana as Chain>::ChainAsset,
	) -> <Solana as Chain>::ChainAmount {
		Self::with_tracked_data_then_apply_fee_multiplier(|tracked_data| {
			tracked_data.estimate_ingress_fee(asset)
		})
	}

	fn estimate_ingress_fee_vault_swap() -> Option<<Solana as Chain>::ChainAmount> {
		Some(Self::with_tracked_data_then_apply_fee_multiplier(|tracked_data| {
			// TODO: These should be untangled?
			tracked_data.estimate_ingress_fee_vault_swap().unwrap_or_else(|| {
				log_or_panic!(
					"Obtained None when estimating Solana Ingress Vault Swap fee. This should not happen"
				);
				Default::default()
			})
		}))
	}

	fn estimate_egress_fee(asset: <Solana as Chain>::ChainAsset) -> <Solana as Chain>::ChainAmount {
		Self::with_tracked_data_then_apply_fee_multiplier(|tracked_data| {
			tracked_data.estimate_egress_fee(asset)
		})
	}

	fn estimate_ccm_fee(
		asset: <Solana as Chain>::ChainAsset,
		gas_budget: cf_primitives::GasAmount,
		message_length: usize,
	) -> Option<<Solana as Chain>::ChainAmount> {
		Some(Self::with_tracked_data_then_apply_fee_multiplier(|tracked_data| {
			tracked_data
				.estimate_ccm_fee(asset, gas_budget, message_length)
				.unwrap_or_else(|| {
					log_or_panic!(
						"Obtained None when estimating Solana Ccm fee. This should not happen"
					);
					Default::default()
				})
		}))
	}
}

pub struct SolanaIngress;
impl IngressSource for SolanaIngress {
	type Chain = Solana;
	type StateChainBlockNumber = BlockNumberFor<Runtime>;

	fn open_channel(
		channel: <Self::Chain as Chain>::ChainAccount,
		asset: <Self::Chain as Chain>::ChainAsset,
		close_block: <Self::Chain as Chain>::ChainBlockNumber,
		current_state_chain_block_number: Self::StateChainBlockNumber,
	) -> DispatchResult {
		pallet_cf_elections::Pallet::<Runtime, SolanaInstance>::with_election_identifiers(
			|composite_election_identifiers| {
				SolanaElectoralSystemRunner::with_identifiers(
					composite_election_identifiers,
					|grouped_election_identifiers| {
						let (_, election_identifiers, ..) = grouped_election_identifiers;
						SolanaIngressTracking::open_channel::<
							DerivedElectoralAccess<
								_,
								SolanaIngressTracking,
								RunnerStorageAccess<Runtime, SolanaInstance>,
							>,
						>(
							election_identifiers,
							channel,
							asset,
							close_block,
							current_state_chain_block_number,
						)
					},
				)
			},
		)
	}
}

pub struct SolanaNonceTrackingTrigger;

impl SolanaNonceWatch for SolanaNonceTrackingTrigger {
	fn watch_for_nonce_change(
		nonce_account: SolAddress,
		previous_nonce_value: SolHash,
	) -> DispatchResult {
		pallet_cf_elections::Pallet::<Runtime, SolanaInstance>::with_status_check(|| {
			SolanaNonceTracking::watch_for_change::<
				DerivedElectoralAccess<
					_,
					SolanaNonceTracking,
					RunnerStorageAccess<Runtime, SolanaInstance>,
				>,
			>(nonce_account, previous_nonce_value)
		})
	}
}

pub struct SolanaEgressWitnessingTrigger;

impl ElectionEgressWitnesser for SolanaEgressWitnessingTrigger {
	type Chain = SolanaCrypto;

	fn watch_for_egress_success(signature: SolSignature) -> DispatchResult {
		pallet_cf_elections::Pallet::<Runtime, SolanaInstance>::with_status_check(|| {
			SolanaEgressWitnessing::witness_exact_value::<
				DerivedElectoralAccess<
					_,
					SolanaEgressWitnessing,
					RunnerStorageAccess<Runtime, SolanaInstance>,
				>,
			>(signature)
		})
	}
}

#[derive(
	Clone, PartialEq, Eq, Debug, Serialize, Deserialize, TypeInfo, Encode, Decode, PartialOrd, Ord,
)]
pub struct SolanaVaultSwapDetails {
	pub from: SolAsset,
	pub to: Asset,
	pub deposit_amount: SolAmount,
	pub destination_address: EncodedAddress,
	pub deposit_metadata: Option<CcmDepositMetadataUnchecked<ForeignChainAddress>>,
	pub swap_account: SolAddress,
	pub creation_slot: u64,
	pub broker_fee: Beneficiary<AccountId>,
	pub refund_params: ChannelRefundParametersForChain<Solana>,
	pub dca_params: Option<DcaParameters>,
	pub boost_fee: u8,
	pub affiliate_fees: Affiliates<AffiliateShortId>,
}

#[cfg(feature = "runtime-benchmarks")]
impl BenchmarkValue for SolanaVaultSwapDetails {
	fn benchmark_value() -> Self {
		Self {
			from: BenchmarkValue::benchmark_value(),
			to: BenchmarkValue::benchmark_value(),
			deposit_amount: BenchmarkValue::benchmark_value(),
			destination_address: BenchmarkValue::benchmark_value(),
			deposit_metadata: Some(BenchmarkValue::benchmark_value()),
			swap_account: BenchmarkValue::benchmark_value(),
			creation_slot: BenchmarkValue::benchmark_value(),
			broker_fee: BenchmarkValue::benchmark_value(),
			refund_params: BenchmarkValue::benchmark_value(),
			dca_params: Some(BenchmarkValue::benchmark_value()),
			boost_fee: BenchmarkValue::benchmark_value(),
			affiliate_fees: BenchmarkValue::benchmark_value(),
		}
	}
}

pub struct SolanaVaultSwapsHandler;

impl
	SolanaVaultSwapAccountsHook<
		VaultSwapAccountAndSender,
		SolanaVaultSwapDetails,
		SolanaTransactionBuildingError,
	> for SolanaVaultSwapsHandler
{
	fn initiate_vault_swap(swap_details: SolanaVaultSwapDetails) {
		let block_height = swap_details.creation_slot;
		SolanaIngressEgress::process_vault_swap_request_full_witness(
			block_height,
			VaultDepositWitness {
				input_asset: swap_details.from,
				deposit_address: None,
				channel_id: None,
				deposit_amount: swap_details.deposit_amount,
				deposit_details: (),
				output_asset: swap_details.to,
				destination_address: swap_details.destination_address,
				deposit_metadata: swap_details.deposit_metadata,
				tx_id: (swap_details.swap_account, swap_details.creation_slot),
				broker_fee: Some(swap_details.broker_fee),
				affiliate_fees: swap_details.affiliate_fees,
				dca_params: swap_details.dca_params,
				refund_params: swap_details.refund_params,
				boost_fee: swap_details.boost_fee.into(),
			},
		);
	}

	fn maybe_fetch_and_close_accounts(
		accounts: Vec<VaultSwapAccountAndSender>,
	) -> Result<(), SolanaTransactionBuildingError> {
		<SolanaApi<SolEnvironment> as FetchAndCloseSolanaVaultSwapAccounts>::new_unsigned(accounts)
			.map(|apicall| {
				let _ = <SolanaBroadcaster as Broadcaster<Solana>>::threshold_sign_and_broadcast(
					apicall,
				);
			})
	}

	fn get_number_of_available_sol_nonce_accounts(critical: bool) -> usize {
		Environment::get_number_of_available_sol_nonce_accounts(critical)
	}
}

#[derive(
	Clone, PartialEq, Eq, Debug, Serialize, Deserialize, TypeInfo, Encode, Decode, PartialOrd, Ord,
)]
pub struct SolanaVaultSwapsSettings {
	pub swap_endpoint_data_account_address: SolAddress,
	pub usdc_token_mint_pubkey: SolAddress,
}

#[cfg(feature = "runtime-benchmarks")]
impl BenchmarkValue for SolanaVaultSwapsSettings {
	fn benchmark_value() -> Self {
		Self {
			swap_endpoint_data_account_address: BenchmarkValue::benchmark_value(),
			usdc_token_mint_pubkey: BenchmarkValue::benchmark_value(),
		}
	}
}

impl FromSolOrNot for SolanaVaultSwapDetails {
	fn sol_or_not(s: &SolanaVaultSwapDetails) -> bool {
		s.from == SolAsset::Sol
	}
}

pub(crate) fn initiate_solana_alt_election(alts: BTreeSet<SolAddress>) {
	if alts.is_empty() {
		return
	}

	pallet_cf_elections::Pallet::<Runtime, SolanaInstance>::with_status_check(|| {
		SolanaAltWitnessing::witness_exact_value::<SolanaAltWitnessingElectoralAccess>(
			SolanaAltWitnessingIdentifier(alts),
		)
	})
	.unwrap_or_else(|e| {
		//The error should not happen as long as the election identifiers don't overflow
		log::error!("Cannot start Solana ALT witnessing election: {:?}", e);
	})
}

pub struct SolanaElectoralSystemConfiguration;

impl pallet_cf_elections::ElectoralSystemConfiguration for SolanaElectoralSystemConfiguration {
	type Properties = ();

	fn start(_properties: Self::Properties) {}

	type ElectoralEvents = ();

	type SafeMode = ();
}
