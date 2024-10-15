use bitcoin::{opcodes::all::OP_RETURN, ScriptBuf};
use cf_amm::common::{bounded_sqrt_price, sqrt_price_to_price};
use cf_chains::{
	btc::{
		deposit_address::DepositAddress, smart_contract_encoding::UtxoEncodedData,
		BtcDepositDetails, ScriptPubkey, UtxoId,
	},
	ChannelRefundParameters, ForeignChainAddress,
};
use cf_primitives::{Asset, DcaParameters};
use codec::Decode;
use itertools::Itertools;
use state_chain_runtime::BitcoinInstance;
use utilities::SliceToArray;

use crate::btc::rpc::VerboseTransaction;

const OP_PUSHBYTES_75: u8 = 0x4b;
const OP_PUSHDATA1: u8 = 0x4c;

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
		script.bytes().skip(bytes_to_skip).take(LEN).collect_vec().as_array()
	}

	let pubkey = if script.is_p2pkh() {
		ScriptPubkey::P2PKH(data_from_script(script, 3))
	} else if script.is_p2sh() {
		ScriptPubkey::P2SH(data_from_script(script, 2))
	} else if script.is_v1_p2tr() {
		ScriptPubkey::Taproot(data_from_script(script, 2))
	} else if script.is_v0_p2wsh() {
		ScriptPubkey::P2WSH(data_from_script(script, 2))
	} else if script.is_v0_p2wpkh() {
		ScriptPubkey::P2WPKH(data_from_script(script, 2))
	} else {
		ScriptPubkey::OtherSegwit {
			version: script.witness_version()?.to_num(),
			program: script.bytes().skip(2).collect_vec().try_into().ok()?,
		}
	};

	Some(pubkey)
}

type BtcIngressEgressCall =
	pallet_cf_ingress_egress::Call<state_chain_runtime::Runtime, BitcoinInstance>;

// Currently unused, but will be used by the deposit wintesser:
pub fn try_extract_contract_call(
	tx: &VerboseTransaction,
	vault_address: &DepositAddress,
) -> Option<BtcIngressEgressCall> {
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

	use secp256k1::hashes::Hash as secp256k1Hash;

	let tx_id: [u8; 32] = tx.txid.to_byte_array();

	Some(BtcIngressEgressCall::contract_swap_request {
		from: Asset::Btc,
		to: data.output_asset,
		deposit_amount,
		destination_address: data.output_address,
		tx_hash: tx_id,
		deposit_details: BtcDepositDetails {
			// we require the deposit to be the first UTXO
			utxo_id: UtxoId { tx_id: tx_id.into(), vout: 0 },
			deposit_address: vault_address.clone(),
		},
		refund_params: Some(ChannelRefundParameters {
			retry_duration: data.parameters.retry_duration as u32,
			refund_address: ForeignChainAddress::Btc(refund_address),
			min_price,
		}),
		dca_params: Some(DcaParameters {
			number_of_chunks: data.parameters.number_of_chunks as u32,
			chunk_interval: data.parameters.chunk_interval as u32,
		}),
		// This is only to be checked in the pre-witnessed version
		boost_fee: data.parameters.boost_fee as u16,
	})
}

#[cfg(test)]
mod tests {

	use bitcoin::{
		address::WitnessProgram, key::TweakedPublicKey, PubkeyHash, ScriptHash, WPubkeyHash,
		WScriptHash,
	};
	use cf_chains::address::EncodedAddress;
	use secp256k1::{hashes::Hash, XOnlyPublicKey};
	use sp_core::bounded_vec;

	use crate::{btc::rpc::VerboseTxOut, witness::btc::deposits::tests::fake_transaction};

	use super::*;

	use cf_chains::btc::smart_contract_encoding::*;

	const MOCK_DOT_ADDRESS: [u8; 32] = [9u8; 32];

	const MOCK_SWAP_PARAMS: UtxoEncodedData = UtxoEncodedData {
		output_asset: Asset::Dot,
		output_address: EncodedAddress::Dot(MOCK_DOT_ADDRESS),
		parameters: SharedCfParameters {
			retry_duration: 5,
			min_output_amount: u128::MAX,
			number_of_chunks: 0x0ffff,
			chunk_interval: 2,
			boost_fee: 5,
		},
	};

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
				ScriptBuf::new_v1_p2tr_tweaked(TweakedPublicKey::dangerous_assume_tweaked(
					XOnlyPublicKey::from_slice(&[7; 32]).unwrap(),
				)),
				ScriptPubkey::Taproot([7; 32]),
			),
			(
				ScriptBuf::new_v0_p2wsh(&WScriptHash::from_byte_array([7; 32])),
				ScriptPubkey::P2WSH([7; 32]),
			),
			(
				ScriptBuf::new_v0_p2wpkh(&WPubkeyHash::from_byte_array([7; 20])),
				ScriptPubkey::P2WPKH([7; 20]),
			),
			(
				ScriptBuf::new_witness_program(
					&WitnessProgram::new(bitcoin::address::WitnessVersion::V2, [7; 40]).unwrap(),
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
				// A nulddata UTXO encoding some swap parameters:
				VerboseTxOut {
					value: Amount::from_sat(0),
					n: 1,
					script_pubkey: ScriptBuf::from_bytes(
						encode_swap_params_in_nulldata_utxo(MOCK_SWAP_PARAMS.clone()).raw(),
					),
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

		assert_eq!(
			try_extract_contract_call(&tx, &vault_deposit_address),
			Some(BtcIngressEgressCall::contract_swap_request {
				from: Asset::Btc,
				to: MOCK_SWAP_PARAMS.output_asset,
				deposit_amount: DEPOSIT_AMOUNT,
				destination_address: MOCK_SWAP_PARAMS.output_address.clone(),
				tx_hash: tx.hash.to_byte_array(),
				deposit_details: BtcDepositDetails {
					utxo_id: UtxoId { tx_id: tx.txid.to_byte_array().into(), vout: 0 },
					deposit_address: vault_deposit_address,
				},
				refund_params: Some(ChannelRefundParameters {
					retry_duration: MOCK_SWAP_PARAMS.parameters.retry_duration as u32,
					refund_address: ForeignChainAddress::Btc(refund_pubkey),
					min_price: sqrt_price_to_price(bounded_sqrt_price(
						MOCK_SWAP_PARAMS.parameters.min_output_amount.into(),
						DEPOSIT_AMOUNT.into(),
					)),
				}),
				dca_params: Some(DcaParameters {
					number_of_chunks: MOCK_SWAP_PARAMS.parameters.number_of_chunks as u32,
					chunk_interval: MOCK_SWAP_PARAMS.parameters.chunk_interval as u32,
				}),
				boost_fee: MOCK_SWAP_PARAMS.parameters.boost_fee as u16,
			})
		);
	}

	#[test]
	fn extract_nulldata_utxo() {
		for data in [vec![0x3u8; 1_usize], vec![0x3u8; 75_usize], vec![0x3u8; 80_usize]] {
			let script_buf =
				ScriptBuf::from_bytes(encode_data_in_nulldata_utxo(&data).unwrap().raw());

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
