use bitcoin::{hashes::Hash as btcHash, opcodes::all::OP_RETURN, ScriptBuf};
use cf_amm::math::{bounded_sqrt_price, sqrt_price_to_price};
use cf_chains::{
	assets::btc::Asset as BtcAsset,
	btc::{
		deposit_address::DepositAddress, vault_swap_encoding::UtxoEncodedData, ScriptPubkey, Utxo,
		UtxoId,
	},
	ChannelRefundParameters,
};
use cf_primitives::{AccountId, Beneficiary, ChannelId, DcaParameters};
use cf_utilities::SliceToArray;
use codec::Decode;
use itertools::Itertools;
use sp_core::H256;
use state_chain_runtime::BitcoinInstance;

use crate::btc::rpc::VerboseTransaction;

const OP_PUSHBYTES_75: u8 = 0x4b;
const OP_PUSHDATA1: u8 = 0x4c;
const NATIVE_ASSET: BtcAsset = BtcAsset::Btc;

fn try_extract_utxo_encoded_data(script: &bitcoin::ScriptBuf) -> Option<&[u8]> {
	let bytes = script.as_script().as_bytes();

	if bytes.len() < 2 {
		return None;
	}

	// First opcode must be OP_RETURN
	if bytes[0] != OP_RETURN.to_u8() {
		return None;
	}

	// Second opcode must be either OP_PUSHBYTES_X (1..=75) or OP_PUSHDATA1 (76):
	let (data_len, data_bytes) = match bytes[1] {
		// Opcode encodes the length directly
		data_len @ 1..=OP_PUSHBYTES_75 => (data_len, &bytes[2..]),
		// The length is encoded in the following byte:
		OP_PUSHDATA1 => (bytes[2], &bytes[3..]),
		_ => {
			return None;
		},
	};

	// Sanity check:
	if data_bytes.len() != data_len as usize {
		return None;
	}

	Some(data_bytes)
}

fn script_buf_to_script_pubkey(script: &ScriptBuf) -> Option<ScriptPubkey> {
	fn data_from_script<const LEN: usize>(script: &ScriptBuf, bytes_to_skip: usize) -> [u8; LEN] {
		script.bytes().skip(bytes_to_skip).take(LEN).collect_vec().copy_to_array()
	}

	let pubkey = if script.is_p2pkh() {
		ScriptPubkey::P2PKH(data_from_script(script, 3))
	} else if script.is_p2sh() {
		ScriptPubkey::P2SH(data_from_script(script, 2))
	} else if script.is_p2tr() {
		ScriptPubkey::Taproot(data_from_script(script, 2))
	} else if script.is_p2wsh() {
		ScriptPubkey::P2WSH(data_from_script(script, 2))
	} else if script.is_p2wpkh() {
		ScriptPubkey::P2WPKH(data_from_script(script, 2))
	} else {
		ScriptPubkey::OtherSegwit {
			version: script.witness_version()?.to_num(),
			program: script.bytes().skip(2).collect_vec().try_into().ok()?,
		}
	};

	Some(pubkey)
}

pub(super) type BtcIngressEgressCall =
	pallet_cf_ingress_egress::Call<state_chain_runtime::Runtime, BitcoinInstance>;

type VaultDepositWitness =
	pallet_cf_ingress_egress::VaultDepositWitness<state_chain_runtime::Runtime, BitcoinInstance>;

pub fn try_extract_vault_swap_witness(
	tx: &VerboseTransaction,
	vault_address: &DepositAddress,
	channel_id: ChannelId,
	broker_id: &AccountId,
) -> Option<VaultDepositWitness> {
	// A correctly constructed transaction carrying CF swap parameters must have at least 3 outputs:
	let [utxo_to_vault, nulldata_utxo, change_utxo, ..] = &tx.vout[..] else {
		return None;
	};

	// First output must be a deposit into our vault:
	if utxo_to_vault.script_pubkey.as_bytes() != vault_address.script_pubkey().bytes() {
		return None;
	}

	// Second output must be a nulldata UTXO (with 0 amount):
	if nulldata_utxo.value.to_sat() != 0 {
		tracing::warn!(
			"Observed a tx into our vault's change address, but the value of the second UTXO is non-zero (tx_hash: {})",
			tx.hash
		);
		return None;
	}

	let mut data = try_extract_utxo_encoded_data(&nulldata_utxo.script_pubkey)?;

	let Ok(data) = UtxoEncodedData::decode(&mut data) else {
		tracing::warn!(
			"Failed to decode UTXO encoded data targeting our vault (tx_hash: {})",
			tx.hash
		);
		return None;
	};

	// Third output must be a "change utxo" whose address we assume to also be the refund address:
	let Some(refund_address) = script_buf_to_script_pubkey(&change_utxo.script_pubkey) else {
		tracing::error!("Failed to extract refund address (tx_hash: {})", tx.hash);
		return None;
	};

	let deposit_amount = utxo_to_vault.value.to_sat();

	// Derive min price (encoded as min output amount to save space):
	let min_price = sqrt_price_to_price(bounded_sqrt_price(
		data.parameters.min_output_amount.into(),
		deposit_amount.into(),
	));

	let tx_id: [u8; 32] = tx.txid.to_byte_array();

	Some(VaultDepositWitness {
		input_asset: NATIVE_ASSET,
		output_asset: data.output_asset,
		deposit_amount,
		destination_address: data.output_address,
		tx_id: H256::from(tx_id),
		deposit_details: Utxo {
			// we require the deposit to be the first UTXO
			id: UtxoId { tx_id: tx_id.into(), vout: 0 },
			amount: deposit_amount,
			deposit_address: vault_address.clone(),
		},
		deposit_metadata: None, // No ccm for BTC (yet?)
		broker_fee: Some(Beneficiary {
			account: broker_id.clone(),
			bps: data.parameters.broker_fee.into(),
		}),
		affiliate_fees: data
			.parameters
			.affiliates
			.into_iter()
			.map(Into::into)
			.collect_vec()
			.try_into()
			.expect("runtime supports at least as many affiliates as we allow in UTXO encoding"),
		refund_params: ChannelRefundParameters {
			retry_duration: data.parameters.retry_duration.into(),
			refund_address,
			min_price,
		},
		dca_params: Some(DcaParameters {
			number_of_chunks: data.parameters.number_of_chunks.into(),
			chunk_interval: data.parameters.chunk_interval.into(),
		}),
		// This is only to be checked in the pre-witnessed version
		boost_fee: data.parameters.boost_fee.into(),
		channel_id: Some(channel_id),
		deposit_address: Some(vault_address.script_pubkey()),
	})
}

#[cfg(test)]
mod tests {
	use std::sync::LazyLock;

	use bitcoin::{
		blockdata::script::{witness_program::WitnessProgram, witness_version::WitnessVersion},
		hashes::Hash,
		key::TweakedPublicKey,
		PubkeyHash, ScriptHash, WPubkeyHash, WScriptHash,
	};
	use cf_chains::{
		address::EncodedAddress,
		btc::{BitcoinOp, BitcoinScript},
	};
	use secp256k1::XOnlyPublicKey;
	use sp_core::bounded_vec;

	use crate::{btc::rpc::VerboseTxOut, witness::btc::deposits::tests::fake_transaction};

	use super::*;

	use cf_chains::btc::vault_swap_encoding::*;

	const MOCK_DOT_ADDRESS: [u8; 32] = [9u8; 32];

	static MOCK_SWAP_PARAMS: LazyLock<UtxoEncodedData> = LazyLock::new(|| UtxoEncodedData {
		output_asset: cf_primitives::Asset::Dot,
		output_address: EncodedAddress::Dot(MOCK_DOT_ADDRESS),
		parameters: BtcCfParameters {
			retry_duration: 5,
			min_output_amount: u128::MAX,
			number_of_chunks: 0x0ffff,
			chunk_interval: 2,
			boost_fee: 5,
			broker_fee: 10,
			affiliates: bounded_vec![cf_primitives::AffiliateAndFee {
				affiliate: 17.into(),
				fee: 7
			}],
		},
	});

	fn add_opcodes_to_data(data: Vec<u8>) -> ScriptBuf {
		ScriptBuf::from_bytes(
			BitcoinScript::new(&[
				BitcoinOp::Return,
				BitcoinOp::PushBytes { bytes: data.try_into().unwrap() },
			])
			.raw(),
		)
	}

	#[test]
	fn script_buf_to_script_pubkey_conversion() {
		// Check that we can convert from all types of bitcoin addresses:
		for (script_buf, script_pubkey) in [
			(
				ScriptBuf::new_p2pkh(&PubkeyHash::from_byte_array([7; 20])),
				ScriptPubkey::P2PKH([7; 20]),
			),
			(
				ScriptBuf::new_p2sh(&ScriptHash::from_byte_array([7; 20])),
				ScriptPubkey::P2SH([7; 20]),
			),
			(
				ScriptBuf::new_p2tr_tweaked(TweakedPublicKey::dangerous_assume_tweaked(
					XOnlyPublicKey::from_slice(&[7; 32]).unwrap(),
				)),
				ScriptPubkey::Taproot([7; 32]),
			),
			(
				ScriptBuf::new_p2wsh(&WScriptHash::from_byte_array([7; 32])),
				ScriptPubkey::P2WSH([7; 32]),
			),
			(
				ScriptBuf::new_p2wpkh(&WPubkeyHash::from_byte_array([7; 20])),
				ScriptPubkey::P2WPKH([7; 20]),
			),
			(
				ScriptBuf::new_witness_program(
					&WitnessProgram::new(WitnessVersion::V2, &[7; 40]).unwrap(),
				),
				ScriptPubkey::OtherSegwit { version: 2, program: bounded_vec![7; 40] },
			),
		] {
			assert_eq!(script_buf_to_script_pubkey(&script_buf), Some(script_pubkey));
		}
	}

	#[test]
	fn test_extract_contract_call_from_tx() {
		use bitcoin::Amount;

		const REFUND_PK_HASH: [u8; 20] = [8; 20];
		const DEPOSIT_AMOUNT: u64 = 1000;
		const BROKER: AccountId = AccountId::new([1; 32]);

		// Addresses have different representations to satisfy interfaces:
		let vault_deposit_address = DepositAddress::new([7; 32], 0);
		let vault_script = ScriptBuf::from_bytes(vault_deposit_address.script_pubkey().bytes());

		let refund_pubkey = ScriptPubkey::P2PKH(REFUND_PK_HASH);
		let refund_script = ScriptBuf::new_p2pkh(&PubkeyHash::from_byte_array(REFUND_PK_HASH));
		assert_eq!(refund_pubkey.bytes(), refund_script.to_bytes());
		let tx = fake_transaction(
			vec![
				// A UTXO spending into our vault;
				VerboseTxOut {
					value: Amount::from_sat(DEPOSIT_AMOUNT),
					n: 0,
					script_pubkey: vault_script.clone(),
				},
				// A nulldata UTXO encoding some swap parameters:
				VerboseTxOut {
					value: Amount::from_sat(0),
					n: 1,
					script_pubkey: add_opcodes_to_data(encode_swap_params_in_nulldata_payload(
						MOCK_SWAP_PARAMS.clone(),
					)),
				},
				// A UTXO containing refund address:
				VerboseTxOut {
					value: Amount::from_sat(0),
					n: 2,
					script_pubkey: refund_script.clone(),
				},
			],
			None,
		);

		const CHANNEL_ID: ChannelId = 7;

		assert_eq!(
			try_extract_vault_swap_witness(&tx, &vault_deposit_address, CHANNEL_ID, &BROKER),
			Some(VaultDepositWitness {
				input_asset: NATIVE_ASSET,
				output_asset: MOCK_SWAP_PARAMS.output_asset,
				deposit_amount: DEPOSIT_AMOUNT,
				destination_address: MOCK_SWAP_PARAMS.output_address.clone(),
				tx_id: tx.txid.to_byte_array().into(),
				deposit_details: Utxo {
					id: UtxoId { tx_id: tx.txid.to_byte_array().into(), vout: 0 },
					amount: DEPOSIT_AMOUNT,
					deposit_address: vault_deposit_address.clone(),
				},
				broker_fee: Some(Beneficiary {
					account: BROKER,
					bps: MOCK_SWAP_PARAMS.parameters.broker_fee.into()
				}),
				affiliate_fees: bounded_vec![MOCK_SWAP_PARAMS.parameters.affiliates[0].into()],
				deposit_metadata: None,
				refund_params: ChannelRefundParameters {
					retry_duration: MOCK_SWAP_PARAMS.parameters.retry_duration.into(),
					refund_address: refund_pubkey,
					min_price: sqrt_price_to_price(bounded_sqrt_price(
						MOCK_SWAP_PARAMS.parameters.min_output_amount.into(),
						DEPOSIT_AMOUNT.into(),
					)),
				},
				dca_params: Some(DcaParameters {
					number_of_chunks: MOCK_SWAP_PARAMS.parameters.number_of_chunks.into(),
					chunk_interval: MOCK_SWAP_PARAMS.parameters.chunk_interval.into(),
				}),
				boost_fee: MOCK_SWAP_PARAMS.parameters.boost_fee.into(),
				deposit_address: Some(vault_deposit_address.script_pubkey()),
				channel_id: Some(CHANNEL_ID),
			})
		);
	}

	#[test]
	fn extract_nulldata_utxo() {
		for data in [vec![0x3u8; 1_usize], vec![0x3u8; 75_usize], vec![0x3u8; 80_usize]] {
			let script_buf = add_opcodes_to_data(data.clone());
			assert_eq!(try_extract_utxo_encoded_data(&script_buf), Some(&data[..]));
		}

		// Some degenerate cases:
		for data in [
			vec![],                                   // too few bytes
			vec![OP_RETURN.to_u8()],                  // too few bytes
			vec![OP_RETURN.to_u8(), OP_PUSHBYTES_75], // no bytes follow "pushbytes"
		] {
			let script_buf = ScriptBuf::from_bytes(data);

			assert_eq!(try_extract_utxo_encoded_data(&script_buf), None);
		}
	}
}
