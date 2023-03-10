pub mod batch_fetch_and_transfer;

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
	BatchFetchAndTransfer(batch_fetch_and_transfer::BatchFetchAndTransfer),
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
		for transfer_param in transfer_params {
			btc_outputs.push(BitcoinOutput {
				amount: transfer_param.clone().amount.try_into().expect("Since this output comes from the AMM and if AMM math works correctly, this should be a valid bitcoin amount which should be less than u64::max"),
				script_pubkey: scriptpubkey_from_address(
					sp_std::str::from_utf8(&transfer_param.to[..]).map_err(|_| ())?,
					bitcoin_network.clone(),
				).map_err(|_|())?,
			});
			total_output_amount += <u128 as TryInto<u64>>::try_into(transfer_param.amount)
				.expect("BTC amounts are never more than u64 max");
		}
		let (selected_input_utxos, total_input_amount_available) =
			<E as ChainEnvironment<BtcAmount, (Vec<Utxo>, u64)>>::lookup(
				total_output_amount.into(),
			)
			.ok_or(())?;

		btc_outputs.push(BitcoinOutput {
			amount: total_input_amount_available - total_output_amount,
			script_pubkey: scriptpubkey_from_address(
				sp_std::str::from_utf8(&bitcoin_return_address[..]).map_err(|_| ())?,
				bitcoin_network,
			)
			.map_err(|_| ())?,
		});
		Ok(Self::BatchFetchAndTransfer(
			batch_fetch_and_transfer::BatchFetchAndTransfer::new_unsigned(
				selected_input_utxos,
				btc_outputs,
			),
		))
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

		Ok(Self::BatchFetchAndTransfer(
			batch_fetch_and_transfer::BatchFetchAndTransfer::new_unsigned(
				all_input_utxos,
				vec![BitcoinOutput {
					amount: total_spendable_amount_in_vault,
					script_pubkey: scriptpubkey_from_address(
						&new_vault_return_address,
						bitcoin_network,
					)
					.map_err(|_| ())?,
				}],
			),
		))
	}
}

impl<E> From<batch_fetch_and_transfer::BatchFetchAndTransfer> for BitcoinApi<E> {
	fn from(tx: batch_fetch_and_transfer::BatchFetchAndTransfer) -> Self {
		Self::BatchFetchAndTransfer(tx)
	}
}

impl<E> ApiCall<Bitcoin> for BitcoinApi<E> {
	fn threshold_signature_payload(&self) -> <Bitcoin as ChainCrypto>::Payload {
		match self {
			BitcoinApi::BatchFetchAndTransfer(tx) => tx.threshold_signature_payload(),

			BitcoinApi::_Phantom(..) => unreachable!(),
		}
	}

	fn signed(self, threshold_signature: &<Bitcoin as ChainCrypto>::ThresholdSignature) -> Self {
		match self {
			BitcoinApi::BatchFetchAndTransfer(call) => call.signed(threshold_signature).into(),

			BitcoinApi::_Phantom(..) => unreachable!(),
		}
	}

	fn chain_encoded(&self) -> Vec<u8> {
		match self {
			BitcoinApi::BatchFetchAndTransfer(call) => call.chain_encoded(),

			BitcoinApi::_Phantom(..) => unreachable!(),
		}
	}

	fn is_signed(&self) -> bool {
		match self {
			BitcoinApi::BatchFetchAndTransfer(call) => call.is_signed(),

			BitcoinApi::_Phantom(..) => unreachable!(),
		}
	}
}
