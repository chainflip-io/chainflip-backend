pub mod api;
#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;
pub mod ingress_address;
pub mod utxo_selection;

use arrayref::array_ref;
use base58::FromBase58;
use bech32::{self, u5, FromBase32, ToBase32, Variant};
use codec::{Decode, Encode, MaxEncodedLen};
use core::{borrow::Borrow, iter};
use frame_support::{sp_io::hashing::sha2_256, RuntimeDebug};
use libsecp256k1::{curve::*, PublicKey, SecretKey};
use scale_info::TypeInfo;
use sp_std::{vec, vec::Vec};

extern crate alloc;
use crate::{
	address::BitcoinAddressData, Chain, ChainAbi, ChainCrypto, FeeRefundCalculator,
	IngressIdConstructor,
};
use alloc::string::String;
pub use cf_primitives::chains::Bitcoin;
use cf_primitives::{chains::assets, EpochIndex, IntentId, KeyId, PublicKeyBytes};
use itertools;

/// This salt is used to derive the change address for every vault. i.e. for every epoch.
pub const CHANGE_ADDRESS_SALT: u32 = 0;

pub type BlockNumber = u64;

#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq, Copy)]
pub struct BitcoinFetchId(u64);

// TODO: Come back to this. in BTC u64 works, but the trait has from u128 required, so we do this
// for now
pub type BtcAmount = u128;

pub type SigningPayload = [u8; 32];

pub type Signature = [u8; 64];

pub type Hash = [u8; 32];

#[derive(
	Copy,
	Clone,
	RuntimeDebug,
	Default,
	PartialEq,
	Eq,
	Encode,
	Decode,
	MaxEncodedLen,
	TypeInfo,
	Ord,
	PartialOrd,
)]

/// The public key x-coordinate
pub struct AggKey(pub [u8; 32]);

impl From<KeyId> for AggKey {
	fn from(key_id: KeyId) -> Self {
		AggKey(key_id.public_key_bytes.try_into().unwrap())
	}
}

impl From<AggKey> for PublicKeyBytes {
	fn from(agg_key: AggKey) -> Self {
		agg_key.0.to_vec()
	}
}

impl From<PublicKeyBytes> for AggKey {
	fn from(public_key_bytes: PublicKeyBytes) -> Self {
		AggKey(public_key_bytes.try_into().unwrap())
	}
}

#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, Default, PartialEq, Eq)]
pub struct BitcoinTransactionData {
	pub encoded_transaction: Vec<u8>,
}

impl FeeRefundCalculator<Bitcoin> for BitcoinTransactionData {
	fn return_fee_refund(
		&self,
		fee_paid: <Bitcoin as Chain>::TransactionFee,
	) -> <Bitcoin as Chain>::ChainAmount {
		fee_paid
	}
}

#[derive(Clone, Encode, Decode, MaxEncodedLen, TypeInfo, Debug, PartialEq, Eq)]
pub struct EpochStartData {
	pub change_address: BitcoinAddressData,
}

impl Chain for Bitcoin {
	type ChainBlockNumber = BlockNumber;

	type ChainAmount = BtcAmount;

	type TransactionFee = Self::ChainAmount;

	type TrackedData = ();

	type ChainAsset = assets::btc::Asset;

	type ChainAccount = BitcoinAddressData;

	type EpochStartData = EpochStartData;

	type IngressFetchId = BitcoinFetchId;
}

impl ChainCrypto for Bitcoin {
	type AggKey = AggKey;

	// A single transaction can sign over multiple UTXOs
	type Payload = Vec<SigningPayload>;

	// The response from a threshold signing ceremony over multiple payloads
	// is multiple signatures
	type ThresholdSignature = Vec<Signature>;

	type TransactionId = UtxoId;

	type GovKey = Self::AggKey;

	fn verify_threshold_signature(
		agg_key: &Self::AggKey,
		payloads: &Self::Payload,
		signatures: &Self::ThresholdSignature,
	) -> bool {
		payloads.iter().zip(signatures).all(|(payload, signature)| {
			verify_single_threshold_signature(agg_key, payload, signature)
		})
	}

	fn agg_key_to_payload(agg_key: Self::AggKey) -> Self::Payload {
		vec![agg_key.0]
	}

	fn agg_key_to_key_id(agg_key: Self::AggKey, epoch_index: EpochIndex) -> KeyId {
		KeyId { epoch_index, public_key_bytes: agg_key.into() }
	}
}
fn verify_single_threshold_signature(
	agg_key: &AggKey,
	payload: &[u8; 32],
	signature: &[u8; 64],
) -> bool {
	// SHA256("BIP0340/challenge")
	const CHALLENGE_TAG: &[u8] =
		&hex_literal::hex!("7bb52d7a9fef58323eb1bf7a407db382d2f3f2d81bb1224f49fe518f6d48d37c");
	let mut rx = Field::default();
	if !rx.set_b32(array_ref!(signature, 0, 32)) {
		return false
	}
	let mut pubx = Field::default();
	if !pubx.set_b32(&agg_key.0) {
		return false
	}
	let mut pubkey = Affine::default();
	if !pubkey.set_xo_var(&pubx, false) {
		return false
	}

	let mut challenge = Scalar::default();
	let _unused = challenge.set_b32(&sha2_256(
		&[CHALLENGE_TAG, CHALLENGE_TAG, &rx.b32(), &agg_key.0, payload].concat(),
	));
	challenge.cond_neg_assign(1.into());

	let mut s = Scalar::default();
	let _unused = s.set_b32(array_ref!(signature, 32, 32));

	let mut temp_r = Jacobian::default();
	libsecp256k1::ECMULT_CONTEXT.ecmult(&mut temp_r, &Jacobian::from_ge(&pubkey), &challenge, &s);
	let mut recovered_r = Affine::from_gej(&temp_r);
	if recovered_r.is_infinity() {
		return false
	}
	recovered_r.y.normalize();
	if recovered_r.y.is_odd() {
		return false
	}
	recovered_r.x.normalize();
	recovered_r.x.eq_var(&rx)
}

impl ChainAbi for Bitcoin {
	type Transaction = BitcoinTransactionData;

	type ReplayProtection = ();
}

// TODO: Look at moving this into Utxo. They're exactly the same apart from the IntentId
// which could be made generic, if even necessary at all.
#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq)]
pub struct UtxoId {
	// Tx hash of the transaction this utxo was a part of
	pub tx_hash: Hash,
	// The index of the output for this utxo
	pub vout: u32,
	// The public key of the account that can spend this utxo
	pub pubkey_x: [u8; 32],
	// Salt used to generate an address from the public key. In our case its the intent id of the
	// swap
	pub salt: IntentId,
}

impl IngressIdConstructor for BitcoinFetchId {
	type Address = BitcoinAddressData;

	fn deployed(_intent_id: u64, _address: Self::Address) -> Self {
		todo!()
	}

	fn undeployed(_intent_id: u64, _address: Self::Address) -> Self {
		todo!()
	}
}

use self::ingress_address::tweaked_pubkey;

const INTERNAL_PUBKEY: &[u8] =
	&hex_literal::hex!("02eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee");

const SEGWIT_VERSION: u8 = 1;

#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq)]
pub enum Error {
	/// The address is invalid
	InvalidAddress,
}
#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq)]
pub struct Utxo {
	pub amount: u64,
	pub txid: Hash,
	pub vout: u32,
	pub pubkey_x: [u8; 32],
	// Salt used to create the address that this utxo was sent to.
	pub salt: u32,
}

pub trait GetUtxoAmount {
	fn amount(&self) -> u64;
}
impl GetUtxoAmount for Utxo {
	fn amount(&self) -> u64 {
		self.amount
	}
}

#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq)]
pub struct BitcoinOutput {
	amount: u64,
	script_pubkey: BitcoinScript,
}

fn get_tapleaf_hash(pubkey_x: [u8; 32], salt: u32) -> Hash {
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

#[derive(
	Clone, Copy, Debug, PartialEq, Eq, Encode, Decode, TypeInfo, MaxEncodedLen, PartialOrd, Ord,
)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub enum BitcoinNetwork {
	Mainnet,
	Testnet,
	Regtest,
}

impl Default for BitcoinNetwork {
	fn default() -> Self {
		BitcoinNetwork::Mainnet
	}
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
) -> Result<BitcoinScript, Error> {
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
		Err(Error::InvalidAddress)
	}
}

#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq)]
pub struct BitcoinTransaction {
	inputs: Vec<Utxo>,
	outputs: Vec<BitcoinOutput>,
	signatures: Vec<Signature>,
	transaction_bytes: Vec<u8>,
}

impl BitcoinTransaction {
	pub fn create_new_unsigned(inputs: Vec<Utxo>, outputs: Vec<BitcoinOutput>) -> Self {
		const VERSION: [u8; 4] = 2u32.to_le_bytes();
		const SEGWIT_MARKER: u8 = 0u8;
		const SEGWIT_FLAG: u8 = 1u8;
		const SEQUENCE_NUMBER: [u8; 4] = (u32::MAX - 2).to_le_bytes();

		let mut transaction_bytes = Vec::default();
		transaction_bytes.extend(VERSION);
		transaction_bytes.push(SEGWIT_MARKER);
		transaction_bytes.push(SEGWIT_FLAG);
		transaction_bytes.extend(to_varint(inputs.len() as u64));
		transaction_bytes.extend(inputs.iter().fold(Vec::<u8>::default(), |mut acc, input| {
			acc.extend(input.txid.iter().rev());
			acc.extend(input.vout.to_le_bytes());
			acc.push(0);
			acc.extend(SEQUENCE_NUMBER);
			acc
		}));
		transaction_bytes.extend(to_varint(outputs.len() as u64));
		transaction_bytes.extend(outputs.iter().fold(Vec::<u8>::default(), |mut acc, output| {
			acc.extend(output.amount.to_le_bytes());
			acc.extend(output.script_pubkey.serialize());
			acc
		}));
		Self { inputs, outputs, signatures: vec![], transaction_bytes }
	}
	pub fn add_signatures(&mut self, signatures: Vec<Signature>) {
		debug_assert_eq!(signatures.len(), self.inputs.len());
		self.signatures = signatures;
	}
	pub fn is_signed(&self) -> bool {
		self.signatures.len() == self.inputs.len() &&
			!self.signatures.iter().any(|signature| signature == &[0u8; 64])
	}
	pub fn finalize(self) -> Vec<u8> {
		const LOCKTIME: [u8; 4] = 0u32.to_le_bytes();
		const NUM_WITNESSES: u8 = 3u8;
		const LEN_SIGNATURE: u8 = 64u8;

		let mut transaction_bytes = self.transaction_bytes;

		for i in 0..self.inputs.len() {
			transaction_bytes.push(NUM_WITNESSES);
			transaction_bytes.push(LEN_SIGNATURE);
			transaction_bytes.extend(self.signatures[i]);
			transaction_bytes.extend(
				BitcoinScript::default()
					.push_uint(self.inputs[i].salt)
					.op_drop()
					.push_bytes(self.inputs[i].pubkey_x)
					.op_checksig()
					.serialize(),
			);
			transaction_bytes.push(0x21u8); // Length of tweaked pubkey + leaf version
			let tweaked = tweaked_pubkey(self.inputs[i].pubkey_x, self.inputs[i].salt);
			// push correct leaf version depending on evenness of public key
			if tweaked.serialize_compressed()[0] == 2 {
				transaction_bytes.push(0xC0_u8);
			} else {
				transaction_bytes.push(0xC1_u8);
			}
			transaction_bytes.extend(INTERNAL_PUBKEY[1..33].iter());
		}
		transaction_bytes.extend(LOCKTIME);
		transaction_bytes
	}

	pub fn get_signing_payloads(&self) -> Vec<SigningPayload> {
		// SHA256("TapSighash")
		const TAPSIG_HASH: &[u8] =
			&hex_literal::hex!("f40a48df4b2a70c8b4924bf2654661ed3d95fd66a313eb87237597c628e4a031");
		const EPOCH: u8 = 0u8;
		const HASHTYPE: u8 = 0u8;
		const VERSION: [u8; 4] = 2u32.to_le_bytes();
		const LOCKTIME: [u8; 4] = 0u32.to_le_bytes();
		const SPENDTYPE: u8 = 2u8;
		const KEYVERSION: u8 = 0u8;
		const CODESEPARATOR: [u8; 4] = u32::MAX.to_le_bytes();
		const SEQUENCE_NUMBER: [u8; 4] = (u32::MAX - 2).to_le_bytes();

		let prevouts = sha2_256(
			self.inputs
				.iter()
				.fold(Vec::<u8>::default(), |mut acc, input| {
					acc.extend(input.txid.iter().rev());
					acc.extend(input.vout.to_le_bytes());
					acc
				})
				.as_slice(),
		);
		let amounts = sha2_256(
			self.inputs
				.iter()
				.fold(Vec::<u8>::default(), |mut acc, input| {
					acc.extend(input.amount.to_le_bytes());
					acc
				})
				.as_slice(),
		);
		let scriptpubkeys = sha2_256(
			self.inputs
				.iter()
				.fold(Vec::<u8>::default(), |mut acc, input| {
					let script = BitcoinScript::default()
						.push_uint(SEGWIT_VERSION as u32)
						.push_bytes(
							&tweaked_pubkey(input.pubkey_x, input.salt).serialize_compressed()
								[1..33],
						)
						.serialize();
					acc.extend(script);
					acc
				})
				.as_slice(),
		);
		let sequences = sha2_256(
			&core::iter::repeat(SEQUENCE_NUMBER)
				.take(self.inputs.len())
				.collect::<Vec<_>>()
				.concat(),
		);
		let outputs = sha2_256(
			self.outputs
				.iter()
				.fold(Vec::<u8>::default(), |mut acc, output| {
					acc.extend(output.amount.to_le_bytes());
					acc.extend(output.script_pubkey.serialize());
					acc
				})
				.as_slice(),
		);

		(0u32..)
			.zip(&self.inputs)
			.map(|(input_index, input)| {
				sha2_256(
					&[
						// Tagged Hash according to BIP 340
						TAPSIG_HASH,
						TAPSIG_HASH,
						// Epoch according to footnote 20 in BIP 341
						&[EPOCH],
						// "Common signature message" according to BIP 341
						&[HASHTYPE],
						&VERSION,
						&LOCKTIME,
						&prevouts,
						&amounts,
						&scriptpubkeys,
						&sequences,
						&outputs,
						&[SPENDTYPE],
						&input_index.to_le_bytes(),
						// "Common signature message extension" according to BIP 342
						&get_tapleaf_hash(input.pubkey_x, input.salt),
						&[KEYVERSION],
						&CODESEPARATOR,
					]
					.concat(),
				)
			})
			.collect()
	}
}

#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq, Default)]
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
	pub fn serialize(&self) -> Vec<u8> {
		itertools::chain!(to_varint(self.data.len() as u64), self.data.iter().cloned()).collect()
	}
}

#[cfg(test)]
mod test {
	use super::*;

	#[test]
	fn test_verify_signature() {
		// test cases from https://github.com/bitcoin/bips/blob/master/bip-0340/test-vectors.csv
		assert!(verify_single_threshold_signature(
			&AggKey(hex_literal::hex!("3913CC82D3CE5A22409E61D1E42E7C60435A3DDCB9192CFDCF7D67C3F520EDAB")),
			&hex_literal::hex!("461E208488056167B18085A0B5CC62464BA8D854540D1BCC7AB987AB8F64FA53"),
			&hex_literal::hex!("719B74CE347D7CDA876C39DDEAB89EE750AC24091835300FD27E7783EC336232626EEAA1500F84326F4144F453FFE5AE44D35C503B36AD68C00C3A4AB12C3CFB")));
		assert!(verify_single_threshold_signature(
			&AggKey(hex_literal::hex!("F9308A019258C31049344F85F89D5229B531C845836F99B08601F113BCE036F9")),
			&hex_literal::hex!("0000000000000000000000000000000000000000000000000000000000000000"),
			&hex_literal::hex!("E907831F80848D1069A5371B402410364BDF1C5F8307B0084C55F1CE2DCA821525F66A4A85EA8B71E482A74F382D2CE5EBEEE8FDB2172F477DF4900D310536C0")));
		assert!(verify_single_threshold_signature(
			&AggKey(hex_literal::hex!("DFF1D77F2A671C5F36183726DB2341BE58FEAE1DA2DECED843240F7B502BA659")),
			&hex_literal::hex!("243F6A8885A308D313198A2E03707344A4093822299F31D0082EFA98EC4E6C89"),
			&hex_literal::hex!("6896BD60EEAE296DB48A229FF71DFE071BDE413E6D43F917DC8DCF8C78DE33418906D11AC976ABCCB20B091292BFF4EA897EFCB639EA871CFA95F6DE339E4B0A")));
		assert!(verify_single_threshold_signature(
			&AggKey(hex_literal::hex!("DD308AFEC5777E13121FA72B9CC1B7CC0139715309B086C960E18FD969774EB8")),
			&hex_literal::hex!("7E2D58D8B3BCDF1ABADEC7829054F90DDA9805AAB56C77333024B9D0A508B75C"),
			&hex_literal::hex!("5831AAEED7B44BB74E5EAB94BA9D4294C49BCF2A60728D8B4C200F50DD313C1BAB745879A5AD954A72C45A91C3A51D3C7ADEA98D82F8481E0E1E03674A6F3FB7")));
		assert!(verify_single_threshold_signature(
			&AggKey(hex_literal::hex!("25D1DFF95105F5253C4022F628A996AD3A0D95FBF21D468A1B33F8C160D8F517")),
			&hex_literal::hex!("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF"),
			&hex_literal::hex!("7EB0509757E246F19449885651611CB965ECC1A187DD51B64FDA1EDC9637D5EC97582B9CB13DB3933705B32BA982AF5AF25FD78881EBB32771FC5922EFC66EA3")));
		assert!(verify_single_threshold_signature(
			&AggKey(hex_literal::hex!("D69C3509BB99E412E68B0FE8544E72837DFA30746D8BE2AA65975F29D22DC7B9")),
			&hex_literal::hex!("4DF3C3F68FCC83B27E9D42C90431A72499F17875C81A599B566C9889B9696703"),
			&hex_literal::hex!("00000000000000000000003B78CE563F89A0ED9414F5AA28AD0D96D6795F9C6376AFB1548AF603B3EB45C9F8207DEE1060CB71C04E80F593060B07D28308D7F4")));
		assert!(!verify_single_threshold_signature(
			&AggKey(hex_literal::hex!("EEFDEA4CDB677750A420FEE807EACF21EB9898AE79B9768766E4FAA04A2D4A34")),
			&hex_literal::hex!("243F6A8885A308D313198A2E03707344A4093822299F31D0082EFA98EC4E6C89"),
			&hex_literal::hex!("6CFF5C3BA86C69EA4B7376F31A9BCB4F74C1976089B2D9963DA2E5543E17776969E89B4C5564D00349106B8497785DD7D1D713A8AE82B32FA79D5F7FC407D39B")));
		assert!(!verify_single_threshold_signature(
			&AggKey(hex_literal::hex!("DFF1D77F2A671C5F36183726DB2341BE58FEAE1DA2DECED843240F7B502BA659")),
			&hex_literal::hex!("243F6A8885A308D313198A2E03707344A4093822299F31D0082EFA98EC4E6C89"),
			&hex_literal::hex!("FFF97BD5755EEEA420453A14355235D382F6472F8568A18B2F057A14602975563CC27944640AC607CD107AE10923D9EF7A73C643E166BE5EBEAFA34B1AC553E2")));
		assert!(!verify_single_threshold_signature(
			&AggKey(hex_literal::hex!("DFF1D77F2A671C5F36183726DB2341BE58FEAE1DA2DECED843240F7B502BA659")),
			&hex_literal::hex!("243F6A8885A308D313198A2E03707344A4093822299F31D0082EFA98EC4E6C89"),
			&hex_literal::hex!("1FA62E331EDBC21C394792D2AB1100A7B432B013DF3F6FF4F99FCB33E0E1515F28890B3EDB6E7189B630448B515CE4F8622A954CFE545735AAEA5134FCCDB2BD")));
		assert!(!verify_single_threshold_signature(
			&AggKey(hex_literal::hex!("DFF1D77F2A671C5F36183726DB2341BE58FEAE1DA2DECED843240F7B502BA659")),
			&hex_literal::hex!("243F6A8885A308D313198A2E03707344A4093822299F31D0082EFA98EC4E6C89"),
			&hex_literal::hex!("6CFF5C3BA86C69EA4B7376F31A9BCB4F74C1976089B2D9963DA2E5543E177769961764B3AA9B2FFCB6EF947B6887A226E8D7C93E00C5ED0C1834FF0D0C2E6DA6")));
		assert!(!verify_single_threshold_signature(
			&AggKey(hex_literal::hex!("DFF1D77F2A671C5F36183726DB2341BE58FEAE1DA2DECED843240F7B502BA659")),
			&hex_literal::hex!("243F6A8885A308D313198A2E03707344A4093822299F31D0082EFA98EC4E6C89"),
			&hex_literal::hex!("0000000000000000000000000000000000000000000000000000000000000000123DDA8328AF9C23A94C1FEECFD123BA4FB73476F0D594DCB65C6425BD186051")));
		assert!(!verify_single_threshold_signature(
			&AggKey(hex_literal::hex!("DFF1D77F2A671C5F36183726DB2341BE58FEAE1DA2DECED843240F7B502BA659")),
			&hex_literal::hex!("243F6A8885A308D313198A2E03707344A4093822299F31D0082EFA98EC4E6C89"),
			&hex_literal::hex!("00000000000000000000000000000000000000000000000000000000000000017615FBAF5AE28864013C099742DEADB4DBA87F11AC6754F93780D5A1837CF197")));
		assert!(!verify_single_threshold_signature(
			&AggKey(hex_literal::hex!("DFF1D77F2A671C5F36183726DB2341BE58FEAE1DA2DECED843240F7B502BA659")),
			&hex_literal::hex!("243F6A8885A308D313198A2E03707344A4093822299F31D0082EFA98EC4E6C89"),
			&hex_literal::hex!("4A298DACAE57395A15D0795DDBFD1DCB564DA82B0F269BC70A74F8220429BA1D69E89B4C5564D00349106B8497785DD7D1D713A8AE82B32FA79D5F7FC407D39B")));
		assert!(!verify_single_threshold_signature(
			&AggKey(hex_literal::hex!("DFF1D77F2A671C5F36183726DB2341BE58FEAE1DA2DECED843240F7B502BA659")),
			&hex_literal::hex!("243F6A8885A308D313198A2E03707344A4093822299F31D0082EFA98EC4E6C89"),
			&hex_literal::hex!("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEFFFFFC2F69E89B4C5564D00349106B8497785DD7D1D713A8AE82B32FA79D5F7FC407D39B")));
		assert!(!verify_single_threshold_signature(
			&AggKey(hex_literal::hex!("DFF1D77F2A671C5F36183726DB2341BE58FEAE1DA2DECED843240F7B502BA659")),
			&hex_literal::hex!("243F6A8885A308D313198A2E03707344A4093822299F31D0082EFA98EC4E6C89"),
			&hex_literal::hex!("6CFF5C3BA86C69EA4B7376F31A9BCB4F74C1976089B2D9963DA2E5543E177769FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141")));
		assert!(!verify_single_threshold_signature(
			&AggKey(hex_literal::hex!("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEFFFFFC30")),
			&hex_literal::hex!("243F6A8885A308D313198A2E03707344A4093822299F31D0082EFA98EC4E6C89"),
			&hex_literal::hex!("6CFF5C3BA86C69EA4B7376F31A9BCB4F74C1976089B2D9963DA2E5543E17776969E89B4C5564D00349106B8497785DD7D1D713A8AE82B32FA79D5F7FC407D39B")));
	}

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
				Err(Error::InvalidAddress)
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
		let mut tx = BitcoinTransaction::create_new_unsigned(vec![input], vec![output]);
		tx.add_signatures(vec![[0u8; 64]]);
		assert_eq!(tx.finalize(), hex_literal::hex!("020000000001014C94E48A870B85F41228D33CF25213DFCC8DD796E7211ED6B1F9A014809DBBB50100000000FDFFFFFF0100E1F5050000000022512042E4F4C78A1D8F936AD7FC2C2F028F9BB1538CFC9A509B985031457C367815C003400000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000025017B752078C79A2B436DA5575A03CDE40197775C656FFF9F0F59FC1466E09C20A81A9CDBAC21C0EEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEE00000000"));
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
		let tx = BitcoinTransaction::create_new_unsigned(vec![input], vec![output]);
		assert_eq!(
			tx.get_signing_payloads(),
			vec![hex_literal::hex!(
				"E16117C6CD69142E41736CE2882F0E697FF4369A2CBCEE9D92FC0346C6774FB4"
			)]
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
