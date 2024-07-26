use crate::*;
use cf_chains::{
	btc::{
		api::{batch_transfer::BatchTransfer, BitcoinApi},
		BitcoinTransaction,
	},
	eth::api::{register_redemption, update_flip_supply},
	evm::api::{
		all_batch, execute_x_swap_and_call, set_agg_key_with_agg_key, set_comm_key_with_agg_key,
		set_gov_key_with_agg_key, transfer_fallback, EvmTransactionBuilder,
	},
};
use dot::{api::PolkadotApi, PolkadotExtrinsicBuilder};
use frame_support::traits::OnRuntimeUpgrade;
use sp_std::marker::PhantomData;

#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;
#[cfg(feature = "try-runtime")]
use sp_std::{vec, vec::Vec};

pub mod old {

	use sp_std::collections::vec_deque::VecDeque;

	use super::*;
	use cf_chains::{
		btc::{BitcoinOutput, Utxo},
		evm::api::{EvmReplayProtection, SigData},
	};
	use codec::{Decode, Encode, MaxEncodedLen};
	use dot::{PolkadotReplayProtection, PolkadotRuntimeCall, PolkadotSignature};
	use frame_support::{CloneNoBound, DebugNoBound, EqNoBound, Never, PartialEqNoBound};
	use scale_info::TypeInfo;
	use sp_core::RuntimeDebug;

	#[derive(Encode, Decode, TypeInfo, MaxEncodedLen, Clone, RuntimeDebug, PartialEq, Eq)]
	pub struct EvmTransactionBuilder<C> {
		pub sig_data: Option<SigData>,
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
		ExecutexSwapAndCall(EvmTransactionBuilder<execute_x_swap_and_call::ExecutexSwapAndCall>),
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
		ExecutexSwapAndCall(EvmTransactionBuilder<execute_x_swap_and_call::ExecutexSwapAndCall>),
		TransferFallback(EvmTransactionBuilder<transfer_fallback::TransferFallback>),
		#[doc(hidden)]
		#[codec(skip)]
		_Phantom(PhantomData<Environment>, Never),
	}

	#[derive(Debug, Encode, Decode, TypeInfo, Eq, PartialEq, Clone)]
	pub struct PolkadotExtrinsicBuilder {
		pub extrinsic_call: PolkadotRuntimeCall,
		pub replay_protection: PolkadotReplayProtection,
		pub signature: Option<PolkadotSignature>,
	}

	#[derive(CloneNoBound, DebugNoBound, PartialEqNoBound, EqNoBound, Encode, Decode, TypeInfo)]
	#[scale_info(skip_type_params(Environment))]
	pub enum PolkadotApi<Environment: 'static> {
		BatchFetchAndTransfer(PolkadotExtrinsicBuilder),
		RotateVaultProxy(PolkadotExtrinsicBuilder),
		ChangeGovKey(PolkadotExtrinsicBuilder),
		ExecuteXSwapAndCall(PolkadotExtrinsicBuilder),
		#[doc(hidden)]
		#[codec(skip)]
		_Phantom(PhantomData<Environment>, Never),
	}

	#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq)]
	pub struct BitcoinTransaction {
		pub inputs: Vec<Utxo>,
		pub outputs: Vec<BitcoinOutput>,
		pub signatures: Vec<cf_chains::btc::Signature>,
		pub transaction_bytes: Vec<u8>,
		pub old_utxo_input_indices: VecDeque<u32>,
	}

	#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq)]
	pub struct BatchTransfer {
		pub bitcoin_transaction: BitcoinTransaction,
		pub change_utxo_key: [u8; 32],
	}

	#[derive(CloneNoBound, DebugNoBound, PartialEqNoBound, EqNoBound, Encode, Decode, TypeInfo)]
	#[scale_info(skip_type_params(Environment))]
	pub enum BitcoinApi<Environment: 'static> {
		BatchTransfer(BatchTransfer),
		#[doc(hidden)]
		#[codec(skip)]
		_Phantom(PhantomData<Environment>, Never),
	}
}

pub struct MigrateApicallsAndOnChainKey;

impl OnRuntimeUpgrade for MigrateApicallsAndOnChainKey {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		let current_evm_key = pallet_cf_threshold_signature::Keys::<Runtime, Instance16>::get(
			pallet_cf_threshold_signature::CurrentKeyEpoch::<Runtime, Instance16>::get().unwrap(),
		)
		.unwrap();

		let current_dot_key = pallet_cf_threshold_signature::Keys::<Runtime, Instance2>::get(
			pallet_cf_threshold_signature::CurrentKeyEpoch::<Runtime, Instance2>::get().unwrap(),
		)
		.unwrap();

		let current_btc_key = pallet_cf_threshold_signature::Keys::<Runtime, Instance3>::get(
			pallet_cf_threshold_signature::CurrentKeyEpoch::<Runtime, Instance3>::get().unwrap(),
		)
		.unwrap();

		pallet_cf_broadcast::CurrentOnChainKey::<Runtime, Instance1>::put(current_evm_key);
		pallet_cf_broadcast::CurrentOnChainKey::<Runtime, Instance4>::put(current_evm_key);
		pallet_cf_broadcast::CurrentOnChainKey::<Runtime, Instance2>::put(current_dot_key);
		pallet_cf_broadcast::CurrentOnChainKey::<Runtime, Instance3>::put(current_btc_key);

		fn evm_tx_builder_fn<C>(
			evm_tx_builder: old::EvmTransactionBuilder<C>,
			current_evm_key: cf_chains::evm::AggKey,
		) -> EvmTransactionBuilder<C> {
			EvmTransactionBuilder {
				signer_and_sig_data: evm_tx_builder
					.sig_data
					.map(|sig_data| (current_evm_key, sig_data)),
				replay_protection: evm_tx_builder.replay_protection,
				call: evm_tx_builder.call,
			}
		}

		pallet_cf_broadcast::PendingApiCalls::<Runtime, Instance1>::translate(
			|_broadcast_id, old_apicall: old::EthereumApi<EvmEnvironment>| {
				Some(match old_apicall {
					old::EthereumApi::SetAggKeyWithAggKey(evm_tx_builder) =>
						EthereumApi::SetAggKeyWithAggKey(evm_tx_builder_fn(
							evm_tx_builder,
							current_evm_key,
						)),
					old::EthereumApi::RegisterRedemption(evm_tx_builder) =>
						EthereumApi::RegisterRedemption(evm_tx_builder_fn(
							evm_tx_builder,
							current_evm_key,
						)),
					old::EthereumApi::UpdateFlipSupply(evm_tx_builder) =>
						EthereumApi::UpdateFlipSupply(evm_tx_builder_fn(
							evm_tx_builder,
							current_evm_key,
						)),
					old::EthereumApi::SetGovKeyWithAggKey(evm_tx_builder) =>
						EthereumApi::SetGovKeyWithAggKey(evm_tx_builder_fn(
							evm_tx_builder,
							current_evm_key,
						)),
					old::EthereumApi::SetCommKeyWithAggKey(evm_tx_builder) =>
						EthereumApi::SetCommKeyWithAggKey(evm_tx_builder_fn(
							evm_tx_builder,
							current_evm_key,
						)),
					old::EthereumApi::AllBatch(evm_tx_builder) =>
						EthereumApi::AllBatch(evm_tx_builder_fn(evm_tx_builder, current_evm_key)),
					old::EthereumApi::ExecutexSwapAndCall(evm_tx_builder) =>
						EthereumApi::ExecutexSwapAndCall(evm_tx_builder_fn(
							evm_tx_builder,
							current_evm_key,
						)),
					old::EthereumApi::TransferFallback(evm_tx_builder) =>
						EthereumApi::TransferFallback(evm_tx_builder_fn(
							evm_tx_builder,
							current_evm_key,
						)),
					old::EthereumApi::_Phantom(..) => unreachable!(),
				})
			},
		);

		pallet_cf_broadcast::PendingApiCalls::<Runtime, Instance4>::translate(
			|_broadcast_id, old_apicall: old::ArbitrumApi<EvmEnvironment>| {
				Some(match old_apicall {
					old::ArbitrumApi::SetAggKeyWithAggKey(evm_tx_builder) =>
						ArbitrumApi::SetAggKeyWithAggKey(evm_tx_builder_fn(
							evm_tx_builder,
							current_evm_key,
						)),

					old::ArbitrumApi::AllBatch(evm_tx_builder) =>
						ArbitrumApi::AllBatch(evm_tx_builder_fn(evm_tx_builder, current_evm_key)),
					old::ArbitrumApi::ExecutexSwapAndCall(evm_tx_builder) =>
						ArbitrumApi::ExecutexSwapAndCall(evm_tx_builder_fn(
							evm_tx_builder,
							current_evm_key,
						)),
					old::ArbitrumApi::TransferFallback(evm_tx_builder) =>
						ArbitrumApi::TransferFallback(evm_tx_builder_fn(
							evm_tx_builder,
							current_evm_key,
						)),
					old::ArbitrumApi::_Phantom(..) => unreachable!(),
				})
			},
		);

		fn dot_tx_builder_fn(
			dot_ext_builder: old::PolkadotExtrinsicBuilder,
			current_dot_key: cf_chains::dot::PolkadotPublicKey,
		) -> PolkadotExtrinsicBuilder {
			PolkadotExtrinsicBuilder {
				signer_and_signature: dot_ext_builder
					.signature
					.map(|signature| (current_dot_key, signature)),
				replay_protection: dot_ext_builder.replay_protection,
				extrinsic_call: dot_ext_builder.extrinsic_call,
			}
		}

		pallet_cf_broadcast::PendingApiCalls::<Runtime, Instance2>::translate(
			|_broadcast_id, old_apicall: old::PolkadotApi<DotEnvironment>| {
				Some(match old_apicall {
					old::PolkadotApi::BatchFetchAndTransfer(dot_ext_builder) =>
						PolkadotApi::BatchFetchAndTransfer(dot_tx_builder_fn(
							dot_ext_builder,
							current_dot_key,
						)),

					old::PolkadotApi::RotateVaultProxy(dot_ext_builder) =>
						PolkadotApi::RotateVaultProxy(dot_tx_builder_fn(
							dot_ext_builder,
							current_dot_key,
						)),
					old::PolkadotApi::ChangeGovKey(dot_ext_builder) => PolkadotApi::ChangeGovKey(
						dot_tx_builder_fn(dot_ext_builder, current_dot_key),
					),
					old::PolkadotApi::ExecuteXSwapAndCall(dot_ext_builder) =>
						PolkadotApi::ExecuteXSwapAndCall(dot_tx_builder_fn(
							dot_ext_builder,
							current_dot_key,
						)),
					old::PolkadotApi::_Phantom(..) => unreachable!(),
				})
			},
		);

		pallet_cf_broadcast::PendingApiCalls::<Runtime, Instance3>::translate(
			|_broadcast_id, old_apicall: old::BitcoinApi<BtcEnvironment>| {
				Some(match old_apicall {
					old::BitcoinApi::BatchTransfer(old_batch_transfer) =>
						BitcoinApi::BatchTransfer(BatchTransfer {
							bitcoin_transaction: BitcoinTransaction {
								inputs: old_batch_transfer.bitcoin_transaction.inputs,
								outputs: old_batch_transfer.bitcoin_transaction.outputs,
								transaction_bytes: old_batch_transfer
									.bitcoin_transaction
									.transaction_bytes,
								old_utxo_input_indices: old_batch_transfer
									.bitcoin_transaction
									.old_utxo_input_indices,
								// The signature here is a valid signature since this storage item
								// only stores signed calls
								signer_and_signatures: Some((
									current_btc_key,
									old_batch_transfer.bitcoin_transaction.signatures,
								)),
							},
							change_utxo_key: old_batch_transfer.change_utxo_key,
						}),

					old::BitcoinApi::_Phantom(..) => unreachable!(),
				})
			},
		);

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
