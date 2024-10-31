use crate::{
	Environment, Offence, Reputation, Runtime, RuntimeOrigin, SolanaBroadcaster,
	SolanaChainTracking, SolanaIngressEgress, SolanaThresholdSigner,
};
use cf_chains::{
	address::EncodedAddress,
	assets::{any::Asset, sol::Asset as SolAsset},
	instances::ChainInstanceAlias,
	sol::{
		api::{
			SolanaApi, SolanaTransactionBuildingError, SolanaTransactionType,
			VaultSwapAccountAndSender,
		},
		SolAddress, SolAmount, SolHash, SolSignature, SolTrackedData, SolanaCrypto,
	},
	CcmDepositMetadata, Chain, CloseSolanaVaultSwapAccounts, FeeEstimationApi, ForeignChain,
	Solana,
};
use cf_primitives::TransactionHash;
use cf_runtime_utilities::log_or_panic;
use cf_traits::{
	offence_reporting::OffenceReporter, AdjustedFeeEstimationApi, Broadcaster, Chainflip,
	ElectionEgressWitnesser, GetBlockHeight, IngressSource, SolanaNonceWatch,
};
use codec::{Decode, Encode};
use frame_system::pallet_prelude::BlockNumberFor;
use pallet_cf_elections::{
	electoral_system::{ElectoralReadAccess, ElectoralSystem},
	electoral_systems::{
		self,
		composite::{tuple_7_impls::Hooks, Composite, Translator},
		egress_success::OnEgressSuccess,
		liveness::OnCheckComplete,
		monotonic_change::OnChangeHook,
		monotonic_median::MedianChangeHook,
		solana_swap_accounts_tracking::SolanaVaultSwapAccountsHook,
	},
	CorruptStorageError, ElectionIdentifier, InitialState, InitialStateOf,
};
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_runtime::{traits::BlockNumberProvider, DispatchResult, FixedPointNumber, FixedU128};
use sp_std::{collections::btree_set::BTreeSet, vec::Vec};

#[cfg(feature = "runtime-benchmarks")]
use cf_chains::benchmarking_value::BenchmarkValue;
use sol_prim::SlotNumber;

use super::SolEnvironment;

type Instance = <Solana as ChainInstanceAlias>::Instance;

pub type SolanaElectoralSystem = Composite<
	(
		SolanaBlockHeightTracking,
		SolanaFeeTracking,
		SolanaIngressTracking,
		SolanaNonceTracking,
		SolanaEgressWitnessing,
		SolanaVaultSwapTracking,
		SolanaLiveness,
	),
	<Runtime as Chainflip>::ValidatorId,
	SolanaElectionHooks,
>;

pub mod old {
	use super::*;
	use crate::Weight;
	use bitvec::prelude::*;
	use cf_primitives::EpochIndex;
	use frame_support::{
		pallet_prelude::{OptionQuery, StorageDoubleMap},
		traits::{OnRuntimeUpgrade, StorageInstance},
		Identity, Twox64Concat,
	};
	use pallet_cf_elections::{
		electoral_system::ElectionIdentifierOf, vote_storage::VoteStorage, ConsensusHistory,
		SharedDataHash, UniqueMonotonicIdentifier,
	};

	pub type SolanaNonceTrackingOld = pallet_cf_elections::migrations::change_old::Change<
		SolAddress,
		SolHash,
		(),
		SolanaNonceTrackingHook,
		<Runtime as Chainflip>::ValidatorId,
	>;
	pub type SolanaElectoralSystem = Composite<
		(
			SolanaBlockHeightTracking,
			SolanaFeeTracking,
			SolanaIngressTracking,
			SolanaNonceTrackingOld,
			SolanaEgressWitnessing,
			SolanaVaultSwapTracking,
			SolanaLiveness,
		),
		<Runtime as Chainflip>::ValidatorId,
		SolanaElectionHooksOld,
	>;
	pub struct SolanaElectionHooksOld;
	impl
		Hooks<
			SolanaBlockHeightTracking,
			SolanaFeeTracking,
			SolanaIngressTracking,
			SolanaNonceTrackingOld,
			SolanaEgressWitnessing,
			SolanaVaultSwapTracking,
			SolanaLiveness,
		> for SolanaElectionHooksOld
	{
		type OnFinalizeContext = ();
		type OnFinalizeReturn = ();

		fn on_finalize<
			GenericElectoralAccess,
			BlockHeightTranslator: Translator<GenericElectoralAccess, ElectoralSystem = SolanaBlockHeightTracking>,
			FeeTranslator: Translator<GenericElectoralAccess, ElectoralSystem = SolanaFeeTracking>,
			IngressTranslator: Translator<GenericElectoralAccess, ElectoralSystem = SolanaIngressTracking>,
			OldNonceTrackingTranslator: Translator<GenericElectoralAccess, ElectoralSystem = old::SolanaNonceTrackingOld>,
			EgressWitnessingTranslator: Translator<GenericElectoralAccess, ElectoralSystem = SolanaEgressWitnessing>,
			VaultSwapTranslator: Translator<GenericElectoralAccess, ElectoralSystem = SolanaVaultSwapTracking>,
			LivenessTranslator: Translator<GenericElectoralAccess, ElectoralSystem = SolanaLiveness>,
		>(
			generic_electoral_access: &mut GenericElectoralAccess,
			(
				block_height_translator,
				fee_translator,
				ingress_translator,
				old_nonce_translator,
				egress_witnessing_translator,
				vault_swap_translator,
				liveness_translator,
			): (
				BlockHeightTranslator,
				FeeTranslator,
				IngressTranslator,
				OldNonceTrackingTranslator,
				EgressWitnessingTranslator,
				VaultSwapTranslator,
				LivenessTranslator,
			),
			(
				block_height_identifiers,
				fee_identifiers,
				ingress_identifiers,
				old_nonce_identifiers,
				egress_witnessing_identifiers,
				vault_swap_identifiers,
				liveness_identifiers,
			): (
				Vec<
					ElectionIdentifier<
						<SolanaBlockHeightTracking as ElectoralSystem>::ElectionIdentifierExtra,
					>,
				>,
				Vec<
					ElectionIdentifier<
						<SolanaFeeTracking as ElectoralSystem>::ElectionIdentifierExtra,
					>,
				>,
				Vec<
					ElectionIdentifier<
						<SolanaIngressTracking as ElectoralSystem>::ElectionIdentifierExtra,
					>,
				>,
				Vec<
					ElectionIdentifier<
						<old::SolanaNonceTrackingOld as ElectoralSystem>::ElectionIdentifierExtra,
					>,
				>,
				Vec<
					ElectionIdentifier<
						<SolanaEgressWitnessing as ElectoralSystem>::ElectionIdentifierExtra,
					>,
				>,
				Vec<
					ElectionIdentifier<
						<SolanaVaultSwapTracking as ElectoralSystem>::ElectionIdentifierExtra,
					>,
				>,
				Vec<
					ElectionIdentifier<
						<SolanaLiveness as ElectoralSystem>::ElectionIdentifierExtra,
					>,
				>,
			),
			_context: &Self::OnFinalizeContext,
		) -> Result<Self::OnFinalizeReturn, CorruptStorageError> {
			let block_height = SolanaBlockHeightTracking::on_finalize(
				&mut block_height_translator.translate_electoral_access(generic_electoral_access),
				block_height_identifiers,
				&(),
			)?;
			SolanaLiveness::on_finalize(
				&mut liveness_translator.translate_electoral_access(generic_electoral_access),
				liveness_identifiers,
				&(crate::System::block_number(), block_height),
			)?;
			SolanaFeeTracking::on_finalize(
				&mut fee_translator.translate_electoral_access(generic_electoral_access),
				fee_identifiers,
				&(),
			)?;
			SolanaEgressWitnessing::on_finalize(
				&mut egress_witnessing_translator
					.translate_electoral_access(generic_electoral_access),
				egress_witnessing_identifiers,
				&(),
			)?;
			SolanaIngressTracking::on_finalize(
				&mut ingress_translator.translate_electoral_access(generic_electoral_access),
				ingress_identifiers,
				&block_height,
			)?;
			old::SolanaNonceTrackingOld::on_finalize(
				&mut old_nonce_translator.translate_electoral_access(generic_electoral_access),
				old_nonce_identifiers,
				&(),
			)?;
			SolanaVaultSwapTracking::on_finalize(
				&mut vault_swap_translator.translate_electoral_access(generic_electoral_access),
				vault_swap_identifiers,
				&crate::System::current_block_number(),
			)?;
			Ok(())
		}
	}
	#[derive(Encode, Decode, TypeInfo, Clone)]
	struct ElectionBitmapComponents {
		epoch: EpochIndex,
		#[allow(clippy::type_complexity)]
		bitmaps: Vec<(
			<<SolanaElectoralSystem as ElectoralSystem>::Vote as VoteStorage>::BitmapComponent,
			BitVec<u8, bitvec::order::Lsb0>,
		)>, //sp_core::H256, BitVec<u8, bitvec::order::Lsb0>)>,
	}
	#[derive(PartialEq, Eq, Clone, Debug, Encode, Decode, TypeInfo, Default)]
	pub struct ReferenceDetails {
		pub count: u32,
		pub created: u32,
		pub expires: u32,
	}
	pub struct Migration;
	impl OnRuntimeUpgrade for Migration {
		#[cfg(feature = "try-runtime")]
		fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::DispatchError> {
			let election_identifiers = frame_support::migration::storage_key_iter::<
				ElectionIdentifierOf<old::SolanaElectoralSystem>,
				<old::SolanaElectoralSystem as ElectoralSystem>::ElectionProperties,
				Twox64Concat
			>(b"SolanaElections", b"ElectionProperties")
				.filter(|(_, value)| {
					matches!(value, pallet_cf_elections::electoral_systems::composite::tuple_7_impls::CompositeElectionProperties::D(_))
				})
				.map(|(key, value)| {
					log::info!("Old {:?}: {:?}",key, value);
					key
				})
				.collect::<Vec<ElectionIdentifierOf<old::SolanaElectoralSystem>>>();
			log::info!("Number of elections: {:?}", election_identifiers.len());
			Ok((election_identifiers.len() as u32).encode())
		}
		#[cfg(feature = "try-runtime")]
		fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::DispatchError> {
			let previous_number_election = u32::decode(&mut &state[..]).unwrap();
			log::info!("Post upgrade number of election old state: {:?}", previous_number_election);
			log::info!(
				"Post upgrade number of unavailable nonces: {:?}",
				pallet_cf_environment::SolanaUnavailableNonceAccounts::<Runtime>::iter_keys()
					.collect::<Vec<_>>()
					.len() as u32
			);

			assert!(
				previous_number_election ==
					pallet_cf_environment::SolanaUnavailableNonceAccounts::<Runtime>::iter_keys()
						.collect::<Vec<_>>()
						.len() as u32
			);
			let election_identifiers = frame_support::migration::storage_key_iter::<
				ElectionIdentifierOf<super::SolanaElectoralSystem>,
				<super::SolanaElectoralSystem as ElectoralSystem>::ElectionProperties,
				Twox64Concat
			>(b"SolanaElections", b"ElectionProperties")
				.filter(|(_, value)| {
					matches!(value, pallet_cf_elections::electoral_systems::composite::tuple_7_impls::CompositeElectionProperties::D(_))
				})
				.map(|(key, _)| {
					key
				})
				.collect::<Vec<ElectionIdentifierOf<super::SolanaElectoralSystem>>>();
			log::info!("Post upgrade number of elections: {:?}", election_identifiers.len() as u32);
			assert!(previous_number_election == election_identifiers.len() as u32);

			Ok(())
		}
		fn on_runtime_upgrade() -> frame_support::weights::Weight {
			let election_identifiers = frame_support::migration::storage_key_iter::<
				ElectionIdentifierOf<old::SolanaElectoralSystem>,
				<old::SolanaElectoralSystem as ElectoralSystem>::ElectionProperties,
				Twox64Concat
			>(b"SolanaElections", b"ElectionProperties")
				.filter(|(_, value)| {
					matches!(value, pallet_cf_elections::electoral_systems::composite::tuple_7_impls::CompositeElectionProperties::D(_))
				})
				.map(|(key, value)| {
					log::info!("During Upgrade {:?}: {:?}",key, value);
					key
				})
				.collect::<Vec<ElectionIdentifierOf<old::SolanaElectoralSystem>>>();

			for election_identifier in election_identifiers {
				//Removing BitmapComponents
				let bitmap = frame_support::storage::migration::take_storage_item::<
					_,
					ElectionBitmapComponents,
					Twox64Concat,
				>(
					b"SolanaElections",
					b"BitmapComponents",
					election_identifier.unique_monotonic(),
				);
				if bitmap.is_some() {
					log::info!("Bitmap {:?}", bitmap.clone().unwrap().bitmaps);
					//If they have some data, remove the SharedDataRederenceCount as well
					for (bitmap_component, _) in bitmap.unwrap().bitmaps {
						<<SolanaElectoralSystem as ElectoralSystem>::Vote as VoteStorage>::visit_shared_data_references_in_bitmap_component(
							&bitmap_component,
							|shared_data_hash| {
								struct StoragePrefix;
								impl StorageInstance for StoragePrefix{
									const STORAGE_PREFIX: &'static str = "SharedDataReferenceCount";
									fn pallet_prefix() -> &'static str {
										"SolanaElections"
									}
								}
								let hashed_key_and_prefix = StorageDoubleMap::<
									StoragePrefix,
									Identity,
									SharedDataHash,
									Twox64Concat,
									UniqueMonotonicIdentifier,
									(),
									OptionQuery,
								>::hashed_key_for(shared_data_hash, election_identifier.unique_monotonic());
								let reference: core::option::Option<ReferenceDetails> = frame_support::storage::unhashed::take::<ReferenceDetails>(&hashed_key_and_prefix);
								log::info!("References {:?}", reference);
								let shared_data =
									frame_support::storage::migration::take_storage_item::<
										_,
										<<old::SolanaElectoralSystem as ElectoralSystem>::Vote as VoteStorage>::SharedData,
										Identity,
									>(b"SolanaElections", b"SharedData", shared_data_hash);
								log::info!("SharedData {:?}", shared_data);
							}
						);
					}
				}
				let properties =
					frame_support::storage::migration::take_storage_item::<
						_,
						<old::SolanaElectoralSystem as ElectoralSystem>::ElectionProperties,
						Twox64Concat,
					>(b"SolanaElections", b"ElectionProperties", election_identifier);
				log::info!("Properties {:?}", properties);

				let consensus_history = frame_support::storage::migration::take_storage_item::<
					_,
					ConsensusHistory<<old::SolanaElectoralSystem as ElectoralSystem>::Consensus>,
					Twox64Concat,
				>(
					b"SolanaElections",
					b"ElectionConsensusHistory",
					election_identifier.unique_monotonic(),
				);
				log::info!("Consensus history {:?}", consensus_history);

				let settings = frame_support::storage::migration::take_storage_item::<
					_,
					<old::SolanaElectoralSystem as ElectoralSystem>::ElectoralSettings,
					Twox64Concat,
				>(
					b"SolanaElections",
					b"ElectoralSettings",
					election_identifier.unique_monotonic(),
				);
				log::info!("Settings {:?}", settings);

				let consensus_history_uptodate =
					frame_support::storage::migration::take_storage_item::<
						_,
						EpochIndex,
						Twox64Concat,
					>(
						b"SolanaElections",
						b"ElectionConsensusHistoryUpToDate",
						election_identifier.unique_monotonic(),
					);
				log::info!("Consensus history up to date {:?}", consensus_history_uptodate);
			}
			for (key, value) in
				pallet_cf_environment::SolanaUnavailableNonceAccounts::<Runtime>::iter()
			{
				log::info!("Creating a new election for nonce: {:?}, {:?}", key, value);
				let _ = SolanaNonceTrackingTrigger::watch_for_nonce_change(key, value);
			}
			Weight::zero()
		}
	}
}
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
			0,
			(),
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
			SolanaVaultSwapsSettings { swap_endpoint_data_account_address, usdc_token_mint_pubkey },
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

pub type SolanaVaultSwapTracking =
	electoral_systems::solana_swap_accounts_tracking::SolanaVaultSwapAccounts<
		VaultSwapAccountAndSender,
		SolanaVaultSwapDetails,
		BlockNumberFor<Runtime>,
		SolanaVaultSwapsSettings,
		SolanaVaultSwapsHandler,
		<Runtime as Chainflip>::ValidatorId,
		SolanaTransactionBuildingError,
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
		SolanaVaultSwapTracking,
		SolanaLiveness,
	> for SolanaElectionHooks
{
	type OnFinalizeContext = BlockNumberFor<Runtime>;
	type OnFinalizeReturn = ();

	fn on_finalize<
		GenericElectoralAccess,
		BlockHeightTranslator: Translator<GenericElectoralAccess, ElectoralSystem = SolanaBlockHeightTracking>,
		FeeTranslator: Translator<GenericElectoralAccess, ElectoralSystem = SolanaFeeTracking>,
		IngressTranslator: Translator<GenericElectoralAccess, ElectoralSystem = SolanaIngressTracking>,
		NonceTrackingTranslator: Translator<GenericElectoralAccess, ElectoralSystem = SolanaNonceTracking>,
		EgressWitnessingTranslator: Translator<GenericElectoralAccess, ElectoralSystem = SolanaEgressWitnessing>,
		VaultSwapTranslator: Translator<GenericElectoralAccess, ElectoralSystem = SolanaVaultSwapTracking>,
		LivenessTranslator: Translator<GenericElectoralAccess, ElectoralSystem = SolanaLiveness>,
	>(
		generic_electoral_access: &mut GenericElectoralAccess,
		(
			block_height_translator,
			fee_translator,
			ingress_translator,
			nonce_tracking_translator,
			egress_witnessing_translator,
			vault_swap_translator,
			liveness_translator,
		): (
			BlockHeightTranslator,
			FeeTranslator,
			IngressTranslator,
			NonceTrackingTranslator,
			EgressWitnessingTranslator,
			VaultSwapTranslator,
			LivenessTranslator,
		),
		(
			block_height_identifiers,
			fee_identifiers,
			ingress_identifiers,
			nonce_tracking_identifiers,
			egress_witnessing_identifiers,
			vault_swap_identifiers,
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
			Vec<
				ElectionIdentifier<
					<SolanaVaultSwapTracking as ElectoralSystem>::ElectionIdentifierExtra,
				>,
			>,
			Vec<ElectionIdentifier<<SolanaLiveness as ElectoralSystem>::ElectionIdentifierExtra>>,
		),
		context: &Self::OnFinalizeContext,
	) -> Result<Self::OnFinalizeReturn, CorruptStorageError> {
		let block_height = SolanaBlockHeightTracking::on_finalize(
			&mut block_height_translator.translate_electoral_access(generic_electoral_access),
			block_height_identifiers,
			&(),
		)?;
		SolanaLiveness::on_finalize(
			&mut liveness_translator.translate_electoral_access(generic_electoral_access),
			liveness_identifiers,
			&(crate::System::block_number(), block_height),
		)?;
		SolanaFeeTracking::on_finalize(
			&mut fee_translator.translate_electoral_access(generic_electoral_access),
			fee_identifiers,
			&(),
		)?;
		SolanaNonceTracking::on_finalize(
			&mut nonce_tracking_translator.translate_electoral_access(generic_electoral_access),
			nonce_tracking_identifiers,
			&(),
		)?;
		SolanaEgressWitnessing::on_finalize(
			&mut egress_witnessing_translator.translate_electoral_access(generic_electoral_access),
			egress_witnessing_identifiers,
			&(),
		)?;
		SolanaIngressTracking::on_finalize(
			&mut ingress_translator.translate_electoral_access(generic_electoral_access),
			ingress_identifiers,
			&block_height,
		)?;
		SolanaVaultSwapTracking::on_finalize(
			&mut vault_swap_translator.translate_electoral_access(generic_electoral_access),
			vault_swap_identifiers,
			context,
		)?;
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

pub struct SolanaChainTrackingProvider;
impl GetBlockHeight<Solana> for SolanaChainTrackingProvider {
	fn get_block_height() -> <Solana as Chain>::ChainBlockNumber {
		pallet_cf_elections::Pallet::<Runtime, Instance>::with_electoral_access(
			|electoral_access| {
				SolanaElectoralSystem::with_access_translators(|access_translators| {
					let (access_translator, ..) = &access_translators;
					access_translator
						.translate_electoral_access(electoral_access)
						.unsynchronised_state()
				})
			},
		)
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
		pallet_cf_elections::Pallet::<Runtime, Instance>::with_electoral_access(
			|electoral_access| {
				SolanaElectoralSystem::with_access_translators(|access_translators| {
					let (_, access_translator, ..) = &access_translators;
					let electoral_access =
						access_translator.translate_electoral_access(electoral_access);
					electoral_access.unsynchronised_state()
				})
			},
		)
		.ok()
	}

	fn with_tracked_data_then_apply_fee_multiplier<
		F: FnOnce(SolTrackedData) -> <Solana as Chain>::ChainAmount,
	>(
		f: F,
	) -> <Solana as Chain>::ChainAmount {
		pallet_cf_elections::Pallet::<Runtime, Instance>::with_electoral_access(
			|electoral_access| {
				SolanaElectoralSystem::with_access_translators(|access_translators| {
					let (_, access_translator, ..) = &access_translators;
					let electoral_access =
						access_translator.translate_electoral_access(electoral_access);
					Ok(electoral_access
						.unsynchronised_settings()?
						.fee_multiplier
						.saturating_mul_int(f(SolTrackedData {
							priority_fee: electoral_access.unsynchronised_state()?,
						})))
				})
			},
		)
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
		pallet_cf_elections::Pallet::<Runtime, Instance>::with_electoral_access_and_identifiers(
			|electoral_access, election_identifiers| {
				SolanaElectoralSystem::with_identifiers(
					election_identifiers,
					|election_identifiers| {
						SolanaElectoralSystem::with_access_translators(|access_translators| {
							let (_, _, access_translator, ..) = &access_translators;
							let (_, _, election_identifiers, ..) = election_identifiers;
							SolanaIngressTracking::open_channel(
								election_identifiers,
								&mut access_translator.translate_electoral_access(electoral_access),
								channel,
								asset,
								close_block,
							)
						})
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
		pallet_cf_elections::Pallet::<Runtime, Instance>::with_electoral_access(
			|electoral_access| {
				SolanaElectoralSystem::with_access_translators(|access_translators| {
					let (_, _, _, access_translator, ..) = &access_translators;
					let mut electoral_access =
						access_translator.translate_electoral_access(electoral_access);
					SolanaNonceTracking::watch_for_change(
						&mut electoral_access,
						nonce_account,
						previous_nonce_value,
					)
				})
			},
		)
	}
}

pub struct SolanaEgressWitnessingTrigger;

impl ElectionEgressWitnesser for SolanaEgressWitnessingTrigger {
	type Chain = SolanaCrypto;

	fn watch_for_egress_success(signature: SolSignature) -> DispatchResult {
		pallet_cf_elections::Pallet::<Runtime, Instance>::with_electoral_access(
			|electoral_access| {
				SolanaElectoralSystem::with_access_translators(|access_translators| {
					let (_, _, _, _, access_translator, ..) = &access_translators;
					let mut electoral_access =
						access_translator.translate_electoral_access(electoral_access);

					SolanaEgressWitnessing::watch_for_egress(&mut electoral_access, signature)
				})
			},
		)
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
	// TODO: These two will potentially be a TransactionId type
	pub swap_account: SolAddress,
	pub creation_slot: u64,
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
		let _ = SolanaIngressEgress::vault_swap_request(
			RuntimeOrigin::root(),
			swap_details.from.into(),
			swap_details.to,
			swap_details.deposit_amount,
			swap_details.destination_address,
			swap_details.deposit_metadata,
			Default::default(), // todo
			Default::default(),
			Default::default(),
			Default::default(),
			Default::default(),
			Default::default(),
		);
	}

	fn close_accounts(
		accounts: Vec<VaultSwapAccountAndSender>,
	) -> Result<(), SolanaTransactionBuildingError> {
		<SolanaApi<SolEnvironment> as CloseSolanaVaultSwapAccounts>::new_unsigned(accounts).map(
			|apicall| {
				let _ = <SolanaBroadcaster as Broadcaster<Solana>>::threshold_sign_and_broadcast(
					apicall,
				);
			},
		)
	}

	fn get_number_of_available_sol_nonce_accounts() -> usize {
		Environment::get_number_of_available_sol_nonce_accounts()
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
