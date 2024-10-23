use crate::{
	Environment, Offence, Reputation, Runtime, SolanaBroadcaster, SolanaChainTracking,
	SolanaIngressEgress, SolanaThresholdSigner,
};
use cf_chains::{
	instances::ChainInstanceAlias,
	sol::{api::SolanaTransactionType, SolAddress, SolAmount, SolHash, SolSignature, SolTrackedData, SolanaCrypto},
	Chain, FeeEstimationApi, ForeignChain, Solana,
};
use cf_runtime_utilities::log_or_panic;
use cf_traits::{
	offence_reporting::OffenceReporter, AdjustedFeeEstimationApi, Chainflip,
	ElectionEgressWitnesser, GetBlockHeight, IngressSource, SolanaNonceWatch,
};

use codec::{Decode, Encode};
use frame_system::pallet_prelude::BlockNumberFor;
use pallet_cf_elections::{
	electoral_system::{ElectoralReadAccess, ElectoralSystem},
	electoral_systems::{
		self,
		change::OnChangeHook,
		composite::{tuple_6_impls::Hooks, CompositeRunner},
		egress_success::OnEgressSuccess,
		liveness::OnCheckComplete,
		monotonic_change::OnChangeHook,
		monotonic_median::MedianChangeHook,
	},
	CorruptStorageError, ElectionIdentifier, InitialState, InitialStateOf, RunnerStorageAccess,
};

use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_runtime::{DispatchResult, FixedPointNumber, FixedU128};
use sp_std::{collections::btree_set::BTreeSet, vec::Vec};

#[cfg(feature = "runtime-benchmarks")]
use cf_chains::benchmarking_value::BenchmarkValue;
use sol_prim::SlotNumber;

type Instance = <Solana as ChainInstanceAlias>::Instance;

pub type SolanaElectoralSystemRunner = CompositeRunner<
	(
		SolanaBlockHeightTracking,
		SolanaFeeTracking,
		SolanaIngressTracking,
		SolanaNonceTracking,
		SolanaEgressWitnessing,
		SolanaLiveness,
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
		),
		unsynchronised_settings: (
			(),
			SolanaFeeUnsynchronisedSettings { fee_multiplier: FixedU128::from_u32(1u32) },
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

pub type SolanaLiveness = electoral_systems::liveness::Liveness<
	<Solana as Chain>::ChainBlockNumber,
	SolHash,
	cf_primitives::BlockNumber,
	OnCheckCompleteHook,
	<Runtime as Chainflip>::ValidatorId,
>;

pub struct OnCheckCompleteHook;

impl OnCheckComplete<<Runtime as Chainflip>::ValidatorId> for OnCheckCompleteHook {
	fn on_check_complete(validator_ids: BTreeSet<<Runtime as Chainflip>::ValidatorId>) {
		Reputation::report_many(Offence::FailedLivenessCheck(ForeignChain::Solana), validator_ids);
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
				priority_fee: SolanaChainTrackingProvider::priority_fee().unwrap_or_default(),
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
		): (
			Vec<
				ElectionIdentifier<
					<SolanaBlockHeightTracking as ElectoralSystem>::ElectionIdentifierExtra,
				>,
			>,
			Vec<
				ElectionIdentifier<<SolanaFeeTracking as ElectoralSystem>::ElectionIdentifierExtra>,
			>,
			Vec<
				ElectionIdentifier<
					<SolanaIngressTracking as ElectoralSystem>::ElectionIdentifierExtra,
				>,
			>,
			Vec<
				ElectionIdentifier<
					<SolanaNonceTracking as ElectoralSystem>::ElectionIdentifierExtra,
				>,
			>,
			Vec<
				ElectionIdentifier<
					<SolanaEgressWitnessing as ElectoralSystem>::ElectionIdentifierExtra,
				>,
			>,
			Vec<ElectionIdentifier<<SolanaLiveness as ElectoralSystem>::ElectionIdentifierExtra>>,
		),
	) -> Result<(), CorruptStorageError> {
		let block_height = SolanaBlockHeightTracking::on_finalize::<
			DerivedElectoralAccess<
				_,
				SolanaBlockHeightTracking,
				RunnerStorageAccess<Runtime, SolanaInstance>,
			>,
		>(block_height_identifiers, &())?;
		SolanaLiveness::on_finalize::<
			DerivedElectoralAccess<_, SolanaLiveness, RunnerStorageAccess<Runtime, SolanaInstance>>,
		>(liveness_identifiers, &(crate::System::block_number(), block_height))?;
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

use pallet_cf_elections::electoral_systems::composite::tuple_6_impls::DerivedElectoralAccess;

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
	pub fn priority_fee() -> Option<<Solana as Chain>::ChainAmount> {
		DerivedElectoralAccess::<
			_,
			SolanaFeeTracking,
			RunnerStorageAccess<Runtime, SolanaInstance>,
			>::unsynchronised_state()
			.ok()
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

	fn estimate_egress_fee(asset: <Solana as Chain>::ChainAsset) -> <Solana as Chain>::ChainAmount {
		Self::with_tracked_data_then_apply_fee_multiplier(|tracked_data| {
			tracked_data.estimate_egress_fee(asset)
		})
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
		Ok(pallet_cf_elections::Pallet::<Runtime, SolanaInstance>::with_status_check(|| {
			SolanaNonceTracking::watch_for_change::<
				DerivedElectoralAccess<
					_,
					SolanaNonceTracking,
					RunnerStorageAccess<Runtime, SolanaInstance>,
				>,
			>(nonce_account, previous_nonce_value)
		})?)
	}
}

pub struct SolanaEgressWitnessingTrigger;

impl ElectionEgressWitnesser for SolanaEgressWitnessingTrigger {
	type Chain = SolanaCrypto;

	fn watch_for_egress_success(signature: SolSignature) -> DispatchResult {
		Ok(pallet_cf_elections::Pallet::<Runtime, SolanaInstance>::with_status_check(|| {
			SolanaEgressWitnessing::watch_for_egress::<
				DerivedElectoralAccess<
					_,
					SolanaEgressWitnessing,
					RunnerStorageAccess<Runtime, SolanaInstance>,
				>,
			>(signature)
		})?)
	}
}
