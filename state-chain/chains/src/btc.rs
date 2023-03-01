pub mod ingress_address;

use base58::FromBase58;
use bech32::{self, u5, FromBase32, ToBase32, Variant};
use codec::{Decode, Encode};
use frame_support::{sp_io::hashing::sha2_256, RuntimeDebug};
use libsecp256k1::{PublicKey, SecretKey};
use scale_info::TypeInfo;
use sp_std::{vec, vec::Vec};
extern crate alloc;
use alloc::string::String;
use itertools;

use self::ingress_address::tweaked_pubkey;

const INTERNAL_PUBKEY: &[u8] =
	&hex_literal::hex!("02eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee");

#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq)]
pub enum BitcoinTransactionError {
	/// The transaction's chain id is invalid.
	InvalidChainId,
	/// The egress address is invalid
	InvalidEgressAddress,
}

pub struct Utxo {
	amount: u64,
	txid: [u8; 32],
	vout: u32,
	pubkey_x: [u8; 32],
	salt: u32,
}

pub struct BitcoinOutput {
	amount: u64,
	destination: String,
}

pub struct BitcoinTransaction {
	inputs: Vec<Utxo>,
	outputs: Vec<BitcoinOutput>,
	signatures: Vec<[u8; 64]>,
}

fn get_tapleaf_hash(pubkey_x: [u8; 32], salt: u32) -> [u8; 32] {
	// SHA256("TapLeaf")
	let tapleaf_hash: &[u8] =
		&hex_literal::hex!("aeea8fdc4208983105734b58081d1e2638d35f1cb54008d4d357ca03be78e9ee");
	let leaf_version = 0xC0_u8;
	let script = BitcoinScript::default()
		.push_uint(salt)
		.op_drop()
		.push_bytes(&pubkey_x.to_vec())
		.op_checksig()
		.serialize();
	sha2_256(&[tapleaf_hash, tapleaf_hash, &[leaf_version], &script].concat())
}

fn to_varint(value: u64) -> Vec<u8> {
	let mut result = Vec::default();
	let len = match value {
		0..=0xFC => 1,
		0xFD..=0xFFFF => {
			result.push(0xFD_u8);
			2
		},
		0x010000..=0xFFFFFFFF => {
			result.push(0xFE_u8);
			4
		},
		_ => {
			result.push(0xFF_u8);
			8
		},
	};
	result.extend(value.to_le_bytes().iter().take(len));
	result
}

pub fn scriptpubkey_from_address(address: String) -> Result<Vec<u8>, BitcoinTransactionError> {
	let content = address.from_base58();
	if let Ok(data) = content {
		let version = data[0];
		let checksum = data.rchunks(4).next().unwrap();
		if &sha2_256(&sha2_256(&data[..data.len() - 4]))[..4] == checksum {
			if version == 0 {
				// P2PKH
				return Ok(BitcoinScript::default()
					.op_dup()
					.op_hash160()
					.push_bytes(&data[1..data.len() - 4].to_vec())
					.op_equalverify()
					.op_checksig()
					.0)
			} else if version == 5 {
				// P2SH
				return Ok(BitcoinScript::default()
					.op_hash160()
					.push_bytes(&data[1..data.len() - 4].to_vec())
					.op_equal()
					.0)
			}
		} else {
			return Err(BitcoinTransactionError::InvalidEgressAddress)
		}
	}
	let content = bech32::decode(&address);
	if let Ok((_hrp, data, _variant)) = content {
		let version = data[0].to_u8();
		return Ok(BitcoinScript::default()
			.push_uint(version as u32)
			.push_bytes(&Vec::<u8>::from_base32(&data[1..]).unwrap())
			.0)
	}
	Err(BitcoinTransactionError::InvalidEgressAddress)
}

impl BitcoinTransaction {
	pub fn add_signature(mut self, index: u32, signature: [u8; 64]) -> Self {
		if self.signatures.len() != self.inputs.len() {
			self.signatures.resize(self.inputs.len(), [0u8; 64]);
		}
		self.signatures[index as usize] = signature;
		self
	}
	pub fn finalize(self) -> Result<Vec<u8>, BitcoinTransactionError> {
		let mut result = Vec::default();
		let version = 2u32.to_le_bytes();
		let segwit_marker = 0u8;
		let segwit_flag = 1u8;
		let locktime = [0u8, 0, 0, 0];
		result.extend(version);
		result.push(segwit_marker);
		result.push(segwit_flag);
		result.extend(to_varint(self.inputs.len() as u64));
		result.extend(self.inputs.iter().fold(Vec::<u8>::default(), |mut acc, x| {
			let mut le_txid = x.txid.to_vec();
			le_txid.reverse();
			acc.extend(le_txid);
			acc.extend(x.vout.to_le_bytes());
			acc.push(0);
			acc.extend((u32::MAX - 2).to_le_bytes().iter());
			acc
		}));
		result.extend(to_varint(self.outputs.len() as u64));
		result.extend(self.outputs.iter().try_fold(Vec::<u8>::default(), |mut acc, x| {
			acc.extend(x.amount.to_le_bytes());
			let script = scriptpubkey_from_address(x.destination.clone())?;
			acc.extend(to_varint(script.len() as u64));
			acc.extend(script);
			Ok(acc)
		})?);
		for i in 0..self.inputs.len() {
			let num_witnesses = 3u8;
			let len_signature = 64u8;
			result.push(num_witnesses);
			result.push(len_signature);
			result.extend(self.signatures[i]);
			let script = BitcoinScript::default()
				.push_uint(self.inputs[i].salt)
				.op_drop()
				.push_bytes(&self.inputs[i].pubkey_x.to_vec())
				.op_checksig()
				.serialize();
			result.extend(script);
			result.push(0x21u8);
			let tweaked = tweaked_pubkey(self.inputs[i].pubkey_x, self.inputs[i].salt);
			// push correct leaf version depending on evenness of public key
			if tweaked.serialize_compressed()[0] == 2 {
				result.push(0xC0_u8);
			} else {
				result.push(0xC1_u8);
			}
			result.extend(INTERNAL_PUBKEY[1..33].iter());
		}
		result.extend(locktime);
		Ok(result)
	}

	pub fn get_signing_payload(self, index: u32) -> Result<[u8; 32], BitcoinTransactionError> {
		// SHA256("TapSighash")
		let tapsig_hash: &[u8] =
			&hex_literal::hex!("f40a48df4b2a70c8b4924bf2654661ed3d95fd66a313eb87237597c628e4a031");
		let prevouts = sha2_256(
			self.inputs
				.iter()
				.fold(Vec::<u8>::default(), |mut acc, x| {
					let mut le_txid = x.txid.to_vec();
					le_txid.reverse();
					acc.extend(le_txid);
					acc.extend(x.vout.to_le_bytes().iter());
					acc
				})
				.as_slice(),
		);
		let amounts = sha2_256(
			self.inputs
				.iter()
				.fold(Vec::<u8>::default(), |mut acc, x| {
					acc.extend(x.amount.to_le_bytes().iter());
					acc
				})
				.as_slice(),
		);
		let scriptpubkeys = sha2_256(
			self.inputs
				.iter()
				.fold(Vec::<u8>::default(), |mut acc, x| {
					let script = BitcoinScript::default()
						.push_uint(1)
						.push_bytes(
							&tweaked_pubkey(x.pubkey_x, x.salt).serialize_compressed()[1..33]
								.to_vec(),
						)
						.serialize();
					acc.extend(script);
					acc
				})
				.as_slice(),
		);
		let sequences = sha2_256(
			&core::iter::repeat((u32::MAX - 2).to_le_bytes())
				.take(self.inputs.len())
				.collect::<Vec<_>>()
				.concat(),
		);
		let outputs = sha2_256(
			self.outputs
				.iter()
				.try_fold(Vec::<u8>::default(), |mut acc, x| {
					acc.extend(x.amount.to_le_bytes().iter());
					let script = scriptpubkey_from_address(x.destination.clone())?;
					acc.extend(to_varint(script.len() as u64));
					acc.extend(script);
					Ok(acc)
				})?
				.as_slice(),
		);
		let epoch = 0u8;
		let hashtype = 0u8;
		let version = 2u32.to_le_bytes();
		let locktime = 0u32.to_le_bytes();
		let spendtype = 2u8;
		let keyversion = 0u8;
		let codeseparator = u32::MAX.to_le_bytes();
		Ok(sha2_256(
			&[
				tapsig_hash,
				tapsig_hash,
				&[epoch, hashtype],
				&version,
				&locktime,
				&prevouts,
				&amounts,
				&scriptpubkeys,
				&sequences,
				&outputs,
				&[spendtype],
				&index.to_le_bytes(),
				&get_tapleaf_hash(
					self.inputs[index as usize].pubkey_x,
					self.inputs[index as usize].salt,
				),
				&[keyversion],
				&codeseparator,
			]
			.concat(),
		))
	}
}

#[derive(Default)]
struct BitcoinScript(Vec<u8>);

impl BitcoinScript {
	/// Adds an operation to the script that pushes an unsigned integer onto the stack
	fn push_uint(mut self, value: u32) -> Self {
		match value {
			0 => self.0.push(0),
			1..=16 => self.0.push(0x50 + value as u8),
			_ => {
				let num_bytes =
					sp_std::mem::size_of::<u32>() - (value.leading_zeros() / 8) as usize;
				self = self.push_bytes(&value.to_le_bytes().into_iter().take(num_bytes).collect());
			},
		}
		self
	}
	/// Adds an operation to the script that pushes exactly the provided bytes of data to the stack
	fn push_bytes(mut self, data: &Vec<u8>) -> Self {
		self.0.extend(to_varint(data.len() as u64));
		self.0.extend(data);
		self
	}
	/// Adds an operation to the script that drops the topmost item from the stack
	fn op_drop(mut self) -> Self {
		self.0.push(0x75);
		self
	}
	/// Adds the CHECKSIG operation to the script
	fn op_checksig(mut self) -> Self {
		self.0.push(0xAC);
		self
	}
	/// Adds the DUP operation to the script
	fn op_dup(mut self) -> Self {
		self.0.push(0x76);
		self
	}
	/// Adds the HASH160 operation to the script
	fn op_hash160(mut self) -> Self {
		self.0.push(0xA9);
		self
	}
	/// Adds the EQUALVERIFY operation to the script
	fn op_equalverify(mut self) -> Self {
		self.0.push(0x88);
		self
	}
	/// Adds the EQUAL operation to the script
	fn op_equal(mut self) -> Self {
		self.0.push(0x87);
		self
	}
	/// Serializes the script by returning a single byte for the length
	/// of the script and then the script itself
	fn serialize(self) -> Vec<u8> {
		itertools::chain!(to_varint(self.0.len() as u64), self.0).collect()
	}
}

#[test]
fn test_scriptpubkey_from_address() {
	assert_eq!(
		scriptpubkey_from_address(
			"bc1p0xlxvlhemja6c4dqv22uapctqupfhlxm9h8z3k2e72q4k9hcz7vqzk5jj0".to_string()
		)
		.unwrap(),
		hex_literal::hex!("512079be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798")
	);
	assert_eq!(
		scriptpubkey_from_address(
			"bc1pw508d6qejxtdg4y5r3zarvary0c5xw7kw508d6qejxtdg4y5r3zarvary0c5xw7k7grplx"
				.to_string()
		)
		.unwrap(),
		hex_literal::hex!(
			"5128751e76e8199196d454941c45d1b3a323f1433bd6751e76e8199196d454941c45d1b3a323f1433bd6"
		)
	);
	assert_eq!(
		scriptpubkey_from_address(
			"bc1pw508d6qejxtdg4y5r3zarvary0c5xw7kw508d6qejxtdg4y5r3zarvary0c5xw7kt5nd6y"
				.to_string()
		)
		.unwrap(),
		hex_literal::hex!(
			"5128751e76e8199196d454941c45d1b3a323f1433bd6751e76e8199196d454941c45d1b3a323f1433bd6"
		)
	);
	assert_eq!(
		scriptpubkey_from_address("BC1SW50QA3JX3S".to_string()).unwrap(),
		hex_literal::hex!("6002751e")
	);
	assert_eq!(
		scriptpubkey_from_address("bc1zw508d6qejxtdg4y5r3zarvaryvg6kdaj".to_string()).unwrap(),
		hex_literal::hex!("5210751e76e8199196d454941c45d1b3a323")
	);
	assert_eq!(
		scriptpubkey_from_address("BC1QW508D6QEJXTDG4Y5R3ZARVARY0C5XW7KV8F3T4".to_string())
			.unwrap(),
		hex_literal::hex!("0014751e76e8199196d454941c45d1b3a323f1433bd6")
	);
	assert_eq!(
		scriptpubkey_from_address("132F25rTsvBdp9JzLLBHP5mvGY66i1xdiM".to_string()).unwrap(),
		hex_literal::hex!("76a914162c5ea71c0b23f5b9022ef047c4a86470a5b07088ac")
	);
	assert_eq!(
		scriptpubkey_from_address("3QJmV3qfvL9SuYo34YihAf3sRCW3qSinyC".to_string()).unwrap(),
		hex_literal::hex!("a914f815b036d9bbbce5e9f2a00abd1bf3dc91e9551087")
	);
}

#[test]
fn test_finalize() {
	let input = Utxo {
		amount: 100010000,
		vout: 1,
		txid: hex_literal::hex!("b5bb9d8014a0f9b1d61e21e796d78dccdf1352f23cd32812f4850b878ae4944c"),
		pubkey_x: hex_literal::hex!(
			"78C79A2B436DA5575A03CDE40197775C656FFF9F0F59FC1466E09C20A81A9CDB"
		),
		salt: 123,
	};
	let output = BitcoinOutput {
		amount: 100000000,
		destination: "bc1pgtj0f3u2rk8ex6khlskz7q50nwc48r8unfgfhxzsx9zhcdnczhqq60lzjt".to_string(),
	};
	let tx = BitcoinTransaction {
		inputs: vec![input],
		outputs: vec![output],
		signatures: Default::default(),
	}
	.add_signature(0, [0u8; 64]);
	assert_eq!(tx.finalize().unwrap(), hex_literal::hex!("020000000001014C94E48A870B85F41228D33CF25213DFCC8DD796E7211ED6B1F9A014809DBBB50100000000FDFFFFFF0100E1F5050000000022512042E4F4C78A1D8F936AD7FC2C2F028F9BB1538CFC9A509B985031457C367815C003400000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000025017B752078C79A2B436DA5575A03CDE40197775C656FFF9F0F59FC1466E09C20A81A9CDBAC21C0EEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEE00000000"));
}

#[test]
fn test_payload() {
	let input = Utxo {
		amount: 100010000,
		vout: 1,
		txid: hex_literal::hex!("b5bb9d8014a0f9b1d61e21e796d78dccdf1352f23cd32812f4850b878ae4944c"),
		pubkey_x: hex_literal::hex!(
			"78C79A2B436DA5575A03CDE40197775C656FFF9F0F59FC1466E09C20A81A9CDB"
		),
		salt: 123,
	};
	let output = BitcoinOutput {
		amount: 100000000,
		destination: "bc1pgtj0f3u2rk8ex6khlskz7q50nwc48r8unfgfhxzsx9zhcdnczhqq60lzjt".to_string(),
	};
	let tx = BitcoinTransaction {
		inputs: vec![input],
		outputs: vec![output],
		signatures: Default::default(),
	};
	assert_eq!(
		tx.get_signing_payload(0).unwrap(),
		hex_literal::hex!("E16117C6CD69142E41736CE2882F0E697FF4369A2CBCEE9D92FC0346C6774FB4")
	);
}

#[test]
fn test_build_script() {
	assert_eq!(
		BitcoinScript::default()
			.push_uint(0)
			.op_drop()
			.push_bytes(
				&hex::decode("2E897376020217C8E385A30B74B758293863049FA66A3FD177E012B076059105")
					.unwrap(),
			)
			.op_checksig()
			.serialize(),
		hex_literal::hex!(
			"240075202E897376020217C8E385A30B74B758293863049FA66A3FD177E012B076059105AC"
		)
	);
}

#[test]
fn test_push_uint() {
	let test_data = [
		(0, vec![0]),
		(1, vec![81]),
		(2, vec![82]),
		(16, vec![96]),
		(17, vec![1, 17]),
		(255, vec![1, 255]),
		(256, vec![2, 0, 1]),
		(11394560, vec![3, 0, 0xDE, 0xAD]),
		(u32::MAX, vec![4, 255, 255, 255, 255]),
	];
	for x in test_data {
		assert_eq!(BitcoinScript::default().push_uint(x.0).0, x.1);
	}
}
