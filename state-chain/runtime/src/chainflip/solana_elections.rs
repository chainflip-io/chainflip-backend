use crate::{Environment, Runtime, SolanaBroadcaster, SolanaChainTracking, SolanaThresholdSigner};
use cf_chains::{
	instances::ChainInstanceAlias,
	sol::{
		api::{SolanaApi, SolanaTransactionBuildingError},
		SolAddress, SolAmount, SolHash, SolSignature, SolTrackedData, SolanaCrypto,
	},
	Chain, FeeEstimationApi, Solana,
};
use cf_runtime_utilities::log_or_panic;
use cf_traits::{
	AdjustedFeeEstimationApi, Broadcaster, Chainflip, ElectionEgressWitnesser, GetBlockHeight,
	IngressSource, SolanaNonceWatch,
};

use crate::{RuntimeOrigin, SolanaIngressEgress};
use cf_chains::{
	address::EncodedAddress, assets::any::Asset, sol::api::ContractSwapAccountAndSender,
	CloseSolanaVaultSwapAccounts,
};
use cf_primitives::{AssetAmount, TransactionHash};
use codec::{Decode, Encode};
use frame_system::pallet_prelude::BlockNumberFor;
use pallet_cf_elections::{
	electoral_system::{ElectoralReadAccess, ElectoralSystem},
	electoral_systems::{
		self,
		change::OnChangeHook,
		composite::{tuple_6_impls::Hooks, Composite, Translator},
		egress_success::OnEgressSuccess,
		monotonic_median::MedianChangeHook,
		solana_swap_accounts_tracking::{
			SolanaVaultSwapAccountsHook, SolanaVaultSwapsElectoralState,
		},
	},
	CorruptStorageError, ElectionIdentifier, InitialState, InitialStateOf,
};
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_runtime::{DispatchResult, FixedPointNumber, FixedU128};
use sp_std::{collections::btree_set::BTreeSet, vec::Vec};

#[cfg(feature = "runtime-benchmarks")]
use cf_chains::benchmarking_value::BenchmarkValue;

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
	),
	<Runtime as Chainflip>::ValidatorId,
	SolanaElectionHooks,
>;

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
			SolanaVaultSwapsElectoralState {
				block_number_last_closed_accounts: 0,
				witnessed_open_accounts: vec![],
				closure_initiated_accounts: BTreeSet::new(),
			},
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
			(),
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

pub type SolanaNonceTracking = electoral_systems::change::Change<
	SolAddress,
	SolHash,
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
		ContractSwapAccountAndSender,
		SolanaVaultSwapDetails,
		BlockNumberFor<Runtime>,
		(),
		SolanaVaultSwapsHandler,
		<Runtime as Chainflip>::ValidatorId,
		SolanaTransactionBuildingError,
	>;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Encode, Decode, TypeInfo)]
pub struct TransactionSuccessDetails {
	pub tx_fee: u64,
}

pub struct SolanaEgressWitnessingHook;

impl OnEgressSuccess<SolSignature, TransactionSuccessDetails> for SolanaEgressWitnessingHook {
	fn on_egress_success(
		signature: SolSignature,
		TransactionSuccessDetails { tx_fee }: TransactionSuccessDetails,
	) {
		use cf_traits::KeyProvider;

		if let Err(err) = SolanaBroadcaster::egress_success(
			pallet_cf_witnesser::RawOrigin::CurrentEpochWitnessThreshold.into(),
			signature,
			// Assign any owed fees to the current key.
			SolanaThresholdSigner::active_epoch_key().map(|e| e.key).unwrap_or_default(),
			tx_fee,
			(),
			signature,
		) {
			log::error!("Failed to execute egress success: {:?}", err);
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
	>(
		generic_electoral_access: &mut GenericElectoralAccess,
		(
			block_height_translator,
			fee_translator,
			ingress_translator,
			nonce_tracking_translator,
			egress_witnessing_translator,
			vault_swap_translator,
		): (
			BlockHeightTranslator,
			FeeTranslator,
			IngressTranslator,
			NonceTrackingTranslator,
			EgressWitnessingTranslator,
			VaultSwapTranslator,
		),
		(
			block_height_identifiers,
			fee_identifiers,
			ingress_identifiers,
			nonce_tracking_identifiers,
			egress_witnessing_identifiers,
			vault_swap_identifiers,
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
		),
		context: &Self::OnFinalizeContext,
	) -> Result<Self::OnFinalizeReturn, CorruptStorageError> {
		let block_height = SolanaBlockHeightTracking::on_finalize(
			&mut block_height_translator.translate_electoral_access(generic_electoral_access),
			block_height_identifiers,
			&(),
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
	from: Asset,
	to: Asset,
	deposit_amount: AssetAmount,
	destination_address: EncodedAddress,
	tx_hash: TransactionHash,
}
pub struct SolanaVaultSwapsHandler;

impl
	SolanaVaultSwapAccountsHook<
		ContractSwapAccountAndSender,
		SolanaVaultSwapDetails,
		SolanaTransactionBuildingError,
	> for SolanaVaultSwapsHandler
{
	fn initiate_vault_swap(swap_details: SolanaVaultSwapDetails) {
		let _ = SolanaIngressEgress::contract_swap_request(
			RuntimeOrigin::root(),
			swap_details.from,
			swap_details.to,
			swap_details.deposit_amount,
			swap_details.destination_address,
			swap_details.tx_hash,
		);
	}

	fn close_accounts(
		accounts: Vec<ContractSwapAccountAndSender>,
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
