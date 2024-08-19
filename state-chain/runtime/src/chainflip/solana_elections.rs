use crate::{Environment, Runtime};
use cf_chains::{
	instances::ChainInstanceAlias,
	sol::{SolAddress, SolAmount, SolHash, SolTrackedData},
	Chain, FeeEstimationApi, Solana,
};
use cf_runtime_utilities::log_or_panic;
use cf_traits::{AdjustedFeeEstimationApi, GetBlockHeight, IngressSource, SolanaNonceWatch};
use codec::{Decode, Encode};
use pallet_cf_elections::{
	electoral_system::{ElectoralReadAccess, ElectoralSystem},
	electoral_systems::{
		self,
		change::OnChangeHook,
		composite::{tuple_4_impls::Hooks, Composite, Translator},
	},
	CorruptStorageError, ElectionIdentifier, InitialState, InitialStateOf,
};

use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_runtime::{DispatchResult, FixedPointNumber, FixedU128};
use sp_std::vec::Vec;

type Instance = <Solana as ChainInstanceAlias>::Instance;

pub type SolanaElectoralSystem = Composite<
	(SolanaBlockHeightTracking, SolanaFeeTracking, SolanaIngressTracking, SolanaNonceTracking),
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
		),
		unsynchronised_settings: (
			(),
			SolanaFeeUnsynchronisedSettings { fee_multiplier: FixedU128::from_u32(1u32) },
			(),
		),
		settings: ((), (), SolanaIngressSettings { vault_program, usdc_token_mint_pubkey }),
	}
}

pub type SolanaBlockHeightTracking =
	electoral_systems::median::MonotonicMedian<<Solana as Chain>::ChainBlockNumber, ()>;
pub type SolanaFeeTracking = electoral_systems::median::UnsafeMedian<
	<Solana as Chain>::ChainAmount,
	SolanaFeeUnsynchronisedSettings,
	(),
>;
pub type SolanaIngressTracking =
	electoral_systems::blockchain::delta_based_ingress::DeltaBasedIngress<
		pallet_cf_ingress_egress::Pallet<Runtime, Instance>,
		SolanaIngressSettings,
	>;

pub type SolanaNonceTracking =
	electoral_systems::change::Change<SolAddress, SolHash, (), SolanaNonceTrackingHook>;

pub struct SolanaNonceTrackingHook;

impl OnChangeHook<SolAddress, SolHash> for SolanaNonceTrackingHook {
	fn on_change(nonce_account: SolAddress, durable_nonce: SolHash) {
		Environment::update_sol_nonce(nonce_account, durable_nonce);
	}
}

pub struct SolanaElectionHooks;

impl Hooks<SolanaBlockHeightTracking, SolanaFeeTracking, SolanaIngressTracking, SolanaNonceTracking>
	for SolanaElectionHooks
{
	type OnFinalizeContext = ();
	type OnFinalizeReturn = ();

	fn on_finalize<
		GenericElectoralAccess,
		BlockHeightTranslator: Translator<GenericElectoralAccess, ElectoralSystem = SolanaBlockHeightTracking>,
		FeeTranslator: Translator<GenericElectoralAccess, ElectoralSystem = SolanaFeeTracking>,
		IngressTranslator: Translator<GenericElectoralAccess, ElectoralSystem = SolanaIngressTracking>,
		NonceTrackingTranslator: Translator<GenericElectoralAccess, ElectoralSystem = SolanaNonceTracking>,
	>(
		generic_electoral_access: &mut GenericElectoralAccess,
		(block_height_translator, fee_translator, ingress_translator, nonce_tracking_translator): (
			BlockHeightTranslator,
			FeeTranslator,
			IngressTranslator,
			NonceTrackingTranslator,
		),
		(
			block_height_identifiers,
			fee_identifiers,
			ingress_identifiers,
			nonce_tracking_identifiers,
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
		),
		_context: &Self::OnFinalizeContext,
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
		SolanaIngressTracking::on_finalize(
			&mut ingress_translator.translate_electoral_access(generic_electoral_access),
			ingress_identifiers,
			&block_height,
		)?;
		Ok(())
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize)]
pub struct SolanaFeeUnsynchronisedSettings {
	pub fee_multiplier: FixedU128,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize)]
pub struct SolanaIngressSettings {
	pub vault_program: SolAddress,
	pub usdc_token_mint_pubkey: SolAddress,
}

pub struct SolanaChainTracking;
impl GetBlockHeight<Solana> for SolanaChainTracking {
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
impl SolanaChainTracking {
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
impl AdjustedFeeEstimationApi<Solana> for SolanaChainTracking {
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
					let (_, _, _, access_translator) = &access_translators;
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
