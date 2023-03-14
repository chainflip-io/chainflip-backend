pub mod batch_transfer;

use super::{
	ingress_address::derive_btc_ingress_address, scriptpubkey_from_address, Bitcoin,
	BitcoinNetwork, BitcoinOutput, BtcAddress, BtcAmount, Utxo,
};
use crate::*;
use frame_support::{CloneNoBound, DebugNoBound, EqNoBound, Never, PartialEqNoBound};
use sp_std::marker::PhantomData;

#[derive(CloneNoBound, DebugNoBound, PartialEqNoBound, EqNoBound, Encode, Decode, TypeInfo)]
#[scale_info(skip_type_params(Environment))]
pub enum BitcoinApi<Environment: 'static> {
	BatchTransfer(batch_transfer::BatchTransfer),
	//RotateVaultProxy(rotate_vault_proxy::RotateVaultProxy),
	//CreateAnonymousVault(create_anonymous_vault::CreateAnonymousVault),
	//ChangeGovKey(set_gov_key_with_agg_key::ChangeGovKey),
	#[doc(hidden)]
	#[codec(skip)]
	_Phantom(PhantomData<Environment>, Never),
}

impl<E> AllBatch<Bitcoin> for BitcoinApi<E>
where
	E: ChainEnvironment<<Bitcoin as Chain>::ChainAmount, (Vec<Utxo>, u64)>
		+ ChainEnvironment<(), (BitcoinNetwork, BtcAddress)>,
{
	fn new_unsigned(
		_fetch_params: Vec<FetchAssetParams<Bitcoin>>,
		transfer_params: Vec<TransferAssetParams<Bitcoin>>,
	) -> Result<Self, ()> {
		let (bitcoin_network, bitcoin_return_address) =
			<E as ChainEnvironment<(), (BitcoinNetwork, BtcAddress)>>::lookup(())
				.expect("Since the lookup function always returns a some");
		let mut total_output_amount: u64 = 0;
		let mut btc_outputs = vec![];

		let bitcoin_output =
			|amount, address: <Bitcoin as Chain>::ChainAccount| -> Result<BitcoinOutput, ()> {
				Ok(BitcoinOutput {
					amount,
					script_pubkey: scriptpubkey_from_address(
						sp_std::str::from_utf8(&address[..]).map_err(|_| ())?,
						bitcoin_network.clone(),
					)
					.map_err(|_| ())?,
				})
			};

		for transfer_param in transfer_params {
			let amount: u64 = transfer_param.clone().amount.try_into().expect("Since this output comes from the AMM and if AMM math works correctly, this should be a valid bitcoin amount which should be less than u64::max");
			btc_outputs.push(bitcoin_output(amount, transfer_param.to)?);
			total_output_amount += amount;
		}
		let (selected_input_utxos, total_input_amount_available) =
			<E as ChainEnvironment<BtcAmount, (Vec<Utxo>, u64)>>::lookup(
				total_output_amount.into(),
			)
			.ok_or(())?;

		btc_outputs.push(bitcoin_output(
			total_input_amount_available - total_output_amount,
			bitcoin_return_address,
		)?);
		Ok(Self::BatchTransfer(batch_transfer::BatchTransfer::new_unsigned(
			selected_input_utxos,
			btc_outputs,
		)))
	}
}

impl<E> SetAggKeyWithAggKey<Bitcoin> for BitcoinApi<E>
where
	E: ChainEnvironment<<Bitcoin as Chain>::ChainAmount, (Vec<Utxo>, u64)>
		+ ChainEnvironment<(), (BitcoinNetwork, BtcAddress)>,
{
	fn new_unsigned(
		_maybe_old_key: Option<<Bitcoin as ChainCrypto>::AggKey>,
		new_key: <Bitcoin as ChainCrypto>::AggKey,
	) -> Result<Self, ()> {
		let (bitcoin_network, _bitcoin_return_address) =
			<E as ChainEnvironment<(), (BitcoinNetwork, BtcAddress)>>::lookup(())
				.expect("Since the lookup function always returns a some");

		// We will use the bitcoin address derived with the salt of 0 as the vault address where we
		// collect unspent amounts in btc transactions and consolidate funds when rotating epoch.
		let new_vault_return_address =
			derive_btc_ingress_address(new_key.0, 0, bitcoin_network.clone());

		let (all_input_utxos, total_spendable_amount_in_vault) =
			<E as ChainEnvironment<BtcAmount, (Vec<Utxo>, u64)>>::lookup(u64::MAX.into()) //max possible btc value to get all available utxos
				.ok_or(())?;

		Ok(Self::BatchTransfer(batch_transfer::BatchTransfer::new_unsigned(
			all_input_utxos,
			vec![BitcoinOutput {
				amount: total_spendable_amount_in_vault,
				script_pubkey: scriptpubkey_from_address(
					&new_vault_return_address,
					bitcoin_network,
				)
				.map_err(|_| ())?,
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
