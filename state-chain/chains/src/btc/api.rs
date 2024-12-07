pub mod batch_transfer;

use super::{
	deposit_address::DepositAddress, AggKey, Bitcoin, BitcoinCrypto, BitcoinOutput, BtcAmount,
	Utxo, BITCOIN_DUST_LIMIT, CHANGE_ADDRESS_SALT,
};
use crate::{btc::BitcoinTransaction, *};
use frame_support::{CloneNoBound, DebugNoBound, EqNoBound, Never, PartialEqNoBound};
use sp_std::marker::PhantomData;

#[derive(CloneNoBound, DebugNoBound, PartialEqNoBound, EqNoBound, Encode, Decode, TypeInfo)]
#[scale_info(skip_type_params(Environment))]
#[allow(clippy::large_enum_variant)]
pub enum BitcoinApi<Environment: 'static> {
	BatchTransfer(batch_transfer::BatchTransfer),
	NoChangeTransfer(BitcoinTransaction),
	#[doc(hidden)]
	#[codec(skip)]
	_Phantom(PhantomData<Environment>, Never),
}
pub type SelectedUtxosAndChangeAmount = (Vec<Utxo>, BtcAmount);

#[derive(Copy, Clone, RuntimeDebug, PartialEq, Eq, Encode, Decode, MaxEncodedLen, TypeInfo)]
pub enum UtxoSelectionType {
	SelectForConsolidation,
	Some { output_amount: BtcAmount, number_of_outputs: u64 },
}

impl<E> ConsolidateCall<Bitcoin> for BitcoinApi<E>
where
	E: ChainEnvironment<UtxoSelectionType, SelectedUtxosAndChangeAmount>
		+ ChainEnvironment<(), AggKey>,
{
	fn consolidate_utxos() -> Result<Self, ConsolidationError> {
		let agg_key @ AggKey { current, .. } =
			<E as ChainEnvironment<(), AggKey>>::lookup(()).ok_or(ConsolidationError::Other)?;
		let bitcoin_change_script =
			DepositAddress::new(current, CHANGE_ADDRESS_SALT).script_pubkey();

		let (selected_input_utxos, change_amount) =
			E::lookup(UtxoSelectionType::SelectForConsolidation)
				.ok_or(ConsolidationError::NotRequired)?;

		log::info!("Consolidating {} btc utxos", selected_input_utxos.len());

		let btc_outputs =
			vec![BitcoinOutput { amount: change_amount, script_pubkey: bitcoin_change_script }];

		Ok(Self::BatchTransfer(batch_transfer::BatchTransfer::new_unsigned(
			&agg_key,
			agg_key.current,
			selected_input_utxos,
			btc_outputs,
		)))
	}
}

// NB: A Bitcoin transaction containing a UTXO below the dust limit will fail to be included by a
// block. Therefore, we do not include UTXOs below the dust limit in the transaction.
impl<E> AllBatch<Bitcoin> for BitcoinApi<E>
where
	E: ChainEnvironment<UtxoSelectionType, SelectedUtxosAndChangeAmount>
		+ ChainEnvironment<(), AggKey>,
{
	fn new_unsigned(
		_fetch_params: Vec<FetchAssetParams<Bitcoin>>,
		transfer_params: Vec<(TransferAssetParams<Bitcoin>, EgressId)>,
	) -> Result<Vec<(Self, Vec<EgressId>)>, AllBatchError> {
		let (transfer_params, egress_ids): (Vec<TransferAssetParams<Bitcoin>>, Vec<EgressId>) =
			transfer_params.into_iter().unzip();

		let agg_key @ AggKey { current, .. } =
			<E as ChainEnvironment<(), AggKey>>::lookup(()).ok_or(AllBatchError::AggKeyNotSet)?;
		let bitcoin_change_script =
			DepositAddress::new(current, CHANGE_ADDRESS_SALT).script_pubkey();
		let mut total_output_amount: u64 = 0;
		let mut btc_outputs = vec![];
		for transfer_param in transfer_params {
			if transfer_param.amount >= BITCOIN_DUST_LIMIT {
				btc_outputs.push(BitcoinOutput {
					amount: transfer_param.amount,
					script_pubkey: transfer_param.to,
				});
				total_output_amount += transfer_param.amount;
			}
		}
		// Looks up all available Utxos and selects and takes them for the transaction depending on
		// the amount that needs to be output. If the output amount is 0,
		let (selected_input_utxos, change_amount) = E::lookup(UtxoSelectionType::Some {
			output_amount: (total_output_amount > 0)
				.then_some(total_output_amount)
				.ok_or(AllBatchError::NotRequired)?,
			number_of_outputs: (btc_outputs.len() + 1) as u64, // +1 for the change output
		})
		.ok_or(AllBatchError::UtxoSelectionFailed)?;
		if change_amount >= BITCOIN_DUST_LIMIT {
			btc_outputs.push(BitcoinOutput {
				amount: change_amount,
				script_pubkey: bitcoin_change_script,
			});
		}

		Ok(vec![(
			Self::BatchTransfer(batch_transfer::BatchTransfer::new_unsigned(
				&agg_key,
				agg_key.current,
				selected_input_utxos,
				btc_outputs,
			)),
			egress_ids,
		)])
	}
}

impl<E> SetAggKeyWithAggKey<BitcoinCrypto> for BitcoinApi<E>
where
	E: ChainEnvironment<UtxoSelectionType, SelectedUtxosAndChangeAmount>,
{
	fn new_unsigned(
		_maybe_old_key: Option<<BitcoinCrypto as ChainCrypto>::AggKey>,
		_new_key: <BitcoinCrypto as ChainCrypto>::AggKey,
	) -> Result<Option<Self>, SetAggKeyWithAggKeyError> {
		// Utxo transfer into the new vault now happens gradually over the new epoch as part of
		// consolidation. This prevents sending too many utxos within the same transaction
		// which may cause threshold signing to fail.
		Ok(None)
	}
}

impl<E> From<batch_transfer::BatchTransfer> for BitcoinApi<E> {
	fn from(tx: batch_transfer::BatchTransfer) -> Self {
		Self::BatchTransfer(tx)
	}
}

// TODO: Implement transfer / transfer and call for Bitcoin.
impl<E: ReplayProtectionProvider<Bitcoin>> ExecutexSwapAndCall<Bitcoin> for BitcoinApi<E> {
	fn new_unsigned(
		_transfer_param: TransferAssetParams<Bitcoin>,
		_source_chain: ForeignChain,
		_source_address: Option<ForeignChainAddress>,
		_gas_budget: GasAmount,
		_message: Vec<u8>,
		_ccm_additional_data: Vec<u8>,
	) -> Result<Self, ExecutexSwapAndCallError> {
		Err(ExecutexSwapAndCallError::Unsupported)
	}
}

impl<E: ReplayProtectionProvider<Bitcoin>> RejectCall<Bitcoin> for BitcoinApi<E>
where
	E: ChainEnvironment<UtxoSelectionType, SelectedUtxosAndChangeAmount>
		+ ChainEnvironment<(), AggKey>,
{
	fn new_unsigned(
		deposit_details: <Bitcoin as Chain>::DepositDetails,
		refund_address: <Bitcoin as Chain>::ChainAccount,
		refund_amount: <Bitcoin as Chain>::ChainAmount,
	) -> Result<Self, RejectError> {
		let agg_key = <E as ChainEnvironment<(), AggKey>>::lookup(()).ok_or(RejectError::Other)?;
		Ok(Self::NoChangeTransfer(BitcoinTransaction::create_new_unsigned(
			&agg_key,
			vec![deposit_details],
			vec![BitcoinOutput { amount: refund_amount, script_pubkey: refund_address }],
		)))
	}
}

// transfer_fallback is unsupported for Bitcoin.
impl<E: ReplayProtectionProvider<Bitcoin>> TransferFallback<Bitcoin> for BitcoinApi<E> {
	fn new_unsigned(
		_transfer_param: TransferAssetParams<Bitcoin>,
	) -> Result<Self, TransferFallbackError> {
		Err(TransferFallbackError::Unsupported)
	}
}

impl<E> ApiCall<BitcoinCrypto> for BitcoinApi<E> {
	fn threshold_signature_payload(&self) -> <BitcoinCrypto as ChainCrypto>::Payload {
		match self {
			BitcoinApi::BatchTransfer(tx) => tx.threshold_signature_payload(),
			BitcoinApi::NoChangeTransfer(tx) => tx.get_signing_payloads(),
			BitcoinApi::_Phantom(..) => unreachable!(),
		}
	}

	fn signed(
		self,
		threshold_signature: &<BitcoinCrypto as ChainCrypto>::ThresholdSignature,
		signer: <BitcoinCrypto as ChainCrypto>::AggKey,
	) -> Self {
		match self {
			BitcoinApi::BatchTransfer(call) => call.signed(threshold_signature, signer).into(),
			BitcoinApi::NoChangeTransfer(mut tx) => {
				tx.add_signer_and_signatures(signer, threshold_signature.clone());
				Self::NoChangeTransfer(tx)
			},
			BitcoinApi::_Phantom(..) => unreachable!(),
		}
	}

	fn chain_encoded(&self) -> Vec<u8> {
		match self {
			BitcoinApi::BatchTransfer(call) => call.chain_encoded(),
			BitcoinApi::NoChangeTransfer(call) => call.clone().finalize(),
			BitcoinApi::_Phantom(..) => unreachable!(),
		}
	}

	fn is_signed(&self) -> bool {
		match self {
			BitcoinApi::BatchTransfer(call) => call.is_signed(),
			BitcoinApi::NoChangeTransfer(call) => call.is_signed(),
			BitcoinApi::_Phantom(..) => unreachable!(),
		}
	}

	fn transaction_out_id(&self) -> <BitcoinCrypto as ChainCrypto>::TransactionOutId {
		match self {
			BitcoinApi::BatchTransfer(call) => call.transaction_out_id(),
			BitcoinApi::NoChangeTransfer(call) => call.txid(),
			BitcoinApi::_Phantom(..) => unreachable!(),
		}
	}

	fn refresh_replay_protection(&mut self) {
		// No replay protection refresh for Bitcoin.
	}

	fn signer(&self) -> Option<<BitcoinCrypto as ChainCrypto>::AggKey> {
		match self {
			BitcoinApi::BatchTransfer(call) => call.signer(),
			BitcoinApi::NoChangeTransfer(call) =>
				call.signer_and_signatures.as_ref().map(|(signer, _)| (*signer)),
			BitcoinApi::_Phantom(..) => unreachable!(),
		}
	}
}
