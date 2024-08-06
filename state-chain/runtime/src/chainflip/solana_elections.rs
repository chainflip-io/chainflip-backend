use crate::Runtime;
use cf_chains::{
	instances::ChainInstanceAlias, sol::SolTrackedData, Chain, FeeEstimationApi, Solana,
};
use cf_runtime_utilities::log_or_panic;
use cf_traits::{AdjustedFeeEstimationApi, GetBlockHeight, IngressSource};
use codec::{Decode, Encode};
use pallet_cf_elections::{
	electoral_system::{ElectoralReadAccess, ElectoralSystem},
	electoral_systems::{
		self,
		composite::{tuple_3_impls::Hooks, Composite, Translator},
	},
	CorruptStorageError, ElectionIdentifier,
};
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_runtime::{DispatchResult, FixedPointNumber, FixedU128};
use sp_std::vec::Vec;

type Instance = <Solana as ChainInstanceAlias>::Instance;

pub type SolanaElectoralSystem = Composite<
	(SolanaBlockHeightTracking, SolanaFeeTracking, SolanaIngressTracking),
	SolanaElectionHooks,
>;

type SolanaBlockHeightTracking =
	electoral_systems::median::MonotonicMedian<<Solana as Chain>::ChainBlockNumber, ()>;
type SolanaFeeTracking =
	electoral_systems::median::UnsafeMedian<<Solana as Chain>::ChainAmount, SolanaFeeSettings, ()>;
type SolanaIngressTracking = electoral_systems::blockchain::delta_based_ingress::DeltaBasedIngress<
	pallet_cf_ingress_egress::Pallet<Runtime, Instance>,
>;

pub struct SolanaElectionHooks;

impl Hooks<SolanaBlockHeightTracking, SolanaFeeTracking, SolanaIngressTracking>
	for SolanaElectionHooks
{
	type OnFinalizeContext = ();
	type OnFinalizeReturn = ();

	fn on_finalize<
		GenericElectoralAccess,
		BlockHeightTranslator: Translator<GenericElectoralAccess, ElectoralSystem = SolanaBlockHeightTracking>,
		FeeTranslator: Translator<GenericElectoralAccess, ElectoralSystem = SolanaFeeTracking>,
		IngressTranslator: Translator<GenericElectoralAccess, ElectoralSystem = SolanaIngressTracking>,
	>(
		generic_electoral_access: &mut GenericElectoralAccess,
		(block_height_translator, fee_translator, ingress_translator): (
			BlockHeightTranslator,
			FeeTranslator,
			IngressTranslator,
		),
		(block_height_identifiers, fee_identifiers, ingress_identifiers): (
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
		SolanaIngressTracking::on_finalize(
			&mut ingress_translator.translate_electoral_access(generic_electoral_access),
			ingress_identifiers,
			&block_height,
		)?;
		Ok(())
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize)]
pub struct SolanaFeeSettings {
	pub fee_multiplier: FixedU128,
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
							let (_, _, access_translator) = &access_translators;
							let (_, _, election_identifiers) = election_identifiers;
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
