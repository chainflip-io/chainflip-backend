use crate::*;
use cf_chains::{
	eth::api::{register_redemption, update_flip_supply},
	evm::api::{
		all_batch, common::EncodableTransferAssetParams, execute_x_swap_and_call,
		set_agg_key_with_agg_key, set_comm_key_with_agg_key, set_gov_key_with_agg_key,
		transfer_fallback, EvmTransactionBuilder,
	},
};
use frame_support::traits::UncheckedOnRuntimeUpgrade;
use sp_std::marker::PhantomData;

#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;
#[cfg(feature = "try-runtime")]
use sp_std::{vec, vec::Vec};

pub mod old {

	use super::*;
	use cf_chains::{
		evm::{
			api::{EvmReplayProtection, SigData},
			AggKey,
		},
		Chain,
	};
	use codec::{Decode, Encode, MaxEncodedLen};
	use frame_support::{CloneNoBound, DebugNoBound, EqNoBound, Never, PartialEqNoBound};
	use scale_info::TypeInfo;
	use sp_core::RuntimeDebug;

	#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq)]
	pub struct ExecutexSwapAndCall {
		pub transfer_param: EncodableTransferAssetParams,
		pub source_chain: u32,
		pub source_address: Vec<u8>,
		pub gas_budget: <Ethereum as Chain>::ChainAmount,
		pub message: Vec<u8>,
	}

	#[derive(Encode, Decode, TypeInfo, MaxEncodedLen, Clone, RuntimeDebug, PartialEq, Eq)]
	pub struct EvmTransactionBuilder<C> {
		pub signer_and_sig_data: Option<(AggKey, SigData)>,
		pub replay_protection: EvmReplayProtection,
		pub call: C,
	}

	#[derive(CloneNoBound, DebugNoBound, PartialEqNoBound, EqNoBound, Encode, Decode, TypeInfo)]
	#[scale_info(skip_type_params(Environment))]
	pub enum EthereumApi<Environment: 'static> {
		SetAggKeyWithAggKey(EvmTransactionBuilder<set_agg_key_with_agg_key::SetAggKeyWithAggKey>),
		RegisterRedemption(EvmTransactionBuilder<register_redemption::RegisterRedemption>),
		UpdateFlipSupply(EvmTransactionBuilder<update_flip_supply::UpdateFlipSupply>),
		SetGovKeyWithAggKey(EvmTransactionBuilder<set_gov_key_with_agg_key::SetGovKeyWithAggKey>),
		SetCommKeyWithAggKey(
			EvmTransactionBuilder<set_comm_key_with_agg_key::SetCommKeyWithAggKey>,
		),
		AllBatch(EvmTransactionBuilder<all_batch::AllBatch>),
		ExecutexSwapAndCall(EvmTransactionBuilder<ExecutexSwapAndCall>),
		TransferFallback(EvmTransactionBuilder<transfer_fallback::TransferFallback>),
		#[doc(hidden)]
		#[codec(skip)]
		_Phantom(PhantomData<Environment>, Never),
	}

	#[derive(CloneNoBound, DebugNoBound, PartialEqNoBound, EqNoBound, Encode, Decode, TypeInfo)]
	#[scale_info(skip_type_params(Environment))]
	pub enum ArbitrumApi<Environment: 'static> {
		SetAggKeyWithAggKey(EvmTransactionBuilder<set_agg_key_with_agg_key::SetAggKeyWithAggKey>),
		AllBatch(EvmTransactionBuilder<all_batch::AllBatch>),
		ExecutexSwapAndCall(EvmTransactionBuilder<ExecutexSwapAndCall>),
		TransferFallback(EvmTransactionBuilder<transfer_fallback::TransferFallback>),
		#[doc(hidden)]
		#[codec(skip)]
		_Phantom(PhantomData<Environment>, Never),
	}
}

fn evm_tx_builder_fn<C>(evm_tx_builder: old::EvmTransactionBuilder<C>) -> EvmTransactionBuilder<C> {
	EvmTransactionBuilder {
		signer_and_sig_data: evm_tx_builder.signer_and_sig_data,
		replay_protection: evm_tx_builder.replay_protection,
		call: evm_tx_builder.call,
	}
}

fn evm_tx_builder_execute_x_swap(
	evm_tx_builder: old::EvmTransactionBuilder<old::ExecutexSwapAndCall>,
	is_arbitrum: bool,
) -> EvmTransactionBuilder<execute_x_swap_and_call::ExecutexSwapAndCall> {
	EvmTransactionBuilder {
		signer_and_sig_data: evm_tx_builder.signer_and_sig_data,
		replay_protection: evm_tx_builder.replay_protection,
		call: execute_x_swap_and_call::ExecutexSwapAndCall {
			transfer_param: evm_tx_builder.call.transfer_param,
			source_chain: evm_tx_builder.call.source_chain,
			source_address: evm_tx_builder.call.source_address,
			// Setting a reasonable default for gas budget
			gas_budget: match is_arbitrum {
				true => 1_500_000_u128,
				false => 300_000_u128,
			},
			message: evm_tx_builder.call.message,
		},
	}
}

pub struct EthApiCallsGasMigration;

impl UncheckedOnRuntimeUpgrade for EthApiCallsGasMigration {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		pallet_cf_broadcast::PendingApiCalls::<Runtime, Instance1>::translate(
			|_broadcast_id, old_apicall: old::EthereumApi<EvmEnvironment>| {
				Some(match old_apicall {
					old::EthereumApi::SetAggKeyWithAggKey(evm_tx_builder) =>
						EthereumApi::SetAggKeyWithAggKey(evm_tx_builder_fn(evm_tx_builder)),
					old::EthereumApi::RegisterRedemption(evm_tx_builder) =>
						EthereumApi::RegisterRedemption(evm_tx_builder_fn(evm_tx_builder)),
					old::EthereumApi::UpdateFlipSupply(evm_tx_builder) =>
						EthereumApi::UpdateFlipSupply(evm_tx_builder_fn(evm_tx_builder)),
					old::EthereumApi::SetGovKeyWithAggKey(evm_tx_builder) =>
						EthereumApi::SetGovKeyWithAggKey(evm_tx_builder_fn(evm_tx_builder)),
					old::EthereumApi::SetCommKeyWithAggKey(evm_tx_builder) =>
						EthereumApi::SetCommKeyWithAggKey(evm_tx_builder_fn(evm_tx_builder)),
					old::EthereumApi::AllBatch(evm_tx_builder) =>
						EthereumApi::AllBatch(evm_tx_builder_fn(evm_tx_builder)),
					old::EthereumApi::ExecutexSwapAndCall(evm_tx_builder) =>
						EthereumApi::ExecutexSwapAndCall(evm_tx_builder_execute_x_swap(
							evm_tx_builder,
							false,
						)),
					old::EthereumApi::TransferFallback(evm_tx_builder) =>
						EthereumApi::TransferFallback(evm_tx_builder_fn(evm_tx_builder)),
					old::EthereumApi::_Phantom(..) => unreachable!(),
				})
			},
		);

		Weight::zero()
	}
}

pub struct ArbApiCallsGasMigration;

impl UncheckedOnRuntimeUpgrade for ArbApiCallsGasMigration {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		pallet_cf_broadcast::PendingApiCalls::<Runtime, Instance1>::translate(
			|_broadcast_id, old_apicall: old::EthereumApi<EvmEnvironment>| {
				Some(match old_apicall {
					old::EthereumApi::SetAggKeyWithAggKey(evm_tx_builder) =>
						EthereumApi::SetAggKeyWithAggKey(evm_tx_builder_fn(evm_tx_builder)),
					old::EthereumApi::RegisterRedemption(evm_tx_builder) =>
						EthereumApi::RegisterRedemption(evm_tx_builder_fn(evm_tx_builder)),
					old::EthereumApi::UpdateFlipSupply(evm_tx_builder) =>
						EthereumApi::UpdateFlipSupply(evm_tx_builder_fn(evm_tx_builder)),
					old::EthereumApi::SetGovKeyWithAggKey(evm_tx_builder) =>
						EthereumApi::SetGovKeyWithAggKey(evm_tx_builder_fn(evm_tx_builder)),
					old::EthereumApi::SetCommKeyWithAggKey(evm_tx_builder) =>
						EthereumApi::SetCommKeyWithAggKey(evm_tx_builder_fn(evm_tx_builder)),
					old::EthereumApi::AllBatch(evm_tx_builder) =>
						EthereumApi::AllBatch(evm_tx_builder_fn(evm_tx_builder)),
					old::EthereumApi::ExecutexSwapAndCall(evm_tx_builder) =>
						EthereumApi::ExecutexSwapAndCall(evm_tx_builder_execute_x_swap(
							evm_tx_builder,
							true,
						)),
					old::EthereumApi::TransferFallback(evm_tx_builder) =>
						EthereumApi::TransferFallback(evm_tx_builder_fn(evm_tx_builder)),
					old::EthereumApi::_Phantom(..) => unreachable!(),
				})
			},
		);

		Weight::zero()
	}
}

pub struct NoOpMigration;

impl UncheckedOnRuntimeUpgrade for NoOpMigration {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		Ok(vec![])
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		Ok(())
	}
}
