pub mod batch_transfer;

use super::{
	ingress_address::derive_btc_ingress_address, scriptpubkey_from_address, AggKey, Bitcoin,
	BitcoinNetwork, BitcoinOutput, BtcAmount, Utxo, CHANGE_ADDRESS_SALT,
};
use crate::*;
use frame_support::{CloneNoBound, DebugNoBound, EqNoBound, Never, PartialEqNoBound};
use sp_std::marker::PhantomData;

#[derive(CloneNoBound, DebugNoBound, PartialEqNoBound, EqNoBound, Encode, Decode, TypeInfo)]
#[scale_info(skip_type_params(Environment))]
pub enum BitcoinApi<Environment: 'static> {
	BatchTransfer(batch_transfer::BatchTransfer),
	#[doc(hidden)]
	#[codec(skip)]
	_Phantom(PhantomData<Environment>, Never),
}

pub type SelectedUtxos = (Vec<Utxo>, u64);

impl<E> AllBatch<Bitcoin> for BitcoinApi<E>
where
	E: ChainEnvironment<<Bitcoin as Chain>::ChainAmount, SelectedUtxos>
		+ ChainEnvironment<(), BitcoinNetwork>
		+ ChainEnvironment<(), AggKey>,
{
	fn new_unsigned(
		_fetch_params: Vec<FetchAssetParams<Bitcoin>>,
		transfer_params: Vec<TransferAssetParams<Bitcoin>>,
	) -> Result<Self, ()> {
		let bitcoin_network = <E as ChainEnvironment<(), BitcoinNetwork>>::lookup(())
			.expect("Since the lookup function always returns a some");
		let bitcoin_return_address = derive_btc_ingress_address(
			<E as ChainEnvironment<(), AggKey>>::lookup(()).ok_or(())?.0,
			CHANGE_ADDRESS_SALT,
			bitcoin_network,
		);
		let mut total_output_amount: u64 = 0;
		let mut btc_outputs = vec![];
		for transfer_param in transfer_params {
			btc_outputs.push(BitcoinOutput {
				amount: transfer_param.clone().amount.try_into().expect("Since this output comes from the AMM and if AMM math works correctly, this should be a valid bitcoin amount which should be less than u64::max"),
				script_pubkey: transfer_param.to.to_scriptpubkey().map_err(|_| ())?,
			});
			total_output_amount += <u128 as TryInto<u64>>::try_into(transfer_param.amount)
				.expect("BTC amounts are never more than u64 max");
		}
		// Looks up all available Utxos and selects and takes them for the transaction depending on
		// the amount that needs to be output.
		let (selected_input_utxos, total_input_amount_available) =
			<E as ChainEnvironment<BtcAmount, SelectedUtxos>>::lookup(total_output_amount.into())
				.ok_or(())?;

		btc_outputs.push(BitcoinOutput {
			amount: total_input_amount_available.checked_sub(total_output_amount).expect("This should never overflow because the total input available was calculated from the total output amount and the algorithm ensures that the total input amount is greater than the total output amount"),
			script_pubkey: scriptpubkey_from_address(
				&bitcoin_return_address[..],
				bitcoin_network,
			)
			.map_err(|_| ())?,
		});
		Ok(Self::BatchTransfer(batch_transfer::BatchTransfer::new_unsigned(
			selected_input_utxos,
			btc_outputs,
		)))
	}
}

impl<E> SetAggKeyWithAggKey<Bitcoin> for BitcoinApi<E>
where
	E: ChainEnvironment<<Bitcoin as Chain>::ChainAmount, SelectedUtxos>
		+ ChainEnvironment<(), BitcoinNetwork>,
{
	fn new_unsigned(
		_maybe_old_key: Option<<Bitcoin as ChainCrypto>::AggKey>,
		new_key: <Bitcoin as ChainCrypto>::AggKey,
	) -> Result<Self, ()> {
		let bitcoin_network = <E as ChainEnvironment<(), BitcoinNetwork>>::lookup(())
			.expect("Since the lookup function always returns a some");

		// We will use the bitcoin address derived with the salt of 0 as the vault address where we
		// collect unspent amounts in btc transactions and consolidate funds when rotating epoch.
		let new_vault_return_address =
			derive_btc_ingress_address(new_key.0, CHANGE_ADDRESS_SALT, bitcoin_network);

		//max possible btc value to get all available utxos
		let (all_input_utxos, total_spendable_amount_in_vault) =
			<E as ChainEnvironment<BtcAmount, SelectedUtxos>>::lookup(u64::MAX.into()).ok_or(())?;

		Ok(Self::BatchTransfer(batch_transfer::BatchTransfer::new_unsigned(
			all_input_utxos,
			vec![BitcoinOutput {
				amount: total_spendable_amount_in_vault,
				script_pubkey: scriptpubkey_from_address(
					&new_vault_return_address,
					bitcoin_network,
				).expect("This shouldn't error because we generate this address ourselves above, and should be valid"),
			}],
		)))
	}
}

impl<E> From<batch_transfer::BatchTransfer> for BitcoinApi<E> {
	fn from(tx: batch_transfer::BatchTransfer) -> Self {
		Self::BatchTransfer(tx)
	}
}

impl<E> ApiCall<Bitcoin> for BitcoinApi<E> {
	fn threshold_signature_payload(&self) -> <Bitcoin as ChainCrypto>::Payload {
		match self {
			BitcoinApi::BatchTransfer(tx) => tx.threshold_signature_payload(),

			BitcoinApi::_Phantom(..) => unreachable!(),
		}
	}

	fn signed(self, threshold_signature: &<Bitcoin as ChainCrypto>::ThresholdSignature) -> Self {
		match self {
			BitcoinApi::BatchTransfer(call) => call.signed(threshold_signature).into(),

			BitcoinApi::_Phantom(..) => unreachable!(),
		}
	}

	fn chain_encoded(&self) -> Vec<u8> {
		match self {
			BitcoinApi::BatchTransfer(call) => call.chain_encoded(),

			BitcoinApi::_Phantom(..) => unreachable!(),
		}
	}

	fn is_signed(&self) -> bool {
		match self {
			BitcoinApi::BatchTransfer(call) => call.is_signed(),

			BitcoinApi::_Phantom(..) => unreachable!(),
		}
	}
}
