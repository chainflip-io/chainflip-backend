pub mod api;
pub mod benchmarking;
pub mod deposit_address;
pub mod utxo_selection;

extern crate alloc;
use core::{cmp::max, mem::size_of};

use self::deposit_address::DepositAddress;
use crate::{
	Chain, ChainCrypto, DepositChannel, FeeEstimationApi, FeeRefundCalculator, RetryPolicy,
};
use alloc::{collections::VecDeque, string::String};
use arrayref::array_ref;
use base58::{FromBase58, ToBase58};
use bech32::{self, u5, FromBase32, ToBase32, Variant};
pub use cf_primitives::chains::Bitcoin;
use cf_primitives::{
	chains::assets, NetworkEnvironment, DEFAULT_FEE_SATS_PER_KILOBYTE, INPUT_UTXO_SIZE_IN_BYTES,
	MINIMUM_BTC_TX_SIZE_IN_BYTES, OUTPUT_UTXO_SIZE_IN_BYTES, VAULT_UTXO_SIZE_IN_BYTES,
};
use cf_utilities::SliceToArray;
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{
	pallet_prelude::RuntimeDebug,
	sp_runtime::{FixedPointNumber, FixedU128},
	traits::{ConstBool, ConstU32},
	BoundedVec,
};
use itertools;
use libsecp256k1::{curve::*, PublicKey, SecretKey};
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_io::hashing::sha2_256;
use sp_std::{vec, vec::Vec};

/// This salt is used to derive the change address for every vault. i.e. for every epoch.
pub const CHANGE_ADDRESS_SALT: u32 = 0;

// The bitcoin script generated from the bitcoin address should not exceed this value according to
// our construction
pub const MAX_BITCOIN_SCRIPT_LENGTH: u32 = 128;

// We must send strictly greater than this amount to avoid hitting the Bitcoin dust
// limit
pub const BITCOIN_DUST_LIMIT: u64 = 600;

pub type BlockNumber = u64;

#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq, Copy)]
pub struct BitcoinFetchId(pub u64);

pub type BtcAmount = u64;

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
	serde::Serialize,
	serde::Deserialize,
)]
/// A Bitcoin AggKey is made up of the previous and current public key x coordinates.
/// The y parity bits are assumed to be always equal to 0x02.
pub struct AggKey {
	pub previous: Option<[u8; 32]>,
	pub current: [u8; 32],
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

#[derive(
	Copy,
	Clone,
	RuntimeDebug,
	PartialEq,
	Eq,
	Encode,
	Decode,
	MaxEncodedLen,
	TypeInfo,
	Serialize,
	Deserialize,
)]
#[codec(mel_bound())]
pub struct BitcoinTrackedData {
	pub btc_fee_info: BitcoinFeeInfo,
}

impl Default for BitcoinTrackedData {
	#[track_caller]
	fn default() -> Self {
		panic!("You should not use the default chain tracking, as it's meaningless.");
	}
}

/// A constant multiplier applied to the fees.
///
/// TODO: Allow this value to adjust based on the current fee deficit/surplus.
const BTC_FEE_MULTIPLIER: FixedU128 = FixedU128::from_rational(3, 2);

impl FeeEstimationApi<Bitcoin> for BitcoinTrackedData {
	fn estimate_ingress_fee(
		&self,
		_asset: <Bitcoin as Chain>::ChainAsset,
	) -> <Bitcoin as Chain>::ChainAmount {
		BTC_FEE_MULTIPLIER.saturating_mul_int(self.btc_fee_info.fee_per_input_utxo())
	}

	fn estimate_egress_fee(
		&self,
		_asset: <Bitcoin as Chain>::ChainAsset,
	) -> <Bitcoin as Chain>::ChainAmount {
		BTC_FEE_MULTIPLIER.saturating_mul_int(
			self.btc_fee_info
				.min_fee_required_per_tx()
				.saturating_add(self.btc_fee_info.fee_per_output_utxo()),
		)
	}
}

/// A record of the Bitcoin transaction fee.
#[derive(
	Copy,
	Clone,
	RuntimeDebug,
	PartialEq,
	Eq,
	Encode,
	Decode,
	MaxEncodedLen,
	TypeInfo,
	Serialize,
	Deserialize,
)]
pub struct BitcoinFeeInfo {
	sats_per_kilobyte: BtcAmount,
}

// See https://github.com/bitcoin/bitcoin/blob/master/src/policy/feerate.h#L35
const BYTES_PER_BTC_KILOBYTE: BtcAmount = 1000;

impl Default for BitcoinFeeInfo {
	fn default() -> Self {
		Self { sats_per_kilobyte: DEFAULT_FEE_SATS_PER_KILOBYTE }
	}
}

impl BitcoinFeeInfo {
	pub fn new(sats_per_kilobyte: BtcAmount) -> Self {
		Self { sats_per_kilobyte: max(sats_per_kilobyte, BYTES_PER_BTC_KILOBYTE) }
	}

	pub fn sats_per_kilobyte(&self) -> BtcAmount {
		self.sats_per_kilobyte
	}

	pub fn fee_for_utxo(&self, utxo: &Utxo) -> BtcAmount {
		if utxo.deposit_address.script_path.is_none() {
			// Our vault utxos (salt = 0) use VAULT_UTXO_SIZE_IN_BYTES vbytes in a Btc transaction
			self.sats_per_kilobyte.saturating_mul(VAULT_UTXO_SIZE_IN_BYTES) / BYTES_PER_BTC_KILOBYTE
		} else {
			// Our input utxos are approximately INPUT_UTXO_SIZE_IN_BYTES vbytes each in the Btc
			// transaction
			self.sats_per_kilobyte.saturating_mul(INPUT_UTXO_SIZE_IN_BYTES) / BYTES_PER_BTC_KILOBYTE
		}
	}

	pub fn fee_per_input_utxo(&self) -> BtcAmount {
		// Our input utxos are approximately INPUT_UTXO_SIZE_IN_BYTES vbytes each in the Btc
		// transaction
		self.sats_per_kilobyte.saturating_mul(INPUT_UTXO_SIZE_IN_BYTES) / BYTES_PER_BTC_KILOBYTE
	}

	pub fn fee_per_output_utxo(&self) -> BtcAmount {
		// Our output utxos are approximately OUTPUT_UTXO_SIZE_IN_BYTES vbytes each in the Btc
		// transaction
		self.sats_per_kilobyte.saturating_mul(OUTPUT_UTXO_SIZE_IN_BYTES) / BYTES_PER_BTC_KILOBYTE
	}

	pub fn min_fee_required_per_tx(&self) -> BtcAmount {
		// Minimum size of tx that does not scale with input and output utxos is
		// MINIMUM_BTC_TX_SIZE_IN_BYTES bytes
		self.sats_per_kilobyte.saturating_mul(MINIMUM_BTC_TX_SIZE_IN_BYTES) / BYTES_PER_BTC_KILOBYTE
	}
}

#[derive(Clone, Encode, Decode, MaxEncodedLen, TypeInfo, Debug, PartialEq, Eq)]
pub struct EpochStartData {
	pub change_pubkey: AggKey,
}

#[derive(Encode, Decode, Default, PartialEq, Copy, Clone, TypeInfo, RuntimeDebug)]
pub struct ConsolidationParameters {
	/// Consolidate when total UTXO count reaches this threshold
	pub consolidation_threshold: u32,
	/// Consolidate this many UTXOs
	pub consolidation_size: u32,
}

impl ConsolidationParameters {
	#[cfg(test)]
	fn new(consolidation_threshold: u32, consolidation_size: u32) -> ConsolidationParameters {
		ConsolidationParameters { consolidation_threshold, consolidation_size }
	}

	pub fn are_valid(&self) -> bool {
		self.consolidation_size <= self.consolidation_threshold && (self.consolidation_size > 1)
	}
}

impl Chain for Bitcoin {
	const NAME: &'static str = "Bitcoin";
	const GAS_ASSET: Self::ChainAsset = assets::btc::Asset::Btc;

	type ChainCrypto = BitcoinCrypto;
	type ChainBlockNumber = BlockNumber;
	type ChainAmount = BtcAmount;
	type TransactionFee = Self::ChainAmount;
	type TrackedData = BitcoinTrackedData;
	type ChainAsset = assets::btc::Asset;
	type ChainAccount = ScriptPubkey;
	type EpochStartData = EpochStartData;
	type DepositFetchId = BitcoinFetchId;
	type DepositChannelState = DepositAddress;
	type DepositDetails = UtxoId;
	type Transaction = BitcoinTransactionData;
	type TransactionMetadata = ();
	// There is no need for replay protection on Bitcoin since it is a UTXO chain.
	type ReplayProtectionParams = ();
	type ReplayProtection = ();
}

#[derive(Clone, Copy, Encode, Decode, MaxEncodedLen, TypeInfo, Debug, PartialEq, Eq)]
pub enum PreviousOrCurrent {
	Previous,
	Current,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BitcoinCrypto;
impl ChainCrypto for BitcoinCrypto {
	type UtxoChain = ConstBool<true>;

	type AggKey = AggKey;

	// A single transaction can sign over multiple UTXOs
	type Payload = Vec<(PreviousOrCurrent, SigningPayload)>;

	// The response from a threshold signing ceremony over multiple payloads
	// is multiple signatures
	type ThresholdSignature = Vec<Signature>;
	type TransactionInId = Hash;
	type TransactionOutId = Hash;
	type KeyHandoverIsRequired = ConstBool<true>;

	type GovKey = Self::AggKey;

	fn verify_threshold_signature(
		agg_key: &Self::AggKey,
		payloads: &Self::Payload,
		signatures: &Self::ThresholdSignature,
	) -> bool {
		payloads
			.iter()
			.zip(signatures)
			.all(|((previous_or_current, payload), signature)| {
				match previous_or_current {
					PreviousOrCurrent::Previous => agg_key.previous.as_ref(),
					PreviousOrCurrent::Current => Some(&agg_key.current),
				}
				.map_or(false, |key| verify_single_threshold_signature(key, payload, signature))
			})
	}

	fn agg_key_to_payload(agg_key: Self::AggKey, for_handover: bool) -> Self::Payload {
		let payload = if for_handover {
			(
				PreviousOrCurrent::Previous,
				agg_key.previous.expect("previous key must exist after handover"),
			)
		} else {
			(PreviousOrCurrent::Current, agg_key.current)
		};
		vec![payload]
	}

	fn handover_key_matches(current_key: &Self::AggKey, new_key: &Self::AggKey) -> bool {
		new_key.previous.is_some_and(|previous| current_key.current == previous)
	}

	fn maybe_broadcast_barriers_on_rotation(
		_rotation_broadcast_id: cf_primitives::BroadcastId,
	) -> Vec<cf_primitives::BroadcastId> {
		// we dont need to put broadcast barriers for Bitcoin
		vec![]
	}
}

fn verify_single_threshold_signature(
	pubkey_x: &[u8; 32],
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
	if !pubx.set_b32(pubkey_x) {
		return false
	}
	let mut pubkey = Affine::default();
	if !pubkey.set_xo_var(&pubx, false) {
		return false
	}

	let mut challenge = Scalar::default();
	let _unused = challenge
		.set_b32(&sha2_256(&[CHALLENGE_TAG, CHALLENGE_TAG, &rx.b32(), pubkey_x, payload].concat()));
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

// TODO: Look at moving this into Utxo. They're exactly the same apart from the ChannelId
// which could be made generic, if even necessary at all.
#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq, MaxEncodedLen, Default)]
pub struct UtxoId {
	// TxId of the transaction in which this utxo was created.
	pub tx_id: Hash,
	// The index of the utxo in that transaction.
	pub vout: u32,
}

impl From<&DepositChannel<Bitcoin>> for BitcoinFetchId {
	fn from(channel: &DepositChannel<Bitcoin>) -> Self {
		BitcoinFetchId(channel.channel_id)
	}
}

const INTERNAL_PUBKEY: &[u8] =
	&hex_literal::hex!("02eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee");

#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq)]
pub enum Error {
	/// The address is invalid
	InvalidAddress,
}
#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq)]
pub struct Utxo {
	pub id: UtxoId,
	pub amount: u64,
	pub deposit_address: DepositAddress,
}

#[derive(Encode, Decode, TypeInfo, MaxEncodedLen, Clone, RuntimeDebug, PartialEq, Eq)]
pub struct BitcoinOutput {
	pub amount: u64,
	pub script_pubkey: ScriptPubkey,
}

impl SerializeBtc for BitcoinOutput {
	fn btc_encode_to(&self, buf: &mut Vec<u8>) {
		buf.extend(self.amount.to_le_bytes());
		buf.extend(self.script_pubkey.btc_serialize());
	}

	fn size(&self) -> usize {
		size_of::<u64>() + self.script_pubkey.size()
	}
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
	Clone,
	Copy,
	Debug,
	PartialEq,
	Eq,
	Encode,
	Decode,
	TypeInfo,
	MaxEncodedLen,
	PartialOrd,
	Ord,
	Default,
	serde::Serialize,
	serde::Deserialize,
)]
pub enum BitcoinNetwork {
	Mainnet,
	Testnet,
	#[default]
	Regtest,
}

impl From<NetworkEnvironment> for BitcoinNetwork {
	fn from(env: NetworkEnvironment) -> Self {
		match env {
			NetworkEnvironment::Mainnet => BitcoinNetwork::Mainnet,
			NetworkEnvironment::Testnet => BitcoinNetwork::Testnet,
			NetworkEnvironment::Development => BitcoinNetwork::Regtest,
		}
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

impl core::fmt::Display for BitcoinNetwork {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		match self {
			BitcoinNetwork::Mainnet => write!(f, "main"),
			BitcoinNetwork::Testnet => write!(f, "test"),
			BitcoinNetwork::Regtest => write!(f, "regtest"),
		}
	}
}

#[cfg(feature = "std")]
impl TryFrom<&str> for BitcoinNetwork {
	type Error = anyhow::Error;

	fn try_from(s: &str) -> Result<Self, Self::Error> {
		match s {
			"main" => Ok(BitcoinNetwork::Mainnet),
			"test" => Ok(BitcoinNetwork::Testnet),
			"regtest" => Ok(BitcoinNetwork::Regtest),
			unknown => Err(anyhow::anyhow!("Unknown Bitcoin network: {unknown}")),
		}
	}
}

const SEGWIT_VERSION_ZERO: u8 = 0;
const SEGWIT_VERSION_TAPROOT: u8 = 1;
const SEGWIT_VERSION_MAX: u8 = 16;
const MIN_SEGWIT_PROGRAM_BYTES: u32 = 2;
const MAX_SEGWIT_PROGRAM_BYTES: u32 = 40;

#[derive(
	Clone,
	Debug,
	PartialEq,
	Eq,
	PartialOrd,
	Ord,
	Encode,
	Decode,
	TypeInfo,
	MaxEncodedLen,
	Serialize,
	Deserialize,
)]
pub enum ScriptPubkey {
	P2PKH([u8; 20]),
	P2SH([u8; 20]),
	P2WPKH([u8; 20]),
	P2WSH([u8; 32]),
	Taproot([u8; 32]),
	OtherSegwit { version: u8, program: BoundedVec<u8, ConstU32<MAX_SEGWIT_PROGRAM_BYTES>> },
}

impl SerializeBtc for ScriptPubkey {
	fn btc_encode_to(&self, buf: &mut Vec<u8>) {
		self.program().btc_encode_to(buf)
	}

	fn size(&self) -> usize {
		self.program().size()
	}
}

impl ScriptPubkey {
	fn program(&self) -> BitcoinScript {
		match self {
			ScriptPubkey::P2PKH(hash) => BitcoinScript::new(&[
				BitcoinOp::Dup,
				BitcoinOp::Hash160,
				BitcoinOp::PushArray20 { bytes: *hash },
				BitcoinOp::EqualVerify,
				BitcoinOp::CheckSig,
			]),
			ScriptPubkey::P2SH(hash) => BitcoinScript::new(&[
				BitcoinOp::Hash160,
				BitcoinOp::PushArray20 { bytes: *hash },
				BitcoinOp::Equal,
			]),
			ScriptPubkey::P2WPKH(hash) => BitcoinScript::new(&[
				BitcoinOp::PushVersion { version: SEGWIT_VERSION_ZERO },
				BitcoinOp::PushArray20 { bytes: *hash },
			]),
			ScriptPubkey::P2WSH(hash) => BitcoinScript::new(&[
				BitcoinOp::PushVersion { version: SEGWIT_VERSION_ZERO },
				BitcoinOp::PushArray32 { bytes: *hash },
			]),
			ScriptPubkey::Taproot(hash) => BitcoinScript::new(&[
				BitcoinOp::PushVersion { version: SEGWIT_VERSION_TAPROOT },
				BitcoinOp::PushArray32 { bytes: *hash },
			]),
			ScriptPubkey::OtherSegwit { version, program } => BitcoinScript::new(&[
				BitcoinOp::PushVersion { version: *version },
				BitcoinOp::PushBytes { bytes: program.clone() },
			]),
		}
	}

	pub fn bytes(&self) -> Vec<u8> {
		self.program().raw()
	}

	pub fn to_address(&self, network: &BitcoinNetwork) -> String {
		let (data, maybe_bech, version) = match self {
			ScriptPubkey::P2PKH(data) => (&data[..], None, network.p2pkh_address_version()),
			ScriptPubkey::P2SH(data) => (&data[..], None, network.p2sh_address_version()),
			ScriptPubkey::P2WPKH(data) => (&data[..], Some(Variant::Bech32), SEGWIT_VERSION_ZERO),
			ScriptPubkey::P2WSH(data) => (&data[..], Some(Variant::Bech32), SEGWIT_VERSION_ZERO),
			ScriptPubkey::Taproot(data) =>
				(&data[..], Some(Variant::Bech32m), SEGWIT_VERSION_TAPROOT),
			ScriptPubkey::OtherSegwit { version, program } =>
				(&program[..], Some(Variant::Bech32m), *version),
		};
		if let Some(variant) = maybe_bech {
			let version = u5::try_from_u8(version);
			bech32::encode(
				network.bech32_and_bech32m_address_hrp(),
				itertools::chain!(version, data.to_base32()).collect::<Vec<_>>(),
				variant,
			)
			.expect("Can only fail on invalid hrp.")
		} else {
			const CHECKSUM_LENGTH: usize = 4;
			let mut buf = Vec::with_capacity(1 + data.len() + CHECKSUM_LENGTH);
			buf.push(version);
			buf.extend_from_slice(data);
			let checksum =
				sha2_256(&sha2_256(&buf))[..CHECKSUM_LENGTH].as_array::<CHECKSUM_LENGTH>();
			buf.extend(checksum);
			buf.to_base58()
		}
	}

	pub fn try_from_address(address: &str, network: &BitcoinNetwork) -> Result<Self, Error> {
		// See https://en.bitcoin.it/wiki/Base58Check_encoding
		fn try_decode_as_base58(address: &str, network: &BitcoinNetwork) -> Option<ScriptPubkey> {
			const CHECKSUM_LENGTH: usize = 4;
			const PAYLOAD_LENGTH: usize = 21;

			let data: [u8; PAYLOAD_LENGTH + CHECKSUM_LENGTH] =
				address.from_base58().ok()?.try_into().ok()?;

			let (payload, checksum) = data.split_at(data.len() - CHECKSUM_LENGTH);

			if &sha2_256(&sha2_256(payload))[..CHECKSUM_LENGTH] == checksum {
				let [version, hash @ ..] = payload.as_array::<PAYLOAD_LENGTH>();
				if version == network.p2pkh_address_version() {
					Some(ScriptPubkey::P2PKH(hash.as_array()))
				} else if version == network.p2sh_address_version() {
					Some(ScriptPubkey::P2SH(hash.as_array()))
				} else {
					None
				}
			} else {
				None
			}
		}

		// See https://en.bitcoin.it/wiki/BIP_0350
		fn try_decode_as_bech32_or_bech32m(
			address: &str,
			network: &BitcoinNetwork,
		) -> Option<ScriptPubkey> {
			let (hrp, data, variant) = bech32::decode(address).ok()?;
			if hrp == network.bech32_and_bech32m_address_hrp() {
				let version = data.first()?.to_u8();
				let program = Vec::from_base32(&data[1..]).ok()?;
				match (version, variant, program.len() as u32) {
					(SEGWIT_VERSION_ZERO, Variant::Bech32, 20) =>
						Some(ScriptPubkey::P2WPKH(program.as_array())),
					(SEGWIT_VERSION_ZERO, Variant::Bech32, 32) =>
						Some(ScriptPubkey::P2WSH(program.as_array())),
					(SEGWIT_VERSION_TAPROOT, Variant::Bech32m, 32) =>
						Some(ScriptPubkey::Taproot(program.as_array())),
					(
						SEGWIT_VERSION_TAPROOT..=SEGWIT_VERSION_MAX,
						Variant::Bech32m,
						(MIN_SEGWIT_PROGRAM_BYTES..=MAX_SEGWIT_PROGRAM_BYTES),
					) => Some(ScriptPubkey::OtherSegwit {
						version,
						program: program.try_into().expect("Checked for MAX_SEGWIT_PROGRAM_BYTES"),
					}),
					_ => None,
				}
			} else {
				None
			}
		}

		try_decode_as_base58(address, network)
			.or_else(|| try_decode_as_bech32_or_bech32m(address, network))
			.ok_or(Error::InvalidAddress)
	}
}

#[derive(Encode, Decode, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq)]
pub struct BitcoinTransaction {
	inputs: Vec<Utxo>,
	pub outputs: Vec<BitcoinOutput>,
	signatures: Vec<Signature>,
	transaction_bytes: Vec<u8>,
	old_utxo_input_indices: VecDeque<u32>,
}

const LOCKTIME: [u8; 4] = 0u32.to_le_bytes();
const VERSION: [u8; 4] = 2u32.to_le_bytes();
const SEQUENCE_NUMBER: [u8; 4] = (u32::MAX - 2).to_le_bytes();

fn extend_with_inputs_outputs(bytes: &mut Vec<u8>, inputs: &[Utxo], outputs: &[BitcoinOutput]) {
	bytes.extend(to_varint(inputs.len() as u64));
	bytes.extend(inputs.iter().fold(Vec::<u8>::default(), |mut acc, input| {
		acc.extend(input.id.tx_id);
		acc.extend(input.id.vout.to_le_bytes());
		acc.push(0);
		acc.extend(SEQUENCE_NUMBER);
		acc
	}));

	outputs.btc_encode_to(bytes);
}

impl BitcoinTransaction {
	pub fn create_new_unsigned(
		agg_key: &AggKey,
		inputs: Vec<Utxo>,
		outputs: Vec<BitcoinOutput>,
	) -> Self {
		const SEGWIT_MARKER: u8 = 0u8;
		const SEGWIT_FLAG: u8 = 1u8;

		let old_utxo_input_indices = (0..)
			.zip(&inputs)
			.filter_map(|(i, Utxo { deposit_address, .. })| {
				if deposit_address.pubkey_x == agg_key.current {
					None
				} else {
					agg_key.previous.map(|previous| {
						// TODO: enforce this assumption ie. ensure we never use unspendable utxos.
						assert!(deposit_address.pubkey_x == previous);
						i
					})
				}
			})
			.collect::<VecDeque<_>>();

		let mut transaction_bytes = Vec::default();
		transaction_bytes.extend(VERSION);
		transaction_bytes.push(SEGWIT_MARKER);
		transaction_bytes.push(SEGWIT_FLAG);
		extend_with_inputs_outputs(&mut transaction_bytes, &inputs, &outputs);
		Self { inputs, outputs, signatures: vec![], transaction_bytes, old_utxo_input_indices }
	}

	pub fn add_signatures(&mut self, signatures: Vec<Signature>) {
		debug_assert_eq!(signatures.len(), self.inputs.len());
		self.signatures = signatures;
	}

	pub fn is_signed(&self) -> bool {
		self.signatures.len() == self.inputs.len() &&
			!self.signatures.iter().any(|signature| signature == &[0u8; 64])
	}

	pub fn txid(&self) -> [u8; 32] {
		let mut id_bytes = Vec::default();
		id_bytes.extend(VERSION);
		extend_with_inputs_outputs(&mut id_bytes, &self.inputs, &self.outputs);
		id_bytes.extend(&LOCKTIME);

		sha2_256(&sha2_256(&id_bytes))
	}

	pub fn finalize(self) -> Vec<u8> {
		const NUM_WITNESSES_SCRIPT: u8 = 3u8;
		const NUM_WITNESSES_KEY: u8 = 1u8;
		const LEN_SIGNATURE: u8 = 64u8;

		let mut transaction_bytes = self.transaction_bytes;

		for i in 0..self.inputs.len() {
			if let Some(script_path) = self.inputs[i].deposit_address.script_path.clone() {
				transaction_bytes.push(NUM_WITNESSES_SCRIPT);
				transaction_bytes.push(LEN_SIGNATURE);
				transaction_bytes.extend(self.signatures[i]);
				transaction_bytes.extend(script_path.unlock_script.btc_serialize());
				// Length of tweaked pubkey + leaf version
				transaction_bytes.push(33u8);
				transaction_bytes.push(script_path.leaf_version());
				transaction_bytes.extend(INTERNAL_PUBKEY[1..33].iter());
			} else {
				transaction_bytes.push(NUM_WITNESSES_KEY);
				transaction_bytes.push(LEN_SIGNATURE);
				transaction_bytes.extend(self.signatures[i]);
			}
		}
		transaction_bytes.extend(LOCKTIME);
		transaction_bytes
	}

	pub fn get_signing_payloads(
		&self,
	) -> <<Bitcoin as Chain>::ChainCrypto as ChainCrypto>::Payload {
		// SHA256("TapSighash")
		const TAPSIG_HASH: &[u8] =
			&hex_literal::hex!("f40a48df4b2a70c8b4924bf2654661ed3d95fd66a313eb87237597c628e4a031");
		const EPOCH: u8 = 0u8;
		const HASHTYPE: u8 = 0u8;
		const VERSION: [u8; 4] = 2u32.to_le_bytes();
		const SPENDTYPE_KEY: u8 = 0u8;
		const SPENDTYPE_SCRIPT: u8 = 2u8;
		const KEYVERSION: u8 = 0u8;
		const CODESEPARATOR: [u8; 4] = u32::MAX.to_le_bytes();

		let prevouts = sha2_256(
			self.inputs
				.iter()
				.fold(Vec::<u8>::default(), |mut acc, input| {
					acc.extend(input.id.tx_id);
					acc.extend(input.id.vout.to_le_bytes());
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
					acc.extend(input.deposit_address.script_pubkey().btc_serialize());
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
					acc.extend(output.script_pubkey.btc_serialize());
					acc
				})
				.as_slice(),
		);

		let mut old_utxo_input_indices = self.old_utxo_input_indices.clone();
		(0u32..)
			.zip(&self.inputs)
			.map(|(input_index, input)| {
				let mut hash_data = [
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
				]
				.concat();
				if let Some(script_path) = input.deposit_address.script_path.clone() {
					hash_data.append(
						&mut [
							&[SPENDTYPE_SCRIPT] as &[u8],
							&input_index.to_le_bytes(),
							// "Common signature message extension" according to BIP 342
							&script_path.tapleaf_hash[..],
							&[KEYVERSION],
							&CODESEPARATOR,
						]
						.concat(),
					);
				} else {
					hash_data.append(
						&mut [&[SPENDTYPE_KEY] as &[u8], &input_index.to_le_bytes()].concat(),
					);
				}
				(
					if Some(&input_index) == old_utxo_input_indices.front() {
						old_utxo_input_indices.pop_front();
						PreviousOrCurrent::Previous
					} else {
						PreviousOrCurrent::Current
					},
					sha2_256(hash_data.as_slice()),
				)
			})
			.collect()
	}
}

trait SerializeBtc {
	/// Encodes this item to a byte buffer.
	fn btc_encode_to(&self, buf: &mut Vec<u8>);
	/// The exact size this object will have once serialized.
	fn size(&self) -> usize;
	/// Returns a serialized bitcoin payload.
	fn btc_serialize(&self) -> Vec<u8> {
		let mut buf = Vec::with_capacity(self.size());
		self.btc_encode_to(&mut buf);
		buf
	}
}

impl<T: SerializeBtc> SerializeBtc for &[T] {
	fn btc_encode_to(&self, buf: &mut Vec<u8>) {
		buf.extend(to_varint(self.len() as u64));
		for t in self.iter() {
			t.btc_encode_to(buf);
		}
	}

	fn size(&self) -> usize {
		let s = self.iter().map(|t| t.size()).sum::<usize>();
		s + to_varint(s as u64).len()
	}
}

/// Subset of ops needed for Chainflip.
///
/// For reference see https://en.bitcoin.it/wiki/Script
#[derive(Encode, Decode, TypeInfo, MaxEncodedLen, Clone, RuntimeDebug, PartialEq, Eq)]
pub enum BitcoinOp {
	PushUint { value: u32 },
	PushBytes { bytes: BoundedVec<u8, ConstU32<MAX_SEGWIT_PROGRAM_BYTES>> },
	Drop,
	CheckSig,
	Dup,
	Hash160,
	EqualVerify,
	Equal,
	// Not part of the bitcoin spec, implemented for convenience
	PushArray20 { bytes: [u8; 20] },
	PushArray32 { bytes: [u8; 32] },
	PushVersion { version: u8 },
}

#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq)]
pub struct BitcoinScript {
	bytes: BoundedVec<u8, ConstU32<MAX_BITCOIN_SCRIPT_LENGTH>>,
}

impl BitcoinScript {
	pub fn new(ops: &[BitcoinOp]) -> Self {
		let mut bytes = Vec::with_capacity(ops.iter().map(|op| op.size()).sum::<usize>());
		for op in ops.iter() {
			op.btc_encode_to(&mut bytes);
		}
		Self { bytes: bytes.try_into().unwrap() }
	}

	pub fn raw(self) -> Vec<u8> {
		self.bytes.into_inner()
	}
}

impl AsRef<[u8]> for BitcoinScript {
	fn as_ref(&self) -> &[u8] {
		self.bytes.as_ref()
	}
}

impl SerializeBtc for BitcoinScript {
	fn btc_encode_to(&self, buf: &mut Vec<u8>) {
		buf.extend(to_varint(self.bytes.len() as u64));
		buf.extend(&self.bytes[..]);
	}

	fn size(&self) -> usize {
		let s = self.bytes.len();
		s + to_varint(s as u64).len()
	}
}

impl SerializeBtc for BitcoinOp {
	fn btc_encode_to(&self, buf: &mut Vec<u8>) {
		match self {
			BitcoinOp::PushUint { value } => match value {
				0 => buf.push(0),
				1..=16 => buf.push(0x50 + *value as u8),
				129 => buf.push(0x4f),
				_ => {
					let num_bytes =
						sp_std::mem::size_of::<u32>() - (value.leading_zeros() / 8) as usize;
					buf.push(num_bytes as u8);
					buf.extend(value.to_le_bytes().into_iter().take(num_bytes));
				},
			},
			BitcoinOp::PushBytes { bytes } => {
				let num_bytes = bytes.len() as u32;
				match num_bytes {
					0..=0x4b => buf.push(num_bytes as u8),
					0x4c..=0xff => {
						buf.push(0x4c);
						buf.push(num_bytes as u8);
					},
					0x100..=0xffff => {
						buf.push(0x4d);
						buf.extend(num_bytes.to_le_bytes().into_iter().take(2));
					},
					0x10000..=0xffffffff => {
						buf.push(0x4e);
						buf.extend(num_bytes.to_le_bytes().into_iter().take(4));
					},
				}
				buf.extend(bytes);
			},
			BitcoinOp::Drop => buf.push(0x75),
			BitcoinOp::CheckSig => buf.push(0xac),
			BitcoinOp::Dup => buf.push(0x76),
			BitcoinOp::Hash160 => buf.push(0xa9),
			BitcoinOp::EqualVerify => buf.push(0x88),
			BitcoinOp::Equal => buf.push(0x87),
			BitcoinOp::PushArray20 { bytes } => {
				buf.push(20u8);
				buf.extend(bytes);
			},
			BitcoinOp::PushArray32 { bytes } => {
				buf.push(32u8);
				buf.extend(bytes);
			},
			BitcoinOp::PushVersion { version } =>
				if *version == 0 {
					buf.push(0);
				} else {
					buf.push(0x50 + *version);
				},
		}
	}

	fn size(&self) -> usize {
		match self {
			BitcoinOp::PushUint { value } => match value {
				0..=16 => 1,
				_ => {
					let num_bytes =
						sp_std::mem::size_of::<u32>() - (value.leading_zeros() / 8) as usize;
					1 + num_bytes
				},
			},
			BitcoinOp::PushBytes { bytes } => {
				let num_bytes = bytes.len();
				num_bytes +
					match num_bytes {
						0..=0x4b => 1,
						0x4c..=0xff => 2,
						0x100..=0xffff => 3,
						_ => 5,
					}
			},
			BitcoinOp::Drop |
			BitcoinOp::CheckSig |
			BitcoinOp::Dup |
			BitcoinOp::Hash160 |
			BitcoinOp::EqualVerify |
			BitcoinOp::Equal => 1,
			BitcoinOp::PushArray20 { .. } => 21,
			BitcoinOp::PushArray32 { .. } => 33,
			BitcoinOp::PushVersion { .. } => 1,
		}
	}
}

pub struct BitcoinRetryPolicy;
impl RetryPolicy for BitcoinRetryPolicy {
	type BlockNumber = u32;
	type AttemptCount = u32;

	fn next_attempt_delay(retry_attempts: Self::AttemptCount) -> Option<Self::BlockNumber> {
		// 1200 State-chain blocks are 2 hours - the maximum time we want to wait between retries.
		const MAX_BROADCAST_DELAY: u32 = 1200u32;
		// 25 * 6 = 150 seconds / 2.5 minutes
		const DELAY_THRESHOLD: u32 = 25u32;

		retry_attempts.checked_sub(DELAY_THRESHOLD).map(|above_threshold| {
			sp_std::cmp::min(2u32.saturating_pow(above_threshold), MAX_BROADCAST_DELAY)
		})
	}
}

#[cfg(test)]
mod test {
	use super::*;

	#[test]
	fn test_verify_signature() {
		// test cases from https://github.com/bitcoin/bips/blob/master/bip-0340/test-vectors.csv
		assert!(verify_single_threshold_signature(
			&hex_literal::hex!("3913CC82D3CE5A22409E61D1E42E7C60435A3DDCB9192CFDCF7D67C3F520EDAB"),
			&hex_literal::hex!("461E208488056167B18085A0B5CC62464BA8D854540D1BCC7AB987AB8F64FA53"),
			&hex_literal::hex!("719B74CE347D7CDA876C39DDEAB89EE750AC24091835300FD27E7783EC336232626EEAA1500F84326F4144F453FFE5AE44D35C503B36AD68C00C3A4AB12C3CFB")));
		assert!(verify_single_threshold_signature(
			&hex_literal::hex!("F9308A019258C31049344F85F89D5229B531C845836F99B08601F113BCE036F9"),
			&hex_literal::hex!("0000000000000000000000000000000000000000000000000000000000000000"),
			&hex_literal::hex!("E907831F80848D1069A5371B402410364BDF1C5F8307B0084C55F1CE2DCA821525F66A4A85EA8B71E482A74F382D2CE5EBEEE8FDB2172F477DF4900D310536C0")));
		assert!(verify_single_threshold_signature(
			&hex_literal::hex!("DFF1D77F2A671C5F36183726DB2341BE58FEAE1DA2DECED843240F7B502BA659"),
			&hex_literal::hex!("243F6A8885A308D313198A2E03707344A4093822299F31D0082EFA98EC4E6C89"),
			&hex_literal::hex!("6896BD60EEAE296DB48A229FF71DFE071BDE413E6D43F917DC8DCF8C78DE33418906D11AC976ABCCB20B091292BFF4EA897EFCB639EA871CFA95F6DE339E4B0A")));
		assert!(verify_single_threshold_signature(
			&hex_literal::hex!("DD308AFEC5777E13121FA72B9CC1B7CC0139715309B086C960E18FD969774EB8"),
			&hex_literal::hex!("7E2D58D8B3BCDF1ABADEC7829054F90DDA9805AAB56C77333024B9D0A508B75C"),
			&hex_literal::hex!("5831AAEED7B44BB74E5EAB94BA9D4294C49BCF2A60728D8B4C200F50DD313C1BAB745879A5AD954A72C45A91C3A51D3C7ADEA98D82F8481E0E1E03674A6F3FB7")));
		assert!(verify_single_threshold_signature(
			&hex_literal::hex!("25D1DFF95105F5253C4022F628A996AD3A0D95FBF21D468A1B33F8C160D8F517"),
			&hex_literal::hex!("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF"),
			&hex_literal::hex!("7EB0509757E246F19449885651611CB965ECC1A187DD51B64FDA1EDC9637D5EC97582B9CB13DB3933705B32BA982AF5AF25FD78881EBB32771FC5922EFC66EA3")));
		assert!(verify_single_threshold_signature(
			&hex_literal::hex!("D69C3509BB99E412E68B0FE8544E72837DFA30746D8BE2AA65975F29D22DC7B9"),
			&hex_literal::hex!("4DF3C3F68FCC83B27E9D42C90431A72499F17875C81A599B566C9889B9696703"),
			&hex_literal::hex!("00000000000000000000003B78CE563F89A0ED9414F5AA28AD0D96D6795F9C6376AFB1548AF603B3EB45C9F8207DEE1060CB71C04E80F593060B07D28308D7F4")));
		assert!(!verify_single_threshold_signature(
			&hex_literal::hex!("EEFDEA4CDB677750A420FEE807EACF21EB9898AE79B9768766E4FAA04A2D4A34"),
			&hex_literal::hex!("243F6A8885A308D313198A2E03707344A4093822299F31D0082EFA98EC4E6C89"),
			&hex_literal::hex!("6CFF5C3BA86C69EA4B7376F31A9BCB4F74C1976089B2D9963DA2E5543E17776969E89B4C5564D00349106B8497785DD7D1D713A8AE82B32FA79D5F7FC407D39B")));
		assert!(!verify_single_threshold_signature(
			&hex_literal::hex!("DFF1D77F2A671C5F36183726DB2341BE58FEAE1DA2DECED843240F7B502BA659"),
			&hex_literal::hex!("243F6A8885A308D313198A2E03707344A4093822299F31D0082EFA98EC4E6C89"),
			&hex_literal::hex!("FFF97BD5755EEEA420453A14355235D382F6472F8568A18B2F057A14602975563CC27944640AC607CD107AE10923D9EF7A73C643E166BE5EBEAFA34B1AC553E2")));
		assert!(!verify_single_threshold_signature(
			&hex_literal::hex!("DFF1D77F2A671C5F36183726DB2341BE58FEAE1DA2DECED843240F7B502BA659"),
			&hex_literal::hex!("243F6A8885A308D313198A2E03707344A4093822299F31D0082EFA98EC4E6C89"),
			&hex_literal::hex!("1FA62E331EDBC21C394792D2AB1100A7B432B013DF3F6FF4F99FCB33E0E1515F28890B3EDB6E7189B630448B515CE4F8622A954CFE545735AAEA5134FCCDB2BD")));
		assert!(!verify_single_threshold_signature(
			&hex_literal::hex!("DFF1D77F2A671C5F36183726DB2341BE58FEAE1DA2DECED843240F7B502BA659"),
			&hex_literal::hex!("243F6A8885A308D313198A2E03707344A4093822299F31D0082EFA98EC4E6C89"),
			&hex_literal::hex!("6CFF5C3BA86C69EA4B7376F31A9BCB4F74C1976089B2D9963DA2E5543E177769961764B3AA9B2FFCB6EF947B6887A226E8D7C93E00C5ED0C1834FF0D0C2E6DA6")));
		assert!(!verify_single_threshold_signature(
			&hex_literal::hex!("DFF1D77F2A671C5F36183726DB2341BE58FEAE1DA2DECED843240F7B502BA659"),
			&hex_literal::hex!("243F6A8885A308D313198A2E03707344A4093822299F31D0082EFA98EC4E6C89"),
			&hex_literal::hex!("0000000000000000000000000000000000000000000000000000000000000000123DDA8328AF9C23A94C1FEECFD123BA4FB73476F0D594DCB65C6425BD186051")));
		assert!(!verify_single_threshold_signature(
			&hex_literal::hex!("DFF1D77F2A671C5F36183726DB2341BE58FEAE1DA2DECED843240F7B502BA659"),
			&hex_literal::hex!("243F6A8885A308D313198A2E03707344A4093822299F31D0082EFA98EC4E6C89"),
			&hex_literal::hex!("00000000000000000000000000000000000000000000000000000000000000017615FBAF5AE28864013C099742DEADB4DBA87F11AC6754F93780D5A1837CF197")));
		assert!(!verify_single_threshold_signature(
			&hex_literal::hex!("DFF1D77F2A671C5F36183726DB2341BE58FEAE1DA2DECED843240F7B502BA659"),
			&hex_literal::hex!("243F6A8885A308D313198A2E03707344A4093822299F31D0082EFA98EC4E6C89"),
			&hex_literal::hex!("4A298DACAE57395A15D0795DDBFD1DCB564DA82B0F269BC70A74F8220429BA1D69E89B4C5564D00349106B8497785DD7D1D713A8AE82B32FA79D5F7FC407D39B")));
		assert!(!verify_single_threshold_signature(
			&hex_literal::hex!("DFF1D77F2A671C5F36183726DB2341BE58FEAE1DA2DECED843240F7B502BA659"),
			&hex_literal::hex!("243F6A8885A308D313198A2E03707344A4093822299F31D0082EFA98EC4E6C89"),
			&hex_literal::hex!("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEFFFFFC2F69E89B4C5564D00349106B8497785DD7D1D713A8AE82B32FA79D5F7FC407D39B")));
		assert!(!verify_single_threshold_signature(
			&hex_literal::hex!("DFF1D77F2A671C5F36183726DB2341BE58FEAE1DA2DECED843240F7B502BA659"),
			&hex_literal::hex!("243F6A8885A308D313198A2E03707344A4093822299F31D0082EFA98EC4E6C89"),
			&hex_literal::hex!("6CFF5C3BA86C69EA4B7376F31A9BCB4F74C1976089B2D9963DA2E5543E177769FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141")));
		assert!(!verify_single_threshold_signature(
			&hex_literal::hex!("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEFFFFFC30"),
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
			("1AKDDsfTh8uY4X3ppy1m7jw1fVMBSMkzjP", BitcoinNetwork::Mainnet, &hex_literal::hex!("76a914662AD25DB00E7BB38BC04831AE48B4B446D1269888ac")[..]),
			("34nSkinWC9rDDJiUY438qQN1JHmGqBHGW7", BitcoinNetwork::Mainnet, &hex_literal::hex!("a91421EF2F4B1EA1F9ED09C1128D1EBB61D4729CA7D687")[..]),
		];

		for (valid_address, intended_btc_net, expected_scriptpubkey) in valid_addresses {
			let pk = ScriptPubkey::try_from_address(valid_address, &intended_btc_net)
				.unwrap_or_else(|_| panic!("Failed to parse address: {valid_address}"));
			assert_eq!(pk.bytes(), expected_scriptpubkey, "Input was {valid_address} / {pk:?}");
			assert_eq!(
				pk.to_address(&intended_btc_net).to_uppercase(),
				valid_address.to_uppercase()
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
				ScriptPubkey::try_from_address(invalid_address, &intended_btc_net,),
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
				ScriptPubkey::try_from_address(address, &BitcoinNetwork::Mainnet,).is_ok(),
				validity
			);
		}
	}

	fn create_test_unsigned_transaction(sign_with: PreviousOrCurrent) -> BitcoinTransaction {
		let pubkey_x =
			hex_literal::hex!("78C79A2B436DA5575A03CDE40197775C656FFF9F0F59FC1466E09C20A81A9CDB");
		let script_pubkey = ScriptPubkey::try_from_address(
			"bc1pgtj0f3u2rk8ex6khlskz7q50nwc48r8unfgfhxzsx9zhcdnczhqq60lzjt",
			&BitcoinNetwork::Mainnet,
		)
		.unwrap();
		let input = Utxo {
			amount: 100010000,
			id: UtxoId {
				tx_id: hex_literal::hex!(
					"4C94E48A870B85F41228D33CF25213DFCC8DD796E7211ED6B1F9A014809DBBB5"
				),
				vout: 1,
			},
			deposit_address: DepositAddress::new(pubkey_x, 123),
		};
		let agg_key = match sign_with {
			PreviousOrCurrent::Previous => AggKey { previous: Some(pubkey_x), current: [0xcf; 32] },
			PreviousOrCurrent::Current => AggKey { previous: None, current: pubkey_x },
		};
		let output = BitcoinOutput { amount: 100000000, script_pubkey };
		BitcoinTransaction::create_new_unsigned(&agg_key, vec![input], vec![output])
	}

	#[test]
	fn test_finalize() {
		let mut tx = create_test_unsigned_transaction(PreviousOrCurrent::Current);
		tx.add_signatures(vec![[0u8; 64]]);
		assert_eq!(tx.finalize(), hex_literal::hex!("020000000001014C94E48A870B85F41228D33CF25213DFCC8DD796E7211ED6B1F9A014809DBBB50100000000FDFFFFFF0100E1F5050000000022512042E4F4C78A1D8F936AD7FC2C2F028F9BB1538CFC9A509B985031457C367815C003400000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000025017B752078C79A2B436DA5575A03CDE40197775C656FFF9F0F59FC1466E09C20A81A9CDBAC21C0EEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEE00000000"));
	}

	#[test]
	fn test_payloads() {
		test_payload(PreviousOrCurrent::Previous);
		test_payload(PreviousOrCurrent::Current);
	}

	fn test_payload(sign_with: PreviousOrCurrent) {
		let tx = create_test_unsigned_transaction(sign_with);
		assert_eq!(
			tx.get_signing_payloads(),
			vec![(
				sign_with,
				hex_literal::hex!(
					"E16117C6CD69142E41736CE2882F0E697FF4369A2CBCEE9D92FC0346C6774FB4"
				)
			)],
			"Failed signing with {sign_with:?}",
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
			(128, vec![1, 128]),
			(129, vec![79]),
			(130, vec![1, 130]),
			(255, vec![1, 255]),
			(256, vec![2, 0, 1]),
			(11394560, vec![3, 0, 0xDE, 0xAD]),
			(u32::MAX, vec![4, 255, 255, 255, 255]),
		];
		for (value, encoded) in test_data {
			let mut buf = Vec::new();
			BitcoinOp::PushUint { value }.btc_encode_to(&mut buf);
			assert_eq!(buf, encoded);
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

	#[test]
	fn test_btc_network_names() {
		assert_eq!(
			BitcoinNetwork::try_from(BitcoinNetwork::Mainnet.to_string().as_str()).unwrap(),
			BitcoinNetwork::Mainnet
		);
		assert_eq!(
			BitcoinNetwork::try_from(BitcoinNetwork::Testnet.to_string().as_str()).unwrap(),
			BitcoinNetwork::Testnet
		);
		assert_eq!(
			BitcoinNetwork::try_from(BitcoinNetwork::Regtest.to_string().as_str()).unwrap(),
			BitcoinNetwork::Regtest
		);
	}

	#[test]
	fn consolidation_parameters() {
		// These are expected to be valid:
		assert!(ConsolidationParameters::new(2, 2).are_valid());
		assert!(ConsolidationParameters::new(10, 2).are_valid());
		assert!(ConsolidationParameters::new(10, 10).are_valid());
		assert!(ConsolidationParameters::new(200, 100).are_valid());

		// Invalid: size < threshold
		assert!(!ConsolidationParameters::new(9, 10).are_valid());
		// Invalid: size is too small
		assert!(!ConsolidationParameters::new(0, 0).are_valid());
		assert!(!ConsolidationParameters::new(1, 1).are_valid());
		assert!(!ConsolidationParameters::new(0, 10).are_valid());
	}

	#[test]
	fn retry_delay_ramps_up() {
		assert_eq!(BitcoinRetryPolicy::next_attempt_delay(0), None);
		assert_eq!(BitcoinRetryPolicy::next_attempt_delay(1), None);
		assert_eq!(BitcoinRetryPolicy::next_attempt_delay(24), None);
		assert_eq!(BitcoinRetryPolicy::next_attempt_delay(25), Some(1));
		assert_eq!(BitcoinRetryPolicy::next_attempt_delay(26), Some(2));
		assert_eq!(BitcoinRetryPolicy::next_attempt_delay(27), Some(4));
		assert_eq!(BitcoinRetryPolicy::next_attempt_delay(28), Some(8));
		assert_eq!(BitcoinRetryPolicy::next_attempt_delay(29), Some(16));
		assert_eq!(BitcoinRetryPolicy::next_attempt_delay(30), Some(32));
		assert_eq!(BitcoinRetryPolicy::next_attempt_delay(40), Some(1200));
		assert_eq!(BitcoinRetryPolicy::next_attempt_delay(150), Some(1200));
	}
}
