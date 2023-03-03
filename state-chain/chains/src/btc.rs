pub mod ingress_address;

use base58::FromBase58;
use bech32::{self, u5, FromBase32, ToBase32, Variant};
use codec::{Decode, Encode};
use core::{borrow::Borrow, iter};
use frame_support::{sp_io::hashing::sha2_256, RuntimeDebug};
use libsecp256k1::{PublicKey, SecretKey};
use scale_info::TypeInfo;
use sp_std::vec::Vec;
extern crate alloc;
use crate::Chain;
use alloc::string::String;
use cf_primitives::chains::assets;
pub use cf_primitives::chains::Bitcoin;
use itertools;

pub type BlockNumber = u64;

// TODO: Come back to this. in BTC u64 works, but the trait has from u128 required, so we do this
// for now
type Amount = u128;

impl Chain for Bitcoin {
	type ChainBlockNumber = BlockNumber;

	type ChainAmount = Amount;

	type TransactionFee = Self::ChainAmount;

	type TrackedData = ();

	type ChainAsset = assets::btc::Asset;

	// TODO: Provide an actual value for this
	type ChainAccount = u64;

	type EpochStartData = ();
}

use self::ingress_address::tweaked_pubkey;

const INTERNAL_PUBKEY: &[u8] =
	&hex_literal::hex!("02eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee");

const SEGWIT_VERSION: u8 = 1;

#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq)]
pub enum BitcoinTransactionError {
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
	script_pubkey: BitcoinScript,
}

pub struct BitcoinTransaction {
	inputs: Vec<Utxo>,
	outputs: Vec<BitcoinOutput>,
	signatures: Vec<[u8; 64]>,
}

fn get_tapleaf_hash(pubkey_x: [u8; 32], salt: u32) -> [u8; 32] {
	// SHA256("TapLeaf")
	const TAPLEAF_HASH: &[u8] =
		&hex_literal::hex!("aeea8fdc4208983105734b58081d1e2638d35f1cb54008d4d357ca03be78e9ee");
	let leaf_version = 0xC0_u8;
	let script = BitcoinScript::default()
		.push_uint(salt)
		.op_drop()
		.push_bytes(pubkey_x)
		.op_checksig()
		.serialize();
	sha2_256(&[TAPLEAF_HASH, TAPLEAF_HASH, &[leaf_version], &script].concat())
}

/// For reference see https://developer.bitcoin.org/reference/transactions.html#compactsize-unsigned-integers
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

#[derive(Clone, Copy, Debug)]
pub enum BitcoinNetwork {
	Mainnet,
	Testnet,
	Regtest,
}

impl BitcoinNetwork {
	fn p2pkh_address_version(&self) -> u8 {
		match self {
			BitcoinNetwork::Mainnet => 0,
			BitcoinNetwork::Testnet | BitcoinNetwork::Regtest => 111,
		}
	}

	fn p2sh_address_version(&self) -> u8 {
		match self {
			BitcoinNetwork::Mainnet => 5,
			BitcoinNetwork::Testnet | BitcoinNetwork::Regtest => 196,
		}
	}

	fn bech32_and_bech32m_address_hrp(&self) -> &'static str {
		match self {
			BitcoinNetwork::Mainnet => "bc",
			BitcoinNetwork::Testnet => "tb",
			BitcoinNetwork::Regtest => "bcrt",
		}
	}
}

pub fn scriptpubkey_from_address(
	address: &str,
	network: BitcoinNetwork,
) -> Result<BitcoinScript, BitcoinTransactionError> {
	// See https://en.bitcoin.it/wiki/Base58Check_encoding
	let try_decode_as_base58 = || {
		const CHECKSUM_LENGTH: usize = 4;

		let data: [u8; 1 + 20 + CHECKSUM_LENGTH] = address.from_base58().ok()?.try_into().ok()?;

		let (payload, checksum) = data.split_at(data.len() - CHECKSUM_LENGTH);

		if &sha2_256(&sha2_256(payload))[..CHECKSUM_LENGTH] == checksum {
			let (&version, hash) = payload.split_first().unwrap();
			if version == network.p2pkh_address_version() {
				Some(
					BitcoinScript::default()
						.op_dup()
						.op_hash160()
						.push_bytes(hash /* pubkey hash */)
						.op_equalverify()
						.op_checksig(),
				)
			} else if version == network.p2sh_address_version() {
				Some(
					BitcoinScript::default()
						.op_hash160()
						.push_bytes(hash /* script hash */)
						.op_equal(),
				)
			} else {
				None
			}
		} else {
			None
		}
	};

	// See https://en.bitcoin.it/wiki/BIP_0350
	let try_decode_as_bech32_or_bech32m = || {
		let (hrp, data, variant) = bech32::decode(address).ok()?;
		if hrp == network.bech32_and_bech32m_address_hrp() {
			let version = data.get(0)?.to_u8();
			let program = {
				let program = Vec::from_base32(&data[1..]).ok()?;
				match (version, variant) {
					(0, Variant::Bech32) if [20, 32].contains(&program.len()) => Some(program),
					(1..=16, Variant::Bech32m) if (2..=40).contains(&program.len()) =>
						Some(program),
					_ => None,
				}
			}?;

			Some(BitcoinScript::default().push_uint(version as u32).push_bytes(program))
		} else {
			None
		}
	};

	if let Some(script) = try_decode_as_base58() {
		Ok(script)
	} else if let Some(script) = try_decode_as_bech32_or_bech32m() {
		Ok(script)
	} else {
		Err(BitcoinTransactionError::InvalidEgressAddress)
	}
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
		let locktime = 0u32.to_le_bytes();
		// signal to allow replacing this transaction by setting sequence number according to BIP
		// 125
		let sequence_number = (u32::MAX - 2).to_le_bytes();
		result.extend(version);
		result.push(segwit_marker);
		result.push(segwit_flag);
		result.extend(to_varint(self.inputs.len() as u64));
		result.extend(self.inputs.iter().fold(Vec::<u8>::default(), |mut acc, x| {
			acc.extend(x.txid.iter().rev());
			acc.extend(x.vout.to_le_bytes());
			acc.push(0);
			acc.extend(sequence_number);
			acc
		}));
		result.extend(to_varint(self.outputs.len() as u64));
		result.extend(self.outputs.iter().try_fold(Vec::<u8>::default(), |mut acc, x| {
			acc.extend(x.amount.to_le_bytes());
			acc.extend(x.script_pubkey.clone().serialize());
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
				.push_bytes(self.inputs[i].pubkey_x)
				.op_checksig()
				.serialize();
			result.extend(script);
			result.push(0x21u8); // Length of tweaked pubkey + leaf version
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

	pub fn get_signing_payload(
		&self,
		input_index: u32,
	) -> Result<[u8; 32], BitcoinTransactionError> {
		// SHA256("TapSighash")
		const TAPSIG_HASH: &[u8] =
			&hex_literal::hex!("f40a48df4b2a70c8b4924bf2654661ed3d95fd66a313eb87237597c628e4a031");
		let prevouts = sha2_256(
			self.inputs
				.iter()
				.fold(Vec::<u8>::default(), |mut acc, x| {
					acc.extend(x.txid.iter().rev());
					acc.extend(x.vout.to_le_bytes());
					acc
				})
				.as_slice(),
		);
		let amounts = sha2_256(
			self.inputs
				.iter()
				.fold(Vec::<u8>::default(), |mut acc, x| {
					acc.extend(x.amount.to_le_bytes());
					acc
				})
				.as_slice(),
		);
		let scriptpubkeys = sha2_256(
			self.inputs
				.iter()
				.fold(Vec::<u8>::default(), |mut acc, x| {
					let script = BitcoinScript::default()
						.push_uint(SEGWIT_VERSION as u32)
						.push_bytes(
							&tweaked_pubkey(x.pubkey_x, x.salt).serialize_compressed()[1..33],
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
					acc.extend(x.amount.to_le_bytes());
					acc.extend(x.script_pubkey.clone().serialize());
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
				// Tagged Hash according to BIP 340
				TAPSIG_HASH,
				TAPSIG_HASH,
				// Epoch according to footnote 20 in BIP 341
				&[epoch],
				// "Common signature message" according to BIP 341
				&[hashtype],
				&version,
				&locktime,
				&prevouts,
				&amounts,
				&scriptpubkeys,
				&sequences,
				&outputs,
				&[spendtype],
				&input_index.to_le_bytes(),
				// "Common signature message extension" according to BIP 342
				&get_tapleaf_hash(
					self.inputs[input_index as usize].pubkey_x,
					self.inputs[input_index as usize].salt,
				),
				&[keyversion],
				&codeseparator,
			]
			.concat(),
		))
	}
}

#[derive(Default, Clone)]
pub struct BitcoinScript {
	data: Vec<u8>,
}

/// For reference see https://en.bitcoin.it/wiki/Script
impl BitcoinScript {
	/// Adds an operation to the script that pushes an unsigned integer onto the stack
	fn push_uint(mut self, value: u32) -> Self {
		match value {
			0 => self.data.push(0),
			1..=16 => self.data.push(0x50 + value as u8),
			_ => {
				let num_bytes =
					sp_std::mem::size_of::<u32>() - (value.leading_zeros() / 8) as usize;
				self = self.push_bytes(value.to_le_bytes().into_iter().take(num_bytes));
			},
		}
		self
	}
	/// Adds an operation to the script that pushes exactly the provided bytes of data to the stack
	fn push_bytes<
		Bytes: IntoIterator<Item = Item, IntoIter = Iter>,
		Iter: ExactSizeIterator<Item = Item>,
		Item: Borrow<u8>,
	>(
		mut self,
		bytes: Bytes,
	) -> Self {
		let bytes = bytes.into_iter().map(|byte| *byte.borrow());
		let num_bytes = bytes.len();
		assert!(num_bytes <= u32::MAX as usize);
		let num_bytes = num_bytes as u32;
		match num_bytes {
			0x0 => self.data.extend(iter::once(0x0)),
			0x1..=0x4B => self.data.extend(itertools::chain!(iter::once(num_bytes as u8), bytes)),
			0x4C..=0xFF => self.data.extend(itertools::chain!(
				iter::once(0x4c),
				(num_bytes as u8).to_le_bytes(),
				bytes
			)),
			0x100..=0xFFFF => self.data.extend(itertools::chain!(
				iter::once(0x4d),
				(num_bytes as u16).to_le_bytes(),
				bytes
			)),
			_ => self.data.extend(itertools::chain!(
				iter::once(0x4e),
				num_bytes.to_le_bytes(),
				bytes
			)),
		}
		self
	}

	/// Adds an operation to the script that drops the topmost item from the stack
	fn op_drop(mut self) -> Self {
		self.data.push(0x75);
		self
	}
	/// Adds the CHECKSIG operation to the script
	fn op_checksig(mut self) -> Self {
		self.data.push(0xAC);
		self
	}
	/// Adds the DUP operation to the script
	fn op_dup(mut self) -> Self {
		self.data.push(0x76);
		self
	}
	/// Adds the HASH160 operation to the script
	fn op_hash160(mut self) -> Self {
		self.data.push(0xA9);
		self
	}
	/// Adds the EQUALVERIFY operation to the script
	fn op_equalverify(mut self) -> Self {
		self.data.push(0x88);
		self
	}
	/// Adds the EQUAL operation to the script
	fn op_equal(mut self) -> Self {
		self.data.push(0x87);
		self
	}
	/// Serializes the script by returning a single byte for the length
	/// of the script and then the script itself
	fn serialize(self) -> Vec<u8> {
		itertools::chain!(to_varint(self.data.len() as u64), self.data).collect()
	}
}

#[cfg(test)]
mod test {
	use super::*;

	#[test]
	fn test_scriptpubkey_from_address() {
		// Test cases from: https://github.com/bitcoin/bips/blob/master/bip-0350.mediawiki

		let valid_addresses = [
			("BC1QW508D6QEJXTDG4Y5R3ZARVARY0C5XW7KV8F3T4", BitcoinNetwork::Mainnet, &hex_literal::hex!("0014751e76e8199196d454941c45d1b3a323f1433bd6")[..]),
			("tb1qrp33g0q5c5txsp9arysrx4k6zdkfs4nce4xj0gdcccefvpysxf3q0sl5k7", BitcoinNetwork::Testnet, &hex_literal::hex!("00201863143c14c5166804bd19203356da136c985678cd4d27a1b8c6329604903262")[..]),
			("bc1pw508d6qejxtdg4y5r3zarvary0c5xw7kw508d6qejxtdg4y5r3zarvary0c5xw7kt5nd6y", BitcoinNetwork::Mainnet, &hex_literal::hex!("5128751e76e8199196d454941c45d1b3a323f1433bd6751e76e8199196d454941c45d1b3a323f1433bd6")[..]),
			("BC1SW50QGDZ25J", BitcoinNetwork::Mainnet, &hex_literal::hex!("6002751e")[..]),
			("bc1zw508d6qejxtdg4y5r3zarvaryvaxxpcs", BitcoinNetwork::Mainnet, &hex_literal::hex!("5210751e76e8199196d454941c45d1b3a323")[..]),
			("tb1qqqqqp399et2xygdj5xreqhjjvcmzhxw4aywxecjdzew6hylgvsesrxh6hy", BitcoinNetwork::Testnet, &hex_literal::hex!("0020000000c4a5cad46221b2a187905e5266362b99d5e91c6ce24d165dab93e86433")[..]),
			("tb1pqqqqp399et2xygdj5xreqhjjvcmzhxw4aywxecjdzew6hylgvsesf3hn0c", BitcoinNetwork::Testnet, &hex_literal::hex!("5120000000c4a5cad46221b2a187905e5266362b99d5e91c6ce24d165dab93e86433")[..]),
			("bc1p0xlxvlhemja6c4dqv22uapctqupfhlxm9h8z3k2e72q4k9hcz7vqzk5jj0", BitcoinNetwork::Mainnet, &hex_literal::hex!("512079be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798")[..]),
		];

		for (valid_address, intended_btc_net, expected_scriptpubkey) in valid_addresses {
			assert_eq!(
				scriptpubkey_from_address(valid_address, intended_btc_net,).unwrap().data,
				expected_scriptpubkey
			);
		}

		let invalid_addresses = [
			(
				"tc1p0xlxvlhemja6c4dqv22uapctqupfhlxm9h8z3k2e72q4k9hcz7vq5zuyut",
				BitcoinNetwork::Mainnet,
			),
			(
				"bc1p0xlxvlhemja6c4dqv22uapctqupfhlxm9h8z3k2e72q4k9hcz7vqh2y7hd",
				BitcoinNetwork::Mainnet,
			),
			(
				"tb1z0xlxvlhemja6c4dqv22uapctqupfhlxm9h8z3k2e72q4k9hcz7vqglt7rf",
				BitcoinNetwork::Testnet,
			),
			(
				"BC1S0XLXVLHEMJA6C4DQV22UAPCTQUPFHLXM9H8Z3K2E72Q4K9HCZ7VQ54WELL",
				BitcoinNetwork::Mainnet,
			),
			("bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kemeawh", BitcoinNetwork::Mainnet),
			(
				"tb1q0xlxvlhemja6c4dqv22uapctqupfhlxm9h8z3k2e72q4k9hcz7vq24jc47",
				BitcoinNetwork::Testnet,
			),
			(
				"bc1p38j9r5y49hruaue7wxjce0updqjuyyx0kh56v8s25huc6995vvpql3jow4",
				BitcoinNetwork::Mainnet,
			),
			(
				"BC130XLXVLHEMJA6C4DQV22UAPCTQUPFHLXM9H8Z3K2E72Q4K9HCZ7VQ7ZWS8R",
				BitcoinNetwork::Mainnet,
			),
			("bc1pw5dgrnzv", BitcoinNetwork::Mainnet),
			(
				"bc1p0xlxvlhemja6c4dqv22uapctqupfhlxm9h8z3k2e72q4k9hcz7v8n0nx0muaewav253zgeav",
				BitcoinNetwork::Mainnet,
			),
			("BC1QR508D6QEJXTDG4Y5R3ZARVARYV98GJ9P", BitcoinNetwork::Mainnet),
			(
				"tb1p0xlxvlhemja6c4dqv22uapctqupfhlxm9h8z3k2e72q4k9hcz7vq47Zagq",
				BitcoinNetwork::Testnet,
			),
			(
				"bc1p0xlxvlhemja6c4dqv22uapctqupfhlxm9h8z3k2e72q4k9hcz7v07qwwzcrf",
				BitcoinNetwork::Mainnet,
			),
			(
				"tb1p0xlxvlhemja6c4dqv22uapctqupfhlxm9h8z3k2e72q4k9hcz7vpggkg4j",
				BitcoinNetwork::Testnet,
			),
			("bc1gmk9yu", BitcoinNetwork::Mainnet),
		];

		for (invalid_address, intended_btc_net) in invalid_addresses {
			assert!(matches!(
				scriptpubkey_from_address(invalid_address, intended_btc_net,),
				Err(BitcoinTransactionError::InvalidEgressAddress)
			));
		}

		// Test cases from: https://rosettacode.org/wiki/Bitcoin/address_validation

		let test_addresses = [
			("1AGNa15ZQXAZUgFiqJ2i7Z2DPU2J6hW62i", true),
			("1Q1pE5vPGEEMqRcVRMbtBK842Y6Pzo6nK9", true),
			("1AGNa15ZQXAZUgFiqJ2i7Z2DPU2J6hW62X", false),
			("1ANNa15ZQXAZUgFiqJ2i7Z2DPU2J6hW62i", false),
			("1A Na15ZQXAZUgFiqJ2i7Z2DPU2J6hW62i", false),
			("1Q1pE5vPGEEMqRcVRMbtBK842Y6Pzo6nJ9", false),
			("1AGNa15ZQXAZUgFiqJ2i7Z2DPU2J6hW62I", false),
			("1AGNa15ZQXAZUgFiqJ2i7Z2DPU2J6hW62j", false),
			("1AGNa15ZQXAZUgFiqJ2i7Z2DPU2J6hW62!", false),
			("1AGNa15ZQXAZUgFiqJ2i7Z2DPU2J6hW62iz", false),
			("1AGNa15ZQXAZUgFiqJ2i7Z2DPU2J6hW62izz", false),
			("1BNGaR29FmfAqidXmD9HLwsGv9p5WVvvhq", true),
			("1BNGaR29FmfAqidXmD9HLws", false),
			("1NAGa15ZQXAZUgFiqJ2i7Z2DPU2J6hW62i", false),
			("0AGNa15ZQXAZUgFiqJ2i7Z2DPU2J6hW62i", false),
			("1AGNa15ZQXAZUgFlqJ2i7Z2DPU2J6hW62i", false),
			("1ANa55215ZQXAZUgFiqJ2i7Z2DPU2J6hW62i", false),
			("i55j", false),
			("BZbvjr", false),
			("3yQ", false),
			("1AGNa15ZQXAZUgFiqJ2i7Z2DPU2J6hW62ix", false),
			("1AGNa15ZQXAZUgFiqJ2i7Z2DPU2J6hW62ixx", false),
			("17NdbrSGoUotzeGCcMMCqnFkEvLymoou9j", true),
			("1badbadbadbadbadbadbadbadbadbadbad", false),
			("16UwLL9Risc3QfPqBUvKofHmBQ7wMtjvM", true),
			("1111111111111111111114oLvT2", true),
			("1BGNa15ZQXAZUgFiqJ2i7Z2DPU2J6hW62i", false),
			("1AGNa15ZQXAZUgFiqJ3i7Z2DPU2J6hW62i", false),
		];

		for (address, validity) in test_addresses {
			assert_eq!(
				scriptpubkey_from_address(address, BitcoinNetwork::Mainnet,).is_ok(),
				validity
			);
		}
	}

	#[test]
	fn test_finalize() {
		let input = Utxo {
			amount: 100010000,
			vout: 1,
			txid: hex_literal::hex!(
				"b5bb9d8014a0f9b1d61e21e796d78dccdf1352f23cd32812f4850b878ae4944c"
			),
			pubkey_x: hex_literal::hex!(
				"78C79A2B436DA5575A03CDE40197775C656FFF9F0F59FC1466E09C20A81A9CDB"
			),
			salt: 123,
		};
		let output = BitcoinOutput {
			amount: 100000000,
			script_pubkey: scriptpubkey_from_address(
				"bc1pgtj0f3u2rk8ex6khlskz7q50nwc48r8unfgfhxzsx9zhcdnczhqq60lzjt",
				BitcoinNetwork::Mainnet,
			)
			.unwrap(),
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
			txid: hex_literal::hex!(
				"b5bb9d8014a0f9b1d61e21e796d78dccdf1352f23cd32812f4850b878ae4944c"
			),
			pubkey_x: hex_literal::hex!(
				"78C79A2B436DA5575A03CDE40197775C656FFF9F0F59FC1466E09C20A81A9CDB"
			),
			salt: 123,
		};
		let output = BitcoinOutput {
			amount: 100000000,
			script_pubkey: scriptpubkey_from_address(
				"bc1pgtj0f3u2rk8ex6khlskz7q50nwc48r8unfgfhxzsx9zhcdnczhqq60lzjt",
				BitcoinNetwork::Mainnet,
			)
			.unwrap(),
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
					hex::decode("2E897376020217C8E385A30B74B758293863049FA66A3FD177E012B076059105")
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
			assert_eq!(BitcoinScript::default().push_uint(x.0).data, x.1);
		}
	}

	#[test]
	fn test_varint() {
		let test_data = [
			(0_u64, vec![0x00]),
			(1, vec![0x01]),
			(252, vec![0xFC]),
			(253, vec![0xFD, 0xFD, 0x00]),
			(254, vec![0xFD, 0xFE, 0x00]),
			(255, vec![0xFD, 0xFF, 0x00]),
			(65534, vec![0xFD, 0xFE, 0xFF]),
			(65535, vec![0xFD, 0xFF, 0xFF]),
			(65536, vec![0xFE, 0x00, 0x00, 0x01, 0x00]),
			(65537, vec![0xFE, 0x01, 0x00, 0x01, 0x00]),
			(4294967295, vec![0xFE, 0xFF, 0xFF, 0xFF, 0xFF]),
			(4294967296, vec![0xFF, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00]),
			(4294967297, vec![0xFF, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00]),
			(9007199254740991, vec![0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x1F, 0x00]),
		];
		for x in test_data {
			assert_eq!(to_varint(x.0), x.1);
		}
	}
}
