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
use frame_support::{traits::OnRuntimeUpgrade, Twox64Concat};
use pallet_cf_broadcast::ThresholdSignatureFor;
use sp_std::marker::PhantomData;

#[cfg(feature = "try-runtime")]
use cf_chains::ApiCall;

#[cfg(feature = "try-runtime")]
use codec::Decode;
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;
#[cfg(feature = "try-runtime")]
use sp_std::{vec, vec::Vec};

pub mod next {

	use pallet_cf_broadcast::ApiCallFor;

	use super::*;

	// This is deleted in the 5->6 migration on the broadcast pallet
	#[frame_support::storage_alias]
	pub type ThresholdSignatureData<T: pallet_cf_broadcast::Config<I>, I: 'static> = StorageMap<
		pallet_cf_broadcast::Pallet<T, I>,
		Twox64Concat,
		BroadcastId,
		(ApiCallFor<T, I>, ThresholdSignatureFor<T, I>),
	>;
}

pub mod old {

	use sp_std::collections::vec_deque::VecDeque;

	use super::*;
	use cf_chains::{
		btc::{BitcoinOutput, Utxo},
		dot::{PolkadotReplayProtection, PolkadotRuntimeCall, PolkadotSignature},
		evm::api::{EvmCall, EvmReplayProtection, SigData},
	};
	use codec::{Decode, Encode, MaxEncodedLen};
	use frame_support::{
		CloneNoBound, DebugNoBound, EqNoBound, Never, PartialEqNoBound, Twox64Concat,
	};

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
	impl<E> EthereumApi<E> {
		pub fn chain_encoded(&self) -> Vec<u8> {
			crate::eth_map_over_api_variants_old!(
				self,
				tx,
				tx.call.abi_encoded(&tx.sig_data.unwrap())
			)
		}
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
	impl<E> ArbitrumApi<E> {
		pub fn chain_encoded(&self) -> Vec<u8> {
			crate::arb_map_over_api_variants_old!(
				self,
				tx,
				tx.call.abi_encoded(&tx.sig_data.unwrap())
			)
		}
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

	impl<E> PolkadotApi<E> {
		pub fn unwrap(&self) -> PolkadotExtrinsicBuilder {
			match self {
				PolkadotApi::BatchFetchAndTransfer(ext) => ext.clone(),
				PolkadotApi::RotateVaultProxy(ext) => ext.clone(),
				PolkadotApi::ChangeGovKey(ext) => ext.clone(),
				PolkadotApi::ExecuteXSwapAndCall(ext) => ext.clone(),
				PolkadotApi::_Phantom(..) => unreachable!(),
			}
		}
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

	impl<E> BitcoinApi<E> {
		pub fn unwrap(&self) -> BatchTransfer {
			match self {
				BitcoinApi::BatchTransfer(call) => call.clone(),

				BitcoinApi::_Phantom(..) => unreachable!(),
			}
		}
	}

	pub mod btc {
		use super::*;

		#[frame_support::storage_alias]
		pub type ThresholdSignatureData<T: Config<BitcoinInstance>, BitcoinInstance> = StorageMap<
			BitcoinBroadcaster,
			Twox64Concat,
			BroadcastId,
			// Note: Using the old bitcoin api
			(BitcoinApi<BtcEnvironment>, ThresholdSignatureFor<T, BitcoinInstance>),
		>;
	}

	pub mod eth {

		use super::*;

		#[frame_support::storage_alias]
		pub type ThresholdSignatureData<T: Config<EthereumInstance>, EthereumInstance> = StorageMap<
			EthereumBroadcaster,
			Twox64Concat,
			BroadcastId,
			// Note: Using the old ethereum api
			(EthereumApi<EvmEnvironment>, ThresholdSignatureFor<T, EthereumInstance>),
		>;
	}

	pub mod dot {
		use super::*;

		#[frame_support::storage_alias]
		pub type ThresholdSignatureData<T: Config<PolkadotInstance>, PolkadotInstance> = StorageMap<
			PolkadotBroadcaster,
			Twox64Concat,
			BroadcastId,
			// Note: Using the old polkadot api
			(PolkadotApi<DotEnvironment>, ThresholdSignatureFor<T, PolkadotInstance>),
		>;
	}

	pub mod arb {
		use super::*;

		#[frame_support::storage_alias]
		pub type ThresholdSignatureData<T: Config<ArbitrumInstance>, ArbitrumInstance> = StorageMap<
			ArbitrumBroadcaster,
			Twox64Concat,
			BroadcastId,
			// Note: Using the old arbitrum api
			(ArbitrumApi<EvmEnvironment>, ThresholdSignatureFor<T, ArbitrumInstance>),
		>;
	}
}

fn evm_tx_builder_fn<C>(
	evm_tx_builder: old::EvmTransactionBuilder<C>,
	current_evm_key: cf_chains::evm::AggKey,
) -> EvmTransactionBuilder<C> {
	EvmTransactionBuilder {
		signer_and_sig_data: evm_tx_builder.sig_data.map(|sig_data| (current_evm_key, sig_data)),
		replay_protection: evm_tx_builder.replay_protection,
		call: evm_tx_builder.call,
	}
}

pub struct EthMigrateApicallsAndOnChainKey;

impl OnRuntimeUpgrade for EthMigrateApicallsAndOnChainKey {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		let current_evm_key = pallet_cf_threshold_signature::Keys::<Runtime, Instance16>::get(
			pallet_cf_threshold_signature::CurrentKeyEpoch::<Runtime, Instance16>::get()
				.unwrap_or_default(),
		)
		.unwrap_or_default();

		pallet_cf_broadcast::CurrentOnChainKey::<Runtime, Instance1>::put(current_evm_key);

		next::ThresholdSignatureData::<Runtime, Instance1>::translate_values(
			|(old_apicall, sig): (
				old::EthereumApi<EvmEnvironment>,
				ThresholdSignatureFor<Runtime, Instance1>,
			)| {
				Some((
					match old_apicall {
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
						old::EthereumApi::AllBatch(evm_tx_builder) => EthereumApi::AllBatch(
							evm_tx_builder_fn(evm_tx_builder, current_evm_key),
						),
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
					},
					sig,
				))
			},
		);

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		pre_upgrade!(Instance1, Instance16, eth)
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		use cf_chains::evm::AggKey;
		fn assertion(
			old_apicall: old::EthereumApi<EvmEnvironment>,
			new_apicall: EthereumApi<EvmEnvironment>,
		) -> bool {
			new_apicall.chain_encoded() == old_apicall.chain_encoded()
		}
		post_upgrade!(Instance1, EthereumApi<EvmEnvironment>, AggKey, state, assertion)
	}
}

pub struct DotMigrateApicallsAndOnChainKey;

impl OnRuntimeUpgrade for DotMigrateApicallsAndOnChainKey {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		let current_dot_key = pallet_cf_threshold_signature::Keys::<Runtime, Instance2>::get(
			pallet_cf_threshold_signature::CurrentKeyEpoch::<Runtime, Instance2>::get()
				.unwrap_or_default(),
		)
		.unwrap_or_default();

		pallet_cf_broadcast::CurrentOnChainKey::<Runtime, Instance2>::put(current_dot_key);

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

		next::ThresholdSignatureData::<Runtime, Instance2>::translate_values(
			|(old_apicall, sig): (
				old::PolkadotApi<DotEnvironment>,
				ThresholdSignatureFor<Runtime, Instance2>,
			)| {
				Some((
					match old_apicall {
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
						old::PolkadotApi::ChangeGovKey(dot_ext_builder) =>
							PolkadotApi::ChangeGovKey(dot_tx_builder_fn(
								dot_ext_builder,
								current_dot_key,
							)),
						old::PolkadotApi::ExecuteXSwapAndCall(dot_ext_builder) =>
							PolkadotApi::ExecuteXSwapAndCall(dot_tx_builder_fn(
								dot_ext_builder,
								current_dot_key,
							)),
						old::PolkadotApi::_Phantom(..) => unreachable!(),
					},
					sig,
				))
			},
		);

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		pre_upgrade!(Instance2, Instance2, dot)
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		use cf_chains::dot::PolkadotPublicKey;
		fn assertion(
			old_apicall: old::PolkadotApi<DotEnvironment>,
			new_apicall: PolkadotApi<DotEnvironment>,
		) -> bool {
			let new_ext_builder =
				cf_chains::map_over_api_variants!(new_apicall, ext_builder, ext_builder);
			let old_ext_builder = old_apicall.unwrap();

			new_ext_builder.extrinsic_call == old_ext_builder.extrinsic_call &&
				new_ext_builder.replay_protection == old_ext_builder.replay_protection &&
				new_ext_builder.signer_and_signature.expect("signer and sig").1 ==
					old_ext_builder.signature.expect("signature")
		}
		post_upgrade!(Instance2, PolkadotApi<DotEnvironment>, PolkadotPublicKey, state, assertion)
	}
}

pub struct BtcMigrateApicallsAndOnChainKey;

impl OnRuntimeUpgrade for BtcMigrateApicallsAndOnChainKey {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		let current_btc_key = pallet_cf_threshold_signature::Keys::<Runtime, Instance3>::get(
			pallet_cf_threshold_signature::CurrentKeyEpoch::<Runtime, Instance3>::get()
				.unwrap_or_default(),
		)
		.unwrap_or_default();

		pallet_cf_broadcast::CurrentOnChainKey::<Runtime, Instance3>::put(current_btc_key);

		next::ThresholdSignatureData::<Runtime, Instance3>::translate_values(
			|(old_apicall, sig): (
				old::BitcoinApi<BtcEnvironment>,
				ThresholdSignatureFor<Runtime, Instance3>,
			)| {
				Some((
					match old_apicall {
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
									// The signature here is a valid signature since this storage
									// item only stores signed calls
									signer_and_signatures: Some((
										current_btc_key,
										old_batch_transfer.bitcoin_transaction.signatures,
									)),
								},
								change_utxo_key: old_batch_transfer.change_utxo_key,
							}),

						old::BitcoinApi::_Phantom(..) => unreachable!(),
					},
					sig,
				))
			},
		);

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		pre_upgrade!(Instance3, Instance3, btc)
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		use cf_chains::btc::AggKey;
		fn assertion(
			old_apicall: old::BitcoinApi<BtcEnvironment>,
			new_apicall: BitcoinApi<BtcEnvironment>,
		) -> bool {
			match new_apicall {
				BitcoinApi::BatchTransfer(new_batch_transfer) => {
					let old_batch_transfer = old_apicall.unwrap();
					new_batch_transfer.change_utxo_key == old_batch_transfer.change_utxo_key &&
						new_batch_transfer.bitcoin_transaction.inputs ==
							old_batch_transfer.bitcoin_transaction.inputs &&
						new_batch_transfer.bitcoin_transaction.outputs ==
							old_batch_transfer.bitcoin_transaction.outputs &&
						new_batch_transfer.bitcoin_transaction.transaction_bytes ==
							old_batch_transfer.bitcoin_transaction.transaction_bytes &&
						new_batch_transfer.bitcoin_transaction.old_utxo_input_indices ==
							old_batch_transfer.bitcoin_transaction.old_utxo_input_indices &&
						new_batch_transfer
							.bitcoin_transaction
							.signer_and_signatures
							.expect("post check bitcoin")
							.1 == old_batch_transfer.bitcoin_transaction.signatures
				},

				BitcoinApi::_Phantom(..) => unreachable!(),
			}
		}
		post_upgrade!(Instance3, BitcoinApi<BtcEnvironment>, AggKey, state, assertion)
	}
}

pub struct ArbMigrateApicallsAndOnChainKey;

impl OnRuntimeUpgrade for ArbMigrateApicallsAndOnChainKey {
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		let current_evm_key = pallet_cf_threshold_signature::Keys::<Runtime, Instance16>::get(
			pallet_cf_threshold_signature::CurrentKeyEpoch::<Runtime, Instance16>::get()
				.unwrap_or_default(),
		)
		.unwrap_or_default();

		pallet_cf_broadcast::CurrentOnChainKey::<Runtime, Instance4>::put(current_evm_key);

		next::ThresholdSignatureData::<Runtime, Instance4>::translate_values(
			|(old_apicall, sig): (
				old::ArbitrumApi<BtcEnvironment>,
				ThresholdSignatureFor<Runtime, Instance4>,
			)| {
				Some((
					match old_apicall {
						old::ArbitrumApi::SetAggKeyWithAggKey(evm_tx_builder) =>
							ArbitrumApi::SetAggKeyWithAggKey(evm_tx_builder_fn(
								evm_tx_builder,
								current_evm_key,
							)),

						old::ArbitrumApi::AllBatch(evm_tx_builder) => ArbitrumApi::AllBatch(
							evm_tx_builder_fn(evm_tx_builder, current_evm_key),
						),
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
					},
					sig,
				))
			},
		);

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		pre_upgrade!(Instance4, Instance16, arb)
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		use cf_chains::evm::AggKey;
		fn assertion(
			old_apicall: old::ArbitrumApi<EvmEnvironment>,
			new_apicall: ArbitrumApi<EvmEnvironment>,
		) -> bool {
			new_apicall.chain_encoded() == old_apicall.chain_encoded()
		}
		post_upgrade!(Instance4, ArbitrumApi<EvmEnvironment>, AggKey, state, assertion)
	}
}

#[macro_export]
macro_rules! arb_map_over_api_variants_old {
	( $self:expr, $var:pat_param, $var_attribute:expr $(,)* ) => {
		match $self {
			old::ArbitrumApi::SetAggKeyWithAggKey($var) => $var_attribute,
			old::ArbitrumApi::AllBatch($var) => $var_attribute,
			old::ArbitrumApi::ExecutexSwapAndCall($var) => $var_attribute,
			old::ArbitrumApi::TransferFallback($var) => $var_attribute,
			old::ArbitrumApi::_Phantom(..) => unreachable!(),
		}
	};
}

#[macro_export]
macro_rules! eth_map_over_api_variants_old {
	( $self:expr, $var:pat_param, $var_attribute:expr $(,)* ) => {
		match $self {
			old::EthereumApi::SetAggKeyWithAggKey($var) => $var_attribute,
			old::EthereumApi::RegisterRedemption($var) => $var_attribute,
			old::EthereumApi::UpdateFlipSupply($var) => $var_attribute,
			old::EthereumApi::SetGovKeyWithAggKey($var) => $var_attribute,
			old::EthereumApi::SetCommKeyWithAggKey($var) => $var_attribute,
			old::EthereumApi::AllBatch($var) => $var_attribute,
			old::EthereumApi::ExecutexSwapAndCall($var) => $var_attribute,
			old::EthereumApi::TransferFallback($var) => $var_attribute,
			old::EthereumApi::_Phantom(..) => unreachable!(),
		}
	};
}

#[macro_export]
macro_rules! pre_upgrade {
	(  $chain_pallet_instance:ident, $crypto_pallet_instance:ident, $path_to_old:ident  ) => {{

		let old_api_calls = old::$path_to_old::ThresholdSignatureData::<Runtime, $chain_pallet_instance>::iter()
			.map(|(id, (api_call, _sig))| (id, api_call))
			.collect::<Vec<_>>();
		let current_key = pallet_cf_threshold_signature::Keys::<Runtime, $crypto_pallet_instance>::get(pallet_cf_threshold_signature::CurrentKeyEpoch::<Runtime, $crypto_pallet_instance>::get().expect("current key epoch")).expect("threshold keys");

		Ok((old_api_calls, current_key).encode())
	}};
}

#[macro_export]
macro_rules! post_upgrade {
	(  $chain_pallet_instance:ident, $chain_api:ident <$env: ident>, $aggkey:ident, $state:expr, $assertion:ident ) => {{
		use pallet_cf_broadcast::CurrentOnChainKey;
		let (old_threshold_sig_datas, current_key) =
			<(Vec<(BroadcastId, old::$chain_api<$env>)>, $aggkey)>::decode(&mut &$state[..])
				.map_err(|_| DispatchError::Other("Failed to decode old PendingApicalls"))?;

		assert_eq!(
			CurrentOnChainKey::<Runtime, $chain_pallet_instance>::get().expect("on chain key"),
			current_key
		);

		old_threshold_sig_datas.into_iter().for_each(|(broadcast_id, old_apicall)| {
			let (new_apicall, _sig) =
				next::ThresholdSignatureData::<Runtime, $chain_pallet_instance>::get(broadcast_id)
					.expect("next threshold sig data");

			assert!(new_apicall.signer().expect("signer new apicall") == current_key);
			assert!($assertion(old_apicall, new_apicall));
		});

		Ok(())
	}};
}

pub struct NoSolUpgrade;

impl OnRuntimeUpgrade for NoSolUpgrade {
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
