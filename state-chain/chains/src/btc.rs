pub mod ingress_address;

use bech32::{self, u5, FromBase32, ToBase32, Variant};
use frame_support::sp_io::hashing::sha2_256;
use libsecp256k1::{PublicKey, SecretKey};
use sp_std::{vec, vec::Vec};
extern crate alloc;
use alloc::string::String;

use self::ingress_address::tweaked_pubkey;

/// SHA256("TapLeaf")
const TAPLEAF_HASH: &[u8] =
	&hex_literal::hex!("aeea8fdc4208983105734b58081d1e2638d35f1cb54008d4d357ca03be78e9ee");
/// SHA256("TapTweak")
const TAPTWEAK_HASH: &[u8] =
	&hex_literal::hex!("e80fe1639c9ca050e3af1b39c143c63e429cbceb15d940fbb5c5a1f4af57c5e9");
/// SHA256("TapSighash")
const TAPSIGHASH_HASH: &[u8] =
	&hex_literal::hex!("f40a48df4b2a70c8b4924bf2654661ed3d95fd66a313eb87237597c628e4a031");
const INTERNAL_PUBKEY: &[u8] =
	&hex_literal::hex!("02eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee");

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
	let mut script = BitcoinScript::default();
	script.push_uint(salt).op_drop().push_bytes(pubkey_x.to_vec()).op_checksig();
	sha2_256(&[TAPLEAF_HASH, TAPLEAF_HASH, &[0xC0_u8], &script.serialize()].concat())
}

pub fn scriptpubkey_from_address(address: String) -> Vec<u8> {
	match bech32::decode(&address) {
		Ok((_hrp, data, _variant)) => {
			let mut script = BitcoinScript::default();
			let version = data[0].to_u8();
			script.push_uint(version as u32);
			script.push_bytes(Vec::<u8>::from_base32(&data[1..]).unwrap());
			script.serialize()
		},
		_ => panic!("todo: figure out how to handle invalid egress addresses here..."),
	}
}

impl BitcoinTransaction {
	fn get_signing_payload(&self, index: u32) -> [u8; 32] {
		let prevouts = sha2_256(
			self.inputs
				.iter()
				.fold(&mut Vec::<u8>::default(), |acc, x| {
					let mut le_txid = x.txid.to_vec();
					le_txid.reverse();
					acc.append(&mut le_txid);
					acc.append(&mut x.vout.to_le_bytes().to_vec());
					acc
				})
				.as_slice(),
		);
		let amounts = sha2_256(
			self.inputs
				.iter()
				.fold(&mut Vec::<u8>::default(), |acc, x| {
					acc.append(&mut x.amount.to_le_bytes().to_vec());
					acc
				})
				.as_slice(),
		);
		let scriptpubkeys = sha2_256(
			self.inputs
				.iter()
				.fold(&mut Vec::<u8>::default(), |acc, x| {
					let mut script = BitcoinScript::default();
					script.push_uint(1);
					script.push_bytes(tweaked_pubkey(x.pubkey_x, x.salt).to_vec());
					acc.append(&mut script.serialize());
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
				.fold(&mut Vec::<u8>::default(), |acc, x| {
					acc.append(&mut x.amount.to_le_bytes().to_vec());
					acc.append(&mut scriptpubkey_from_address(x.destination.clone()));
					acc
				})
				.as_slice(),
		);
		let epoch = 0u8;
		let hashtype = 0u8;
		let version = 2u32.to_le_bytes();
		let locktime = 0u32.to_le_bytes();
		let spendtype = 2u8;
		let keyversion = 0u8;
		let codeseparator = u32::MAX.to_le_bytes();
		sha2_256(
			&[
				TAPSIGHASH_HASH,
				TAPSIGHASH_HASH,
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
		)
	}
}

#[derive(Default)]
struct BitcoinScript(Vec<u8>);

impl BitcoinScript {
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
		result.append(&mut value.to_le_bytes()[..len].into());
		result
	}

	/// Adds an operation to the script that pushes an unsigned integer onto the stack
	fn push_uint(&mut self, value: u32) -> &mut Self {
		match value {
			0 => self.0.push(0),
			1..=16 => self.0.push(0x50 + value as u8),
			_ => {
				let num_bytes = (4 - value.leading_zeros() / 8) as usize;
				self.0.push(num_bytes as u8);
				self.0.append(&mut value.to_le_bytes()[..num_bytes].into());
			},
		}
		self
	}
	/// Adds an operation to the script that pushes exactly the provided bytes of data to the stack
	fn push_bytes(&mut self, mut data: Vec<u8>) -> &mut Self {
		self.0.append(&mut Self::to_varint(data.len() as u64));
		self.0.append(&mut data);
		self
	}
	/// Adds an operation to the script that drops the topmost item from the stack
	fn op_drop(&mut self) -> &mut Self {
		self.0.push(0x75);
		self
	}
	/// Adds the CHECKSIG operation to the script
	fn op_checksig(&mut self) -> &mut Self {
		self.0.push(0xAC);
		self
	}
	/// Serializes the script by returning a single byte for the length
	/// of the script and then the script itself
	fn serialize(&self) -> Vec<u8> {
		let mut result = Vec::default();
		result.append(&mut Self::to_varint(self.0.len() as u64));
		result.append(&mut self.0.clone());
		result
	}
}

#[test]
fn test_scriptpubkey_from_address() {
	assert_eq!(
		scriptpubkey_from_address(
			"bc1p0xlxvlhemja6c4dqv22uapctqupfhlxm9h8z3k2e72q4k9hcz7vqzk5jj0".to_string()
		),
		hex_literal::hex!("22512079be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798")
	);
	assert_eq!(
		scriptpubkey_from_address("BC1QW508D6QEJXTDG4Y5R3ZARVARY0C5XW7KV8F3T4".to_string()),
		hex_literal::hex!("160014751e76e8199196d454941c45d1b3a323f1433bd6")
	);
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
		tx.get_signing_payload(0),
		hex_literal::hex!("E16117C6CD69142E41736CE2882F0E697FF4369A2CBCEE9D92FC0346C6774FB4")
	);
}

#[test]
fn test_build_script() {
	let mut script = BitcoinScript::default();
	script
		.push_uint(0)
		.op_drop()
		.push_bytes(
			hex::decode("2E897376020217C8E385A30B74B758293863049FA66A3FD177E012B076059105")
				.unwrap(),
		)
		.op_checksig();
	assert_eq!(
		script.serialize(),
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
		let mut script = BitcoinScript::default();
		script.push_uint(x.0);
		assert_eq!(script.0, x.1);
	}
}
