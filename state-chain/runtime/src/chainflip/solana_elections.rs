use crate::{
	chainflip::ReportFailedLivenessCheck, AccountId, Environment, Offence, Reputation, Runtime,
	SolanaBroadcaster, SolanaChainTracking, SolanaIngressEgress, SolanaThresholdSigner,
};

use cf_chains::{
	address::EncodedAddress,
	assets::{any::Asset, sol::Asset as SolAsset},
	instances::{ChainInstanceAlias, SolanaInstance},
	sol::{
		api::{
			SolanaApi, SolanaTransactionBuildingError, SolanaTransactionType,
			VaultSwapAccountAndSender,
		},
		compute_units_costs::MIN_COMPUTE_PRICE,
		SolAddress, SolAmount, SolHash, SolSignature, SolTrackedData, SolanaCrypto,
	},
	CcmDepositMetadata, Chain, ChannelRefundParametersDecoded, FeeEstimationApi,
	FetchAndCloseSolanaVaultSwapAccounts, ForeignChain, Solana,
};
use cf_primitives::{AffiliateShortId, Affiliates, Beneficiary, DcaParameters};
use cf_runtime_utilities::log_or_panic;
use cf_traits::{
	offence_reporting::OffenceReporter, AdjustedFeeEstimationApi, Broadcaster, Chainflip,
	ElectionEgressWitnesser, GetBlockHeight, IngressSource, SolanaNonceWatch,
};
use codec::{Decode, Encode};
use frame_system::pallet_prelude::BlockNumberFor;
use pallet_cf_elections::{
	electoral_system::{ElectoralReadAccess, ElectoralSystem, ElectoralSystemTypes},
	electoral_systems::{
		self,
		composite::{tuple_7_impls::Hooks, CompositeRunner},
		egress_success::OnEgressSuccess,
		liveness::OnCheckComplete,
		monotonic_change::OnChangeHook,
		monotonic_median::MedianChangeHook,
		solana_vault_swap_accounts::{FromSolOrNot, SolanaVaultSwapAccountsHook},
	},
	CorruptStorageError, ElectionIdentifier, InitialState, InitialStateOf, RunnerStorageAccess,
};
use pallet_cf_ingress_egress::VaultDepositWitness;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_runtime::{DispatchResult, FixedPointNumber, FixedU128};
use sp_std::{collections::btree_set::BTreeSet, vec::Vec};

#[cfg(feature = "runtime-benchmarks")]
use cf_chains::benchmarking_value::BenchmarkValue;
use cf_primitives::chains::Bitcoin;
use electoral_systems::liveness::Liveness;
use sol_prim::SlotNumber;

use super::SolEnvironment;

type Instance = <Solana as ChainInstanceAlias>::Instance;

pub type SolanaElectoralSystemRunner = CompositeRunner<
	(
		SolanaBlockHeightTracking,
		SolanaFeeTracking,
		SolanaIngressTracking,
		SolanaNonceTracking,
		SolanaEgressWitnessing,
		SolanaLiveness,
		SolanaVaultSwapTracking,
	),
	<Runtime as Chainflip>::ValidatorId,
	RunnerStorageAccess<Runtime, SolanaInstance>,
	SolanaElectionHooks,
>;

const LIVENESS_CHECK_DURATION: BlockNumberFor<Runtime> = 10;

/// Creates an initial state to initialize the pallet with.
pub fn initial_state(
	priority_fee: SolAmount,
	vault_program: SolAddress,
	usdc_token_mint_pubkey: SolAddress,
	swap_endpoint_data_account_address: SolAddress,
) -> InitialStateOf<Runtime, Instance> {
	InitialState {
		unsynchronised_state: (
			// The initial chaintracking value does not matter, as we don't care about the vault
			// start blocks.
			Default::default(),
			priority_fee,
			(),
			(),
			(),
			(),
			0u32,
		),
		unsynchronised_settings: (
			(),
			SolanaFeeUnsynchronisedSettings { fee_multiplier: FixedU128::from_u32(1u32) },
			(),
			(),
			(),
			(),
			(),
		),
		settings: (
			(),
			(),
			SolanaIngressSettings { vault_program, usdc_token_mint_pubkey },
			(),
			(),
			LIVENESS_CHECK_DURATION,
			SolanaVaultSwapsSettings { swap_endpoint_data_account_address, usdc_token_mint_pubkey },
		),
	}
}

pub type SolanaBlockHeightTracking = electoral_systems::monotonic_median::MonotonicMedian<
	<Solana as Chain>::ChainBlockNumber,
	(),
	SolanaBlockHeightTrackingHook,
	<Runtime as Chainflip>::ValidatorId,
>;
pub type SolanaFeeTracking = electoral_systems::unsafe_median::UnsafeMedian<
	<Solana as Chain>::ChainAmount,
	SolanaFeeUnsynchronisedSettings,
	(),
	<Runtime as Chainflip>::ValidatorId,
>;
pub type SolanaIngressTracking =
	electoral_systems::blockchain::delta_based_ingress::DeltaBasedIngress<
		pallet_cf_ingress_egress::Pallet<Runtime, Instance>,
		SolanaIngressSettings,
		<Runtime as Chainflip>::ValidatorId,
	>;

pub type SolanaNonceTracking = electoral_systems::monotonic_change::MonotonicChange<
	SolAddress,
	SolHash,
	SlotNumber,
	(),
	SolanaNonceTrackingHook,
	<Runtime as Chainflip>::ValidatorId,
>;

pub type SolanaEgressWitnessing = electoral_systems::egress_success::EgressSuccess<
	SolSignature,
	TransactionSuccessDetails,
	(),
	SolanaEgressWitnessingHook,
	<Runtime as Chainflip>::ValidatorId,
>;

pub type SolanaLiveness = Liveness<
	<Solana as Chain>::ChainBlockNumber,
	SolHash,
	cf_primitives::BlockNumber,
	ReportFailedLivenessCheck<Solana>,
	<Runtime as Chainflip>::ValidatorId,
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

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Encode, Decode, TypeInfo)]
pub struct TransactionSuccessDetails {
	pub tx_fee: u64,
	// It is possible for a contract call to be reverted due to contract's internal error.
	// This field is set to `true` if the contract call executed successfully without error.
	pub transaction_successful: bool,
}

pub struct SolanaEgressWitnessingHook;

impl OnEgressSuccess<SolSignature, TransactionSuccessDetails> for SolanaEgressWitnessingHook {
	fn on_egress_success(
		signature: SolSignature,
		TransactionSuccessDetails { tx_fee, transaction_successful }: TransactionSuccessDetails,
	) {
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
		SolanaFeeTracking,
		SolanaIngressTracking,
		SolanaNonceTracking,
		SolanaEgressWitnessing,
		SolanaLiveness,
		SolanaVaultSwapTracking,
	> for SolanaElectionHooks
{
	fn on_finalize(
		(
			block_height_identifiers,
			fee_identifiers,
			ingress_identifiers,
			nonce_tracking_identifiers,
			egress_witnessing_identifiers,
			liveness_identifiers,
			vault_swap_identifiers,
		): (
			Vec<
				ElectionIdentifier<
					<SolanaBlockHeightTracking as ElectoralSystemTypes>::ElectionIdentifierExtra,
				>,
			>,
			Vec<
				ElectionIdentifier<
					<SolanaFeeTracking as ElectoralSystemTypes>::ElectionIdentifierExtra,
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
		SolanaFeeTracking::on_finalize::<
			DerivedElectoralAccess<
				_,
				SolanaFeeTracking,
				RunnerStorageAccess<Runtime, SolanaInstance>,
			>,
		>(fee_identifiers, &())?;
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
		Ok(())
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize)]
pub struct SolanaFeeUnsynchronisedSettings {
	pub fee_multiplier: FixedU128,
}

#[cfg(feature = "runtime-benchmarks")]
impl BenchmarkValue for SolanaFeeUnsynchronisedSettings {
	fn benchmark_value() -> Self {
		Self { fee_multiplier: 1u128.into() }
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

impl SolanaChainTrackingProvider {
	pub fn priority_fee() -> <Solana as Chain>::ChainAmount {
		MIN_COMPUTE_PRICE
	}

	fn with_tracked_data_then_apply_fee_multiplier<
		F: FnOnce(SolTrackedData) -> <Solana as Chain>::ChainAmount,
	>(
		f: F,
	) -> <Solana as Chain>::ChainAmount {
		DerivedElectoralAccess::<
			_,
			SolanaFeeTracking,
			RunnerStorageAccess<Runtime, SolanaInstance>,
		>::unsynchronised_state()
			.and_then(|priority_fee| {
				DerivedElectoralAccess::<
			_,
			SolanaFeeTracking,
			RunnerStorageAccess<Runtime, SolanaInstance>,
		>::unsynchronised_settings().map(|fees| {
					{
						fees.fee_multiplier.saturating_mul_int(f(SolTrackedData { priority_fee }))
					}
				})
			})
			.unwrap_or_else(|err| {
				log_or_panic!("Failed to obtain Solana fee: '{err:?}'.");
				Default::default()
			})
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

	fn open_channel(
		channel: <Self::Chain as Chain>::ChainAccount,
		asset: <Self::Chain as Chain>::ChainAsset,
		close_block: <Self::Chain as Chain>::ChainBlockNumber,
	) -> DispatchResult {
		pallet_cf_elections::Pallet::<Runtime, SolanaInstance>::with_election_identifiers(
			|composite_election_identifiers| {
				SolanaElectoralSystemRunner::with_identifiers(
					composite_election_identifiers,
					|grouped_election_identifiers| {
						let (_, _, election_identifiers, ..) = grouped_election_identifiers;
						SolanaIngressTracking::open_channel::<
							DerivedElectoralAccess<
								_,
								SolanaIngressTracking,
								RunnerStorageAccess<Runtime, SolanaInstance>,
							>,
						>(election_identifiers, channel, asset, close_block)
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
			SolanaEgressWitnessing::watch_for_egress::<
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
	pub deposit_metadata: Option<CcmDepositMetadata>,
	pub swap_account: SolAddress,
	pub creation_slot: u64,
	pub broker_fee: Beneficiary<AccountId>,
	pub refund_params: ChannelRefundParametersDecoded,
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
				refund_params: Some(swap_details.refund_params),
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
