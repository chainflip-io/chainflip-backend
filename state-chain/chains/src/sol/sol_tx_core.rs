use core::str::FromStr;

use borsh::{BorshDeserialize, BorshSerialize};
use codec::{Decode, Encode};

use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_std::{collections::btree_map::BTreeMap, vec, vec::Vec};

#[cfg(test)]
use crate::sol::sol_tx_core::{
	signer::{signers::Signers, SignerError},
	transaction::TransactionError,
};
use crate::sol::{SolAddress, SolHash, SolSignature};
use sol_prim::consts::BPF_LOADER_UPGRADEABLE_ID;

pub mod address_derivation;
pub mod bpf_loader_instructions;
pub mod compute_budget;
pub mod program;
pub mod program_instructions;
pub mod short_vec;
#[cfg(feature = "std")]
pub mod signer;
pub mod token_instructions;
pub mod transaction;

#[cfg(test)]
use thiserror::Error;

pub const HASH_BYTES: usize = 32;

/// Maximum string length of a base58 encoded pubkey
const MAX_BASE58_LEN: usize = 44;

/// An atomically-committed sequence of instructions.
///
/// While [`Instruction`]s are the basic unit of computation in Solana,
/// they are submitted by clients in [`Transaction`]s containing one or
/// more instructions, and signed by one or more [`Signer`]s.
///
/// [`Signer`]: crate::signer::Signer
///
/// See the [module documentation] for more details about transactions.
///
/// [module documentation]: self
///
/// Some constructors accept an optional `payer`, the account responsible for
/// paying the cost of executing a transaction. In most cases, callers should
/// specify the payer explicitly in these constructors. In some cases though,
/// the caller is not _required_ to specify the payer, but is still allowed to:
/// in the [`Message`] structure, the first account is always the fee-payer, so
/// if the caller has knowledge that the first account of the constructed
/// transaction's `Message` is both a signer and the expected fee-payer, then
/// redundantly specifying the fee-payer is not strictly required.
#[derive(
	Encode, Decode, TypeInfo, Debug, PartialEq, Default, Eq, Clone, Serialize, Deserialize,
)]
pub struct Transaction {
	/// A set of signatures of a serialized [`Message`], signed by the first
	/// keys of the `Message`'s [`account_keys`], where the number of signatures
	/// is equal to [`num_required_signatures`] of the `Message`'s
	/// [`MessageHeader`].
	///
	/// [`account_keys`]: Message::account_keys
	/// [`MessageHeader`]: crate::message::MessageHeader
	/// [`num_required_signatures`]: crate::message::MessageHeader::num_required_signatures
	// NOTE: Serialization-related changes must be paired with the direct read at sigverify.
	#[serde(with = "short_vec")]
	pub signatures: Vec<SolSignature>,

	/// The message to sign.
	pub message: Message,
}

impl Transaction {
	pub fn new_unsigned(message: Message) -> Self {
		Self {
			signatures: vec![
				SolSignature::default();
				message.header.num_required_signatures as usize
			],
			message,
		}
	}

	#[cfg(test)]
	pub fn new_with_payer(instructions: &[Instruction], payer: Option<&Pubkey>) -> Self {
		let message = Message::new(instructions, payer);
		Self::new_unsigned(message)
	}

	#[cfg(test)]
	pub fn sign<T: Signers + ?Sized>(&mut self, keypairs: &T, recent_blockhash: Hash) {
		if let Err(e) = self.try_sign(keypairs, recent_blockhash) {
			panic!("Transaction::sign failed with error {e:?}");
		}
	}

	#[cfg(test)]
	pub fn try_sign<T: Signers + ?Sized>(
		&mut self,
		keypairs: &T,
		recent_blockhash: Hash,
	) -> Result<(), SignerError> {
		self.try_partial_sign(keypairs, recent_blockhash)?;

		if !self.is_signed() {
			Err(SignerError::NotEnoughSigners)
		} else {
			Ok(())
		}
	}

	#[cfg(test)]
	pub fn try_partial_sign<T: Signers + ?Sized>(
		&mut self,
		keypairs: &T,
		recent_blockhash: Hash,
	) -> Result<(), SignerError> {
		let positions = self.get_signing_keypair_positions(&keypairs.pubkeys())?;
		if positions.iter().any(|pos| pos.is_none()) {
			return Err(SignerError::KeypairPubkeyMismatch)
		}
		let positions: Vec<usize> = positions.iter().map(|pos| pos.unwrap()).collect();
		self.try_partial_sign_unchecked(keypairs, positions, recent_blockhash)
	}

	#[cfg(test)]
	pub fn try_partial_sign_unchecked<T: Signers + ?Sized>(
		&mut self,
		keypairs: &T,
		positions: Vec<usize>,
		recent_blockhash: Hash,
	) -> Result<(), SignerError> {
		// if you change the blockhash, you're re-signing...
		if recent_blockhash != self.message.recent_blockhash {
			self.message.recent_blockhash = recent_blockhash;
			self.signatures
				.iter_mut()
				.for_each(|signature| *signature = SolSignature::default());
		}

		let signatures = keypairs.try_sign_message(&self.message_data())?;
		for i in 0..positions.len() {
			self.signatures[positions[i]] = signatures[i];
		}
		Ok(())
	}

	#[cfg(test)]
	pub fn get_signing_keypair_positions(
		&self,
		pubkeys: &[Pubkey],
	) -> Result<Vec<Option<usize>>, TransactionError> {
		if self.message.account_keys.len() < self.message.header.num_required_signatures as usize {
			return Err(TransactionError::InvalidAccountIndex)
		}
		let signed_keys =
			&self.message.account_keys[0..self.message.header.num_required_signatures as usize];

		Ok(pubkeys
			.iter()
			.map(|pubkey| signed_keys.iter().position(|x| x == pubkey))
			.collect())
	}

	pub fn is_signed(&self) -> bool {
		self.signatures.iter().all(|signature| *signature != SolSignature::default())
	}

	/// Return the message containing all data that should be signed.
	pub fn message(&self) -> &Message {
		&self.message
	}

	/// Return the serialized message data to sign.
	pub fn message_data(&self) -> Vec<u8> {
		self.message().serialize()
	}

	/// Due to different Serialization between SolSignature and Solana native Signature type, the
	/// SolSignatures needs to be converted into the RawSignature type before the transaction is
	/// serialized as whole.
	pub fn finalize_and_serialize(self) -> Result<Vec<u8>, bincode::error::EncodeError> {
		bincode::serde::encode_to_vec(RawTransaction::from(self), bincode::config::legacy())
	}
}

/// Internal raw transaction type used for correct Serialization and Encoding
#[derive(Debug, PartialEq, Default, Eq, Clone, Serialize, Deserialize)]
struct RawTransaction {
	#[serde(with = "short_vec")]
	pub signatures: Vec<RawSignature>,
	pub message: Message,
}

impl From<Transaction> for RawTransaction {
	fn from(from: Transaction) -> Self {
		Self {
			signatures: from.signatures.into_iter().map(RawSignature::from).collect(),
			message: from.message,
		}
	}
}

/// A directive for a single invocation of a Solana program.
///
/// An instruction specifies which program it is calling, which accounts it may
/// read or modify, and additional data that serves as input to the program. One
/// or more instructions are included in transactions submitted by Solana
/// clients. Instructions are also used to describe [cross-program
/// invocations][cpi].
///
/// [cpi]: https://docs.solana.com/developing/programming-model/calling-between-programs
///
/// During execution, a program will receive a list of account data as one of
/// its arguments, in the same order as specified during `Instruction`
/// construction.
///
/// While Solana is agnostic to the format of the instruction data, it has
/// built-in support for serialization via [`borsh`] and [`bincode`].
///
/// [`borsh`]: https://docs.rs/borsh/latest/borsh/
/// [`bincode`]: https://docs.rs/bincode/latest/bincode/
///
/// # Specifying account metadata
///
/// When constructing an [`Instruction`], a list of all accounts that may be
/// read or written during the execution of that instruction must be supplied as
/// [`AccountMeta`] values.
///
/// Any account whose data may be mutated by the program during execution must
/// be specified as writable. During execution, writing to an account that was
/// not specified as writable will cause the transaction to fail. Writing to an
/// account that is not owned by the program will cause the transaction to fail.
///
/// Any account whose lamport balance may be mutated by the program during
/// execution must be specified as writable. During execution, mutating the
/// lamports of an account that was not specified as writable will cause the
/// transaction to fail. While _subtracting_ lamports from an account not owned
/// by the program will cause the transaction to fail, _adding_ lamports to any
/// account is allowed, as long is it is mutable.
///
/// Accounts that are not read or written by the program may still be specified
/// in an `Instruction`'s account list. These will affect scheduling of program
/// execution by the runtime, but will otherwise be ignored.
///
/// When building a transaction, the Solana runtime coalesces all accounts used
/// by all instructions in that transaction, along with accounts and permissions
/// required by the runtime, into a single account list. Some accounts and
/// account permissions required by the runtime to process a transaction are
/// _not_ required to be included in an `Instruction`s account list. These
/// include:
///
/// - The program ID &mdash; it is a separate field of `Instruction`
/// - The transaction's fee-paying account &mdash; it is added during [`Message`] construction. A
///   program may still require the fee payer as part of the account list if it directly references
///   it.
///
/// [`Message`]: crate::message::Message
///
/// Programs may require signatures from some accounts, in which case they
/// should be specified as signers during `Instruction` construction. The
/// program must still validate during execution that the account is a signer.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct Instruction {
	/// Pubkey of the program that executes this instruction.
	pub program_id: Pubkey,
	/// Metadata describing accounts that should be passed to the program.
	pub accounts: Vec<AccountMeta>,
	/// Opaque data passed to the program for its own interpretation.
	pub data: Vec<u8>,
}

impl Instruction {
	pub fn new_with_borsh<T: BorshSerialize>(
		program_id: Pubkey,
		data: &T,
		accounts: Vec<AccountMeta>,
	) -> Self {
		let data = borsh::to_vec(data).unwrap();
		Self { program_id, accounts, data }
	}

	pub fn new_with_bincode<T: Serialize>(
		program_id: Pubkey,
		data: &T,
		accounts: Vec<AccountMeta>,
	) -> Self {
		// NOTE: the solana-sdk uses bincode version 1.3.3 which has a dependency on serde which
		// depends on std and so it cannot be used with our runtime. Fortunately, the new version of
		// bincode (bincode 2) has an optional dependency on serde and we can use the serializer
		// without serde. However, bincode 2 is a complete rewrite of bincode 1 and so to mimic the
		// exact behaviour of serialization that is used by the solana-sdk with bincode 1, we need
		// to use the legacy config for serialization according to the migration guide provided by
		// bincode here: https://github.com/bincode-org/bincode/blob/v2.0.0-rc.3/docs/migration_guide.md.
		// Original serialization call in solana sdk:
		// let data = bincode::serialize(data).unwrap();
		let data = bincode::serde::encode_to_vec(data, bincode::config::legacy()).unwrap();
		Self { program_id, accounts, data }
	}
}

/// Describes a single account read or written by a program during instruction
/// execution.
///
/// When constructing an [`Instruction`], a list of all accounts that may be
/// read or written during the execution of that instruction must be supplied.
/// Any account that may be mutated by the program during execution, either its
/// data or metadata such as held lamports, must be writable.
///
/// Note that because the Solana runtime schedules parallel transaction
/// execution around which accounts are writable, care should be taken that only
/// accounts which actually may be mutated are specified as writable. As the
/// default [`AccountMeta::new`] constructor creates writable accounts, this is
/// a minor hazard: use [`AccountMeta::new_readonly`] to specify that an account
/// is not writable.
#[repr(C)]
#[derive(Debug, Default, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct AccountMeta {
	/// An account's public key.
	pub pubkey: Pubkey,
	/// True if an `Instruction` requires a `Transaction` signature matching `pubkey`.
	pub is_signer: bool,
	/// True if the account data or metadata may be mutated during program execution.
	pub is_writable: bool,
}

impl AccountMeta {
	pub fn new(pubkey: Pubkey, is_signer: bool) -> Self {
		Self { pubkey, is_signer, is_writable: true }
	}

	pub fn new_readonly(pubkey: Pubkey, is_signer: bool) -> Self {
		Self { pubkey, is_signer, is_writable: false }
	}
}

impl<P: Into<Pubkey>> From<P> for AccountMeta {
	fn from(pubkey: P) -> Self {
		AccountMeta { pubkey: pubkey.into(), is_signer: false, is_writable: false }
	}
}

/// Describes the organization of a `Message`'s account keys.
///
/// Every [`Instruction`] specifies which accounts it may reference, or
/// otherwise requires specific permissions of. Those specifications are:
/// whether the account is read-only, or read-write; and whether the account
/// must have signed the transaction containing the instruction.
///
/// Whereas individual `Instruction`s contain a list of all accounts they may
/// access, along with their required permissions, a `Message` contains a
/// single shared flat list of _all_ accounts required by _all_ instructions in
/// a transaction. When building a `Message`, this flat list is created and
/// `Instruction`s are converted to [`CompiledInstruction`]s. Those
/// `CompiledInstruction`s then reference by index the accounts they require in
/// the single shared account list.
///
/// [`Instruction`]: crate::instruction::Instruction
/// [`CompiledInstruction`]: crate::instruction::CompiledInstruction
///
/// The shared account list is ordered by the permissions required of the accounts:
///
/// - accounts that are writable and signers
/// - accounts that are read-only and signers
/// - accounts that are writable and not signers
/// - accounts that are read-only and not signers
///
/// Given this ordering, the fields of `MessageHeader` describe which accounts
/// in a transaction require which permissions.
///
/// When multiple transactions access the same read-only accounts, the runtime
/// may process them in parallel, in a single [PoH] entry. Transactions that
/// access the same read-write accounts are processed sequentially.
///
/// [PoH]: https://docs.solana.com/cluster/synchronization
#[derive(
	Encode, Decode, TypeInfo, Serialize, Deserialize, Default, Debug, PartialEq, Eq, Clone, Copy,
)]
#[serde(rename_all = "camelCase")]
pub struct MessageHeader {
	/// The number of signatures required for this message to be considered
	/// valid. The signers of those signatures must match the first
	/// `num_required_signatures` of [`Message::account_keys`].
	// NOTE: Serialization-related changes must be paired with the direct read at sigverify.
	pub num_required_signatures: u8,

	/// The last `num_readonly_signed_accounts` of the signed keys are read-only
	/// accounts.
	pub num_readonly_signed_accounts: u8,

	/// The last `num_readonly_unsigned_accounts` of the unsigned keys are
	/// read-only accounts.
	pub num_readonly_unsigned_accounts: u8,
}

/// A Solana transaction message (legacy).
///
/// See the [`message`] module documentation for further description.
///
/// [`message`]: crate::message
///
/// Some constructors accept an optional `payer`, the account responsible for
/// paying the cost of executing a transaction. In most cases, callers should
/// specify the payer explicitly in these constructors. In some cases though,
/// the caller is not _required_ to specify the payer, but is still allowed to:
/// in the `Message` structure, the first account is always the fee-payer, so if
/// the caller has knowledge that the first account of the constructed
/// transaction's `Message` is both a signer and the expected fee-payer, then
/// redundantly specifying the fee-payer is not strictly required.
// NOTE: Serialization-related changes must be paired with the custom serialization
// for versioned messages in the `RemainingLegacyMessage` struct.
#[derive(
	Encode, Decode, TypeInfo, Serialize, Deserialize, Default, Debug, PartialEq, Eq, Clone,
)]
#[serde(rename_all = "camelCase")]
pub struct Message {
	/// The message header, identifying signed and read-only `account_keys`.
	// NOTE: Serialization-related changes must be paired with the direct read at sigverify.
	pub header: MessageHeader,

	/// All the account keys used by this transaction.
	#[serde(with = "short_vec")]
	pub account_keys: Vec<Pubkey>,

	/// The id of a recent ledger entry.
	pub recent_blockhash: Hash,

	/// Programs that will be executed in sequence and committed in one atomic transaction if all
	/// succeed.
	#[serde(with = "short_vec")]
	pub instructions: Vec<CompiledInstruction>,
}

impl Message {
	pub fn new_with_blockhash(
		instructions: &[Instruction],
		payer: Option<&Pubkey>,
		blockhash: &Hash,
	) -> Self {
		let compiled_keys = CompiledKeys::compile(instructions, payer.cloned());
		let (header, account_keys) = compiled_keys
			.try_into_message_components()
			.expect("overflow when compiling message keys");
		let instructions = compile_instructions(instructions, &account_keys);
		Self::new_with_compiled_instructions(
			header.num_required_signatures,
			header.num_readonly_signed_accounts,
			header.num_readonly_unsigned_accounts,
			account_keys,
			*blockhash,
			instructions,
		)
	}

	pub fn new(instructions: &[Instruction], payer: Option<&Pubkey>) -> Self {
		Self::new_with_blockhash(instructions, payer, &Hash::default())
	}

	#[cfg(test)]
	pub fn new_with_nonce(
		mut instructions: Vec<Instruction>,
		payer: Option<&Pubkey>,
		nonce_account_pubkey: &Pubkey,
		nonce_authority_pubkey: &Pubkey,
	) -> Self {
		let nonce_ix = program_instructions::SystemProgramInstruction::advance_nonce_account(
			nonce_account_pubkey,
			nonce_authority_pubkey,
		);
		instructions.insert(0, nonce_ix);
		Self::new(&instructions, payer)
	}

	fn new_with_compiled_instructions(
		num_required_signatures: u8,
		num_readonly_signed_accounts: u8,
		num_readonly_unsigned_accounts: u8,
		account_keys: Vec<Pubkey>,
		recent_blockhash: Hash,
		instructions: Vec<CompiledInstruction>,
	) -> Self {
		Self {
			header: MessageHeader {
				num_required_signatures,
				num_readonly_signed_accounts,
				num_readonly_unsigned_accounts,
			},
			account_keys,
			recent_blockhash,
			instructions,
		}
	}

	pub fn serialize(&self) -> Vec<u8> {
		bincode::serde::encode_to_vec(self, bincode::config::legacy()).unwrap()
	}
}

#[derive(PartialEq, Debug, Eq, Clone)]
pub enum CompileError {
	// account index overflowed during compilation
	AccountIndexOverflow,
	// address lookup table index overflowed during compilation
	AddressTableLookupIndexOverflow,
	// encountered unknown account key `{0}` during instruction compilation
	UnknownInstructionKey(Pubkey),
}

/// A helper struct to collect pubkeys compiled for a set of instructions
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub(crate) struct CompiledKeys {
	payer: Option<Pubkey>,
	key_meta_map: BTreeMap<Pubkey, CompiledKeyMeta>,
}

impl CompiledKeys {
	/// Compiles the pubkeys referenced by a list of instructions and organizes by
	/// signer/non-signer and writable/readonly.
	pub(crate) fn compile(instructions: &[Instruction], payer: Option<Pubkey>) -> Self {
		let mut key_meta_map = BTreeMap::<Pubkey, CompiledKeyMeta>::new();
		for ix in instructions {
			let meta = key_meta_map.entry(ix.program_id).or_default();
			meta.is_invoked = true;
			for account_meta in &ix.accounts {
				let meta = key_meta_map.entry(account_meta.pubkey).or_default();
				meta.is_signer |= account_meta.is_signer;
				meta.is_writable |= account_meta.is_writable;
			}
		}
		if let Some(payer) = &payer {
			let meta = key_meta_map.entry(*payer).or_default();
			meta.is_signer = true;
			meta.is_writable = true;
		}
		Self { payer, key_meta_map }
	}

	pub(crate) fn try_into_message_components(
		self,
	) -> Result<(MessageHeader, Vec<Pubkey>), CompileError> {
		let try_into_u8 = |num: usize| -> Result<u8, CompileError> {
			u8::try_from(num).map_err(|_| CompileError::AccountIndexOverflow)
		};

		let Self { payer, mut key_meta_map } = self;

		if let Some(payer) = &payer {
			key_meta_map.remove_entry(payer);
		}

		let writable_signer_keys: Vec<Pubkey> = payer
			.into_iter()
			.chain(
				key_meta_map
					.iter()
					.filter_map(|(key, meta)| (meta.is_signer && meta.is_writable).then_some(*key)),
			)
			.collect();
		let readonly_signer_keys: Vec<Pubkey> = key_meta_map
			.iter()
			.filter_map(|(key, meta)| (meta.is_signer && !meta.is_writable).then_some(*key))
			.collect();
		let writable_non_signer_keys: Vec<Pubkey> = key_meta_map
			.iter()
			.filter_map(|(key, meta)| (!meta.is_signer && meta.is_writable).then_some(*key))
			.collect();
		let readonly_non_signer_keys: Vec<Pubkey> = key_meta_map
			.iter()
			.filter_map(|(key, meta)| (!meta.is_signer && !meta.is_writable).then_some(*key))
			.collect();

		let signers_len = writable_signer_keys.len().saturating_add(readonly_signer_keys.len());

		let header = MessageHeader {
			num_required_signatures: try_into_u8(signers_len)?,
			num_readonly_signed_accounts: try_into_u8(readonly_signer_keys.len())?,
			num_readonly_unsigned_accounts: try_into_u8(readonly_non_signer_keys.len())?,
		};

		let static_account_keys = sp_std::iter::empty()
			.chain(writable_signer_keys)
			.chain(readonly_signer_keys)
			.chain(writable_non_signer_keys)
			.chain(readonly_non_signer_keys)
			.collect();

		Ok((header, static_account_keys))
	}
}

#[derive(Default, Debug, Clone, PartialEq, Eq)]
struct CompiledKeyMeta {
	is_signer: bool,
	is_writable: bool,
	is_invoked: bool,
}

fn position(keys: &[Pubkey], key: &Pubkey) -> u8 {
	keys.iter().position(|k| k == key).unwrap() as u8
}

fn compile_instruction(ix: &Instruction, keys: &[Pubkey]) -> CompiledInstruction {
	let accounts: Vec<_> = ix
		.accounts
		.iter()
		.map(|account_meta| position(keys, &account_meta.pubkey))
		.collect();

	CompiledInstruction {
		program_id_index: position(keys, &ix.program_id),
		data: ix.data.clone(),
		accounts,
	}
}

fn compile_instructions(ixs: &[Instruction], keys: &[Pubkey]) -> Vec<CompiledInstruction> {
	ixs.iter().map(|ix| compile_instruction(ix, keys)).collect()
}

/// A compact encoding of an instruction.
///
/// A `CompiledInstruction` is a component of a multi-instruction [`Message`],
/// which is the core of a Solana transaction. It is created during the
/// construction of `Message`. Most users will not interact with it directly.
///
/// [`Message`]: crate::message::Message
#[derive(Encode, Decode, TypeInfo, Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CompiledInstruction {
	/// Index into the transaction keys array indicating the program account that executes this
	/// instruction.
	pub program_id_index: u8,
	/// Ordered indices into the transaction keys array indicating which accounts to pass to the
	/// program.
	#[serde(with = "short_vec")]
	pub accounts: Vec<u8>,
	/// The program input data.
	#[serde(with = "short_vec")]
	pub data: Vec<u8>,
}

#[derive(
	Encode,
	Decode,
	TypeInfo,
	Debug,
	PartialEq,
	Default,
	Eq,
	Clone,
	Serialize,
	Deserialize,
	Ord,
	PartialOrd,
	Copy,
	BorshSerialize,
	BorshDeserialize,
)]
pub struct Pubkey(pub [u8; 32]);

#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
pub enum ParsePubkeyError {
	// String is the wrong size
	WrongSize,
	// Invalid Base58 string
	Invalid,
}

impl From<[u8; 32]> for Pubkey {
	fn from(from: [u8; 32]) -> Self {
		Self(from)
	}
}

impl From<SolAddress> for Pubkey {
	fn from(from: SolAddress) -> Self {
		Self(from.0)
	}
}
impl From<Pubkey> for SolAddress {
	fn from(from: Pubkey) -> SolAddress {
		SolAddress::from(from.0)
	}
}

impl FromStr for Pubkey {
	type Err = ParsePubkeyError;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		if s.len() > MAX_BASE58_LEN {
			return Err(ParsePubkeyError::WrongSize)
		}
		let pubkey_vec = bs58::decode(s).into_vec().map_err(|_| ParsePubkeyError::Invalid)?;
		if pubkey_vec.len() != sp_std::mem::size_of::<Pubkey>() {
			Err(ParsePubkeyError::WrongSize)
		} else {
			Pubkey::try_from(pubkey_vec).map_err(|_| ParsePubkeyError::Invalid)
		}
	}
}

impl TryFrom<Vec<u8>> for Pubkey {
	type Error = Vec<u8>;
	fn try_from(pubkey: Vec<u8>) -> Result<Self, Self::Error> {
		<[u8; 32]>::try_from(pubkey).map(Self::from)
	}
}

#[cfg(test)]
use ed25519_dalek;
use generic_array::{typenum::U64, GenericArray};
#[derive(Debug, PartialEq, Default, Eq, Clone, Serialize, Deserialize, Copy)]
pub struct RawSignature(GenericArray<u8, U64>);
const SIGNATURE_BYTES: usize = 64;
impl RawSignature {
	#[cfg(test)]
	pub(self) fn verify_verbose(
		&self,
		pubkey_bytes: &[u8],
		message_bytes: &[u8],
	) -> Result<(), ed25519_dalek::SignatureError> {
		let public_key = ed25519_dalek::VerifyingKey::try_from(pubkey_bytes)?;
		let signature = self.0.as_slice().try_into()?;
		public_key.verify_strict(message_bytes, &signature)
	}

	#[cfg(test)]
	pub fn verify(&self, pubkey_bytes: &[u8], message_bytes: &[u8]) -> bool {
		self.verify_verbose(pubkey_bytes, message_bytes).is_ok()
	}
}

impl From<[u8; SIGNATURE_BYTES]> for RawSignature {
	fn from(signature: [u8; SIGNATURE_BYTES]) -> Self {
		Self(GenericArray::from(signature))
	}
}

impl From<SolSignature> for RawSignature {
	fn from(from: SolSignature) -> Self {
		Self::from(from.0)
	}
}

#[derive(
	Encode,
	Decode,
	TypeInfo,
	Serialize,
	Deserialize,
	BorshSerialize,
	BorshDeserialize,
	Debug,
	Clone,
	Copy,
	Default,
	Eq,
	PartialEq,
	Ord,
	PartialOrd,
	Hash,
)]
pub struct Hash(pub [u8; HASH_BYTES]);
impl Hash {
	pub fn new(hash_slice: &[u8]) -> Self {
		Hash(<[u8; HASH_BYTES]>::try_from(hash_slice).unwrap())
	}
}
impl From<[u8; HASH_BYTES]> for Hash {
	fn from(from: [u8; HASH_BYTES]) -> Self {
		Self(from)
	}
}
impl From<SolHash> for Hash {
	fn from(from: SolHash) -> Self {
		Self::from(from.0)
	}
}
impl From<Hash> for SolHash {
	fn from(from: Hash) -> SolHash {
		SolHash::from(from.0)
	}
}

#[derive(Encode, Decode, TypeInfo, Serialize, Deserialize, Debug, Copy, Clone, PartialEq, Eq)]
pub struct CcmAddress {
	pub pubkey: Pubkey,
	pub is_writable: bool,
}

impl From<CcmAddress> for AccountMeta {
	fn from(from: CcmAddress) -> Self {
		match from.is_writable {
			true => AccountMeta::new(from.pubkey, false),
			false => AccountMeta::new_readonly(from.pubkey, false),
		}
	}
}

#[derive(Encode, Decode, TypeInfo, Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct CcmAccounts {
	pub cf_receiver: CcmAddress,
	pub remaining_accounts: Vec<CcmAddress>,
	pub fallback_address: Pubkey,
}

impl CcmAccounts {
	pub fn remaining_account_metas(self) -> Vec<AccountMeta> {
		self.remaining_accounts.into_iter().map(|acc| acc.into()).collect::<Vec<_>>()
	}
}

#[test]
fn ccm_extra_accounts_encoding() {
	let extra_accounts = CcmAccounts {
		cf_receiver: CcmAddress { pubkey: Pubkey([0x11; 32]), is_writable: false },
		remaining_accounts: vec![
			CcmAddress { pubkey: Pubkey([0x22; 32]), is_writable: true },
			CcmAddress { pubkey: Pubkey([0x33; 32]), is_writable: true },
		],
		fallback_address: Pubkey([0xf0; 32]),
	};

	let encoded = Encode::encode(&extra_accounts);
	// println!("{:?}", hex::encode(encoded));

	// Scale encoding format:
	// cf_receiver(32 bytes, bool),
	// size_of_vec(compact encoding), remaining_accounts_0(32 bytes, bool), remaining_accounts_1,
	// etc..
	assert_eq!(
		encoded,
		hex_literal::hex!(
			"1111111111111111111111111111111111111111111111111111111111111111 00
			08 
			2222222222222222222222222222222222222222222222222222222222222222 01
			3333333333333333333333333333333333333333333333333333333333333333 01
			F0F0F0F0F0F0F0F0F0F0F0F0F0F0F0F0F0F0F0F0F0F0F0F0F0F0F0F0F0F0F0F0"
		)
	);
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ParseHashError {
	#[error("string decoded to wrong size for hash")]
	WrongSize,
	#[error("failed to decoded string to hash")]
	Invalid,
}

#[cfg(test)]
impl FromStr for Hash {
	type Err = ParseHashError;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		if s.len() > MAX_BASE58_LEN {
			return Err(ParseHashError::WrongSize)
		}
		let bytes = bs58::decode(s).into_vec().map_err(|_| ParseHashError::Invalid)?;
		if bytes.len() != std::mem::size_of::<Hash>() {
			Err(ParseHashError::WrongSize)
		} else {
			Ok(Hash::new(&bytes))
		}
	}
}

/// Values used for testing purposes
#[cfg(any(test, feature = "runtime-integration-tests"))]
pub mod sol_test_values {
	use crate::{
		sol::{
			api::VaultSwapAccountAndSender, signing_key::SolSigningKey,
			sol_tx_core::signer::Signer, SolAddress, SolAmount, SolAsset, SolCcmAccounts,
			SolCcmAddress, SolComputeLimit, SolHash,
		},
		CcmChannelMetadata, CcmDepositMetadata, ForeignChain, ForeignChainAddress,
	};
	use sol_prim::consts::{const_address, const_hash};
	use sp_std::vec;

	pub const VAULT_PROGRAM: SolAddress =
		const_address("8inHGLHXegST3EPLcpisQe9D1hDT9r7DJjS395L3yuYf");
	pub const VAULT_PROGRAM_DATA_ADDRESS: SolAddress =
		const_address("3oEKmL4nsw6RDZWhkYTdCUmjxDrzVkm1cWayPsvn3p57");
	pub const VAULT_PROGRAM_DATA_ACCOUNT: SolAddress =
		const_address("wxudAoEJWfe6ZFHYsDPYGGs2K3m62N3yApNxZLGyMYc");
	// MIN_PUB_KEY per supported spl-token
	pub const USDC_TOKEN_MINT_PUB_KEY: SolAddress =
		const_address("24PNhTaNtomHhoy3fTRaMhAFCRj4uHqhZEEoWrKDbR5p");
	pub const TOKEN_VAULT_PDA_ACCOUNT: SolAddress =
		const_address("CWxWcNZR1d5MpkvmL3HgvgohztoKyCDumuZvdPyJHK3d");
	// This can be derived from the TOKEN_VAULT_PDA_ACCOUNT and the mintPubKey but we can have it
	// stored There will be a different one per each supported spl-token
	pub const USDC_TOKEN_VAULT_ASSOCIATED_TOKEN_ACCOUNT: SolAddress =
		const_address("GgqCE4bTwMy4QWVaTRTKJqETAgim49zNrH1dL6zXaTpd");
	pub const SWAP_ENDPOINT_DATA_ACCOUNT_ADDRESS: SolAddress =
		const_address("GgqCE4bTwMy4QWVaTRTKJqETAgim49zNrH1dL6zXaTpd");
	pub const NONCE_ACCOUNTS: [SolAddress; 10] = [
		const_address("2cNMwUCF51djw2xAiiU54wz1WrU8uG4Q8Kp8nfEuwghw"),
		const_address("HVG21SovGzMBJDB9AQNuWb6XYq4dDZ6yUwCbRUuFnYDo"),
		const_address("HDYArziNzyuNMrK89igisLrXFe78ti8cvkcxfx4qdU2p"),
		const_address("HLPsNyxBqfq2tLE31v6RiViLp2dTXtJRgHgsWgNDRPs2"),
		const_address("GKMP63TqzbueWTrFYjRwMNkAyTHpQ54notRbAbMDmePM"),
		const_address("EpmHm2aSPsB5ZZcDjqDhQ86h1BV32GFCbGSMuC58Y2tn"),
		const_address("9yBZNMrLrtspj4M7bEf2X6tqbqHxD2vNETw8qSdvJHMa"),
		const_address("J9dT7asYJFGS68NdgDCYjzU2Wi8uBoBusSHN1Z6JLWna"),
		const_address("GUMpVpQFNYJvSbyTtUarZVL7UDUgErKzDTSVJhekUX55"),
		const_address("AUiHYbzH7qLZSkb3u7nAqtvqC7e41sEzgWjBEvXrpfGv"),
	];
	pub const SWAP_ENDPOINT_PROGRAM: SolAddress =
		const_address("35uYgHdfZQT4kHkaaXQ6ZdCkK5LFrsk43btTLbGCRCNT");
	pub const SWAP_ENDPOINT_PROGRAM_DATA_ACCOUNT: SolAddress =
		const_address("2tmtGLQcBd11BMiE9B1tAkQXwmPNgR79Meki2Eme4Ec9");
	pub const EVENT_AND_SENDER_ACCOUNTS: [VaultSwapAccountAndSender; 11] = [
		VaultSwapAccountAndSender {
			vault_swap_account: const_address("2cHcSNtikMpjxJfwwoYL3udpy7hedRExyhakk2eZ6cYA"),
			swap_sender: const_address("7tVhSXxGfZyHQem8MdZVB6SoRsrvV4H8h1rX6hwBuvEA"),
		},
		VaultSwapAccountAndSender {
			vault_swap_account: const_address("6uuU1NFyThN3KJpU9mYXkGSmd8Qgncmd9aYAWYN71VkC"),
			swap_sender: const_address("P3GYr1Z67jdBVimzFjMXQpeuew5TY5txoZ9CvqASpaP"),
		},
		VaultSwapAccountAndSender {
			vault_swap_account: const_address("DmAom3kp2ZKk9cnbWEsnbkLHkp3sx9ef1EX6GWj1JRUB"),
			swap_sender: const_address("CS7yX5TKX36ugF4bycmVQ5vqB2ZbNVC5tvtrtLP92GDW"),
		},
		VaultSwapAccountAndSender {
			vault_swap_account: const_address("CJSdHgxwHLEbTsxKsJk9UyJxUEgku2UC9GXRTzR2ieSh"),
			swap_sender: const_address("2taCR53epDtdrFZBxzKcbmv3cb5Umc5x9k2YCjmTDAnH"),
		},
		VaultSwapAccountAndSender {
			vault_swap_account: const_address("7DGwjsQEFA7XzZS9z5YbMhYGzWJSh5T78hRrU47RDTd2"),
			swap_sender: const_address("FDPzoZj951Hq92jhoFdyzAVyUjyXhL8VEnqBhyjsDhow"),
		},
		VaultSwapAccountAndSender {
			vault_swap_account: const_address("A6yYXUmZHa32mcFRnwwq8ZQKCEYUn9ewF1vWn2wsXN5a"),
			swap_sender: const_address("9bNNNU9B52VPVGm6zRccwPEexDHD1ntndD2aNu2un3ca"),
		},
		VaultSwapAccountAndSender {
			vault_swap_account: const_address("2F3365PULNzt7moa9GgHARy7Lumj5ptDQF7wDt6xeuHK"),
			swap_sender: const_address("4m5t38fJsvULKaPyWZKWjzfbvnzBGL86BTRNk5vLLUrh"),
		},
		VaultSwapAccountAndSender {
			vault_swap_account: const_address("8sCBWv9tzdf2iC4GNj61UBN6TZpzsLP5Ppv9x1ENX4HT"),
			swap_sender: const_address("A3P5kfRU1vgZn7GjNMomS8ye6GHsoHC4JoVNUotMbDPE"),
		},
		VaultSwapAccountAndSender {
			vault_swap_account: const_address("3b1FkNvnvKJ4TzKeft7wA47VfYpjkoHPE4ER13ZTNecX"),
			swap_sender: const_address("ERwuPnX66dCZqj85kH9QQJmwcVrzcczBnu8onJY2R7tG"),
		},
		VaultSwapAccountAndSender {
			vault_swap_account: const_address("Bnrp9X562krXVfaY8FnwJa3Mxp1gbDCrvGNW1qc99rKe"),
			swap_sender: const_address("2aoZg41FFnTBnuHpkfHdFsCuPz8DhN4dsUW5386XwE8g"),
		},
		VaultSwapAccountAndSender {
			vault_swap_account: const_address("EuLceVgXMaJNPT7C88pnL7DRWcf1poy9BCeWY1GL8Agd"),
			swap_sender: const_address("G1iXMtwUU76JGau9cJm6N8wBTmcsvyXuJcC7PtfU1TXZ"),
		},
	];
	pub const RAW_KEYPAIR: [u8; 32] = [
		6, 151, 150, 20, 145, 210, 176, 113, 98, 200, 192, 80, 73, 63, 133, 232, 208, 124, 81, 213,
		117, 199, 196, 243, 219, 33, 79, 217, 157, 69, 205, 140,
	];
	pub const TRANSFER_AMOUNT: SolAmount = 1_000_000_000u64;
	pub const COMPUTE_UNIT_PRICE: SolAmount = 1_000_000u64;
	pub const COMPUTE_UNIT_LIMIT: SolComputeLimit = 300_000u32;
	pub const TEST_DURABLE_NONCE: SolHash =
		const_hash("E6E2bNxGcgFyqeVRT3FSjw7YFbbMAZVQC21ZLVwrztRm");
	pub const FETCH_FROM_ACCOUNT: SolAddress =
		const_address("4Spd3kst7XsA9pdp5ArfdXxEK4xfW88eRKbyQBmMvwQj");
	pub const TRANSFER_TO_ACCOUNT: SolAddress =
		const_address("4MqL4qy2W1yXzuF3PiuSMehMbJzMuZEcBwVvrgtuhx7V");
	pub const NEW_AGG_KEY: SolAddress =
		const_address("7x7wY9yfXjRmusDEfPPCreU4bP49kmH4mqjYUXNAXJoM");

	pub const NEXT_NONCE: SolAddress = NONCE_ACCOUNTS[0];
	pub const SOL: SolAsset = SolAsset::Sol;
	pub const USDC: SolAsset = SolAsset::SolUsdc;

	pub fn ccm_accounts() -> SolCcmAccounts {
		SolCcmAccounts {
			cf_receiver: SolCcmAddress {
				pubkey: const_address("8pBPaVfTAcjLeNfC187Fkvi9b1XEFhRNJ95BQXXVksmH").into(),
				is_writable: true,
			},
			remaining_accounts: vec![SolCcmAddress {
				pubkey: const_address("CFp37nEY6E9byYHiuxQZg6vMCnzwNrgiF9nFGT6Zwcnx").into(),
				is_writable: false,
			}],
			fallback_address: const_address("AkYRjwVHBCcE1HsjZaTFr5SrTNHPRX7PtwZxdSDMcTvb").into(),
		}
	}

	pub fn ccm_parameter() -> CcmDepositMetadata {
		CcmDepositMetadata {
			source_chain: ForeignChain::Ethereum,
			source_address: Some(ForeignChainAddress::Eth([0xff; 20].into())),
			channel_metadata: CcmChannelMetadata {
				message: vec![124u8, 29u8, 15u8, 7u8].try_into().unwrap(), // CCM message
				gas_budget: 0u128,                                         // unused
				ccm_additional_data: codec::Encode::encode(&ccm_accounts())
					.try_into()
					.expect("Test data cannot be too long"), // Extra addresses
			},
		}
	}

	pub fn agg_key() -> SolAddress {
		SolSigningKey::from_bytes(&RAW_KEYPAIR).unwrap().pubkey().into()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		sol::{
			signing_key::SolSigningKey,
			sol_tx_core::{
				address_derivation::{
					derive_associated_token_account, derive_deposit_address, derive_fetch_account,
				},
				compute_budget::ComputeBudgetInstruction,
				program_instructions::{InstructionExt, SystemProgramInstruction, VaultProgram},
				signer::Signer,
				sol_test_values::*,
				token_instructions::AssociatedTokenAccountInstruction,
				AccountMeta, BorshDeserialize, BorshSerialize, Hash, Instruction, Message, Pubkey,
				Transaction,
			},
			SolAddress,
		},
		ForeignChainAddress,
	};
	use core::str::FromStr;
	use sol_prim::{
		consts::{
			MAX_TRANSACTION_LENGTH, SOL_USDC_DECIMAL, SYSTEM_PROGRAM_ID, SYS_VAR_INSTRUCTIONS,
			TOKEN_PROGRAM_ID,
		},
		PdaAndBump,
	};

	#[derive(BorshSerialize, BorshDeserialize)]
	enum BankInstruction {
		Initialize,
		Deposit { lamports: u64 },
		Withdraw { lamports: u64 },
	}

	#[test]
	fn create_simple_tx() {
		fn send_initialize_tx(program_id: Pubkey, payer: &SolSigningKey) -> Result<(), ()> {
			let bank_instruction = BankInstruction::Initialize;

			let instruction = Instruction::new_with_borsh(program_id, &bank_instruction, vec![]);

			let mut tx = Transaction::new_with_payer(&[instruction], Some(&payer.pubkey()));
			tx.sign(&[payer], Default::default());
			Ok(())
		}

		// let client = RpcClient::new(String::new());
		let program_id = Pubkey([0u8; 32]);
		let payer = SolSigningKey::new();
		let _ = send_initialize_tx(program_id, &payer);
	}

	#[test]
	fn create_transfer_native() {
		let durable_nonce = TEST_DURABLE_NONCE.into();
		let agg_key_keypair = SolSigningKey::from_bytes(&RAW_KEYPAIR).unwrap();
		let agg_key_pubkey = agg_key_keypair.pubkey();
		let to_pubkey = TRANSFER_TO_ACCOUNT.into();
		let instructions = [
			SystemProgramInstruction::advance_nonce_account(
				&NONCE_ACCOUNTS[0].into(),
				&agg_key_pubkey,
			),
			ComputeBudgetInstruction::set_compute_unit_price(COMPUTE_UNIT_PRICE),
			ComputeBudgetInstruction::set_compute_unit_limit(COMPUTE_UNIT_LIMIT),
			SystemProgramInstruction::transfer(&agg_key_pubkey, &to_pubkey, TRANSFER_AMOUNT),
		];
		let message =
			Message::new_with_blockhash(&instructions, Some(&agg_key_pubkey), &durable_nonce);
		let mut tx = Transaction::new_unsigned(message);
		tx.sign(&[&agg_key_keypair], durable_nonce);

		let serialized_tx = tx.finalize_and_serialize().unwrap();
		let expected_serialized_tx = hex_literal::hex!("01345c86d1be2bcdf2c93c75b6054b6232e5b1e7f2fe7b3ca241d48c8a5f993af3e474bf581b2e9a1543af13104b3f3a53530d849731cc403418da313743a57e0401000306f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d19231e9528aae784fecbbd0bee129d9539c57be0e90061af6b6f4a5e274654e5bd400000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea9400000c27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890004030301050004040000000400090340420f000000000004000502e0930400030200020c0200000000ca9a3b00000000").to_vec();

		// println!("{:?}", hex::encode(serialized_tx.clone()));

		assert_eq!(serialized_tx, expected_serialized_tx);
		assert!(serialized_tx.len() <= MAX_TRANSACTION_LENGTH)
	}

	#[test]
	fn create_transfer_cu_priority_fees() {
		let durable_nonce = Hash::from_str("2GGxiEHwtWPGNKH5czvxRGvQTayRvCT1PFsA9yK2iMnq").unwrap();
		let agg_key_keypair = SolSigningKey::from_bytes(&RAW_KEYPAIR).unwrap();
		let agg_key_pubkey = agg_key_keypair.pubkey();
		let to_pubkey = TRANSFER_TO_ACCOUNT.into();

		let lamports = 1_000_000;
		let instructions = [
			SystemProgramInstruction::advance_nonce_account(
				&NONCE_ACCOUNTS[0].into(),
				&agg_key_pubkey,
			),
			ComputeBudgetInstruction::set_compute_unit_price(COMPUTE_UNIT_PRICE),
			ComputeBudgetInstruction::set_compute_unit_limit(COMPUTE_UNIT_LIMIT),
			SystemProgramInstruction::transfer(&agg_key_pubkey, &to_pubkey, lamports),
		];
		let message =
			Message::new_with_blockhash(&instructions, Some(&agg_key_pubkey), &durable_nonce);
		let mut tx = Transaction::new_unsigned(message);
		tx.sign(&[&agg_key_keypair], durable_nonce);

		let serialized_tx = tx.finalize_and_serialize().unwrap();
		let expected_serialized_tx = hex_literal::hex!("017036ecc82313548a7f1ef280b9d7c53f9747e23abcb4e76d86c8df6aa87e82d460ad7cea2e8d972a833d3e1802341448a99be200ad4648c454b9d5a5e2d5020d01000306f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d19231e9528aae784fecbbd0bee129d9539c57be0e90061af6b6f4a5e274654e5bd400000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000012c57218f6315b83818802f3522fe7e04c596ae4fe08841e7940bc2f958aaaea04030301050004040000000400090340420f000000000004000502e0930400030200020c0200000040420f0000000000").to_vec();

		assert_eq!(serialized_tx, expected_serialized_tx);
		assert!(serialized_tx.len() <= MAX_TRANSACTION_LENGTH)
	}

	#[test]
	fn create_fetch_native() {
		let durable_nonce = TEST_DURABLE_NONCE.into();
		let agg_key_keypair = SolSigningKey::from_bytes(&RAW_KEYPAIR).unwrap();
		let agg_key_pubkey = agg_key_keypair.pubkey();
		let vault_program_id = VAULT_PROGRAM;
		let deposit_channel: Pubkey = FETCH_FROM_ACCOUNT.into();
		let deposit_channel_historical_fetch =
			derive_fetch_account(SolAddress::from(deposit_channel), vault_program_id)
				.unwrap()
				.address;

		let instructions = [
			SystemProgramInstruction::advance_nonce_account(
				&NONCE_ACCOUNTS[0].into(),
				&agg_key_pubkey,
			),
			ComputeBudgetInstruction::set_compute_unit_price(COMPUTE_UNIT_PRICE),
			ComputeBudgetInstruction::set_compute_unit_limit(COMPUTE_UNIT_LIMIT),
			VaultProgram::with_id(VAULT_PROGRAM).fetch_native(
				vec![11u8, 12u8, 13u8, 55u8, 0u8, 0u8, 0u8, 0u8],
				255,
				VAULT_PROGRAM_DATA_ACCOUNT,
				agg_key_pubkey,
				deposit_channel,
				deposit_channel_historical_fetch,
				SYSTEM_PROGRAM_ID,
			),
		];
		let message =
			Message::new_with_blockhash(&instructions, Some(&agg_key_pubkey), &durable_nonce);
		let mut tx = Transaction::new_unsigned(message);
		tx.sign(&[&agg_key_keypair], durable_nonce);
		// println!("{:?}", tx);

		let serialized_tx =
			tx.finalize_and_serialize().expect("Transaction serialization should succeed");

		// With compute unit price and limit
		let expected_serialized_tx = hex_literal::hex!("01f18817b1a85d96d5084c5534866d5638ab4298e458c169464ecd11081d7b4c4091ac3ff41d9c1582a7d9f2f6f8bebb9cdaec0e1090d9842f5d80196882cb660f01000509f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d1921be0fac7f9583cfe14f5c09dd7653c597f93168e946760abaad3e3c2cc101f5233306d43f017cdb7b1a324afdc62c79317d5b93e2e63b870143344134db9c60000000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea94000000e14940a2247d0a8a33650d7dfe12d269ecabce61c1219b5a6dcdb6961026e0972b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293cc27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890004040301060004040000000500090340420f000000000005000502e093040008050700030204158e24658f6c59298c080000000b0c0d3700000000ff").to_vec();

		// println!("tx:{:?}", hex::encode(serialized_tx.clone()));

		assert_eq!(serialized_tx, expected_serialized_tx);
		assert!(serialized_tx.len() <= MAX_TRANSACTION_LENGTH)
	}

	#[test]
	fn create_fetch_native_in_batch() {
		let durable_nonce = TEST_DURABLE_NONCE.into();
		let agg_key_keypair = SolSigningKey::from_bytes(&RAW_KEYPAIR).unwrap();
		let agg_key_pubkey = agg_key_keypair.pubkey();
		let vault_program_id = VAULT_PROGRAM;

		let deposit_channel_0 = derive_deposit_address(0u64, vault_program_id).unwrap();
		let deposit_channel_1 = derive_deposit_address(1u64, vault_program_id).unwrap();

		let deposit_channel_historical_fetch_0 =
			derive_fetch_account(deposit_channel_0.address, vault_program_id).unwrap();
		let deposit_channel_historical_fetch_1 =
			derive_fetch_account(deposit_channel_1.address, vault_program_id).unwrap();

		let vault_program = VaultProgram::with_id(VAULT_PROGRAM);

		let instructions = [
			SystemProgramInstruction::advance_nonce_account(
				&NONCE_ACCOUNTS[0].into(),
				&agg_key_pubkey,
			),
			ComputeBudgetInstruction::set_compute_unit_price(COMPUTE_UNIT_PRICE),
			ComputeBudgetInstruction::set_compute_unit_limit(COMPUTE_UNIT_LIMIT),
			vault_program.fetch_native(
				0u64.to_le_bytes().to_vec(),
				deposit_channel_0.bump,
				VAULT_PROGRAM_DATA_ACCOUNT,
				agg_key_pubkey,
				deposit_channel_0.address,
				deposit_channel_historical_fetch_0.address,
				SYSTEM_PROGRAM_ID,
			),
			vault_program.fetch_native(
				1u64.to_le_bytes().to_vec(),
				deposit_channel_1.bump,
				VAULT_PROGRAM_DATA_ACCOUNT,
				agg_key_pubkey,
				deposit_channel_1.address,
				deposit_channel_historical_fetch_1.address,
				SYSTEM_PROGRAM_ID,
			),
		];
		let message =
			Message::new_with_blockhash(&instructions, Some(&agg_key_pubkey), &durable_nonce);
		let mut tx = Transaction::new_unsigned(message);
		tx.sign(&[&agg_key_keypair], durable_nonce);
		// println!("{:?}", tx);

		let serialized_tx =
			tx.finalize_and_serialize().expect("Transaction serialization should succeed");

		// With compute unit price and limit
		let expected_serialized_tx = hex_literal::hex!("01b7fdd98ccb9f9656a15deee41677b35054b8ae500e6f87e618eccda73eccecc053eff8a30e106d81415ec128438247495993c52d9d2b95ae34ac15a5006839000100050bf79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d19238861d2f0bf5cd80031b701a6c25d13b4c812dd92f9d6301fafd9a58fb9e438646cd507258c10454d484e64ba59d3e7570658001c5f854b6b3ebb57be90e7a708d9871ed5fb2ee05765af23b7cabcc0d6b08ed370bb9f616a0d4dea40a25f870b5b9d633289c8fd72fb05f33349bf4cc44e82add5d865311ae346d7c9a67b7dd00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea94000000e14940a2247d0a8a33650d7dfe12d269ecabce61c1219b5a6dcdb6961026e0972b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293cc27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890005060301080004040000000700090340420f000000000007000502e09304000a050900030206158e24658f6c59298c080000000000000000000000ff0a050900040506158e24658f6c59298c080000000100000000000000ff").to_vec();

		// println!("tx:{:?}", hex::encode(serialized_tx.clone()));

		assert_eq!(serialized_tx, expected_serialized_tx);
		assert!(serialized_tx.len() <= MAX_TRANSACTION_LENGTH)
	}

	#[test]
	fn create_fetch_tokens() {
		let durable_nonce = TEST_DURABLE_NONCE.into();
		let agg_key_keypair = SolSigningKey::from_bytes(&RAW_KEYPAIR).unwrap();
		let agg_key_pubkey = agg_key_keypair.pubkey();
		let vault_program_id = VAULT_PROGRAM;
		let token_mint_pubkey = USDC_TOKEN_MINT_PUB_KEY;

		let seed = 0u64;
		let deposit_channel = derive_deposit_address(seed, vault_program_id).unwrap();
		let deposit_channel_ata =
			derive_associated_token_account(deposit_channel.address, token_mint_pubkey).unwrap();
		let deposit_channel_historical_fetch =
			derive_fetch_account(deposit_channel_ata.address, vault_program_id).unwrap();

		// Deposit channel derived from the Vault address from the seed and the bump
		assert_eq!(
			deposit_channel,
			PdaAndBump {
				address: SolAddress::from_str("5mP7x1r66PC62PFxXTiEEJVd2Guddc3vWEAkhgWxXehm")
					.unwrap(),
				bump: 255u8
			},
		);
		assert_eq!(
			deposit_channel_ata,
			PdaAndBump {
				address: SolAddress::from_str("5WXnwDp1AA4QZqi3CJEx7HGjTPBj9h42pLwCRuV7AmGs")
					.unwrap(),
				bump: 255u8
			},
		);
		// Historical fetch account derived from the Vault address using the ATA as the seed
		assert_eq!(
			deposit_channel_historical_fetch,
			PdaAndBump {
				address: SolAddress::from_str("CkGQUU19izDobt5NLGmj2h6DBMFRkmj6WN6onNtQVwzn")
					.unwrap(),
				bump: 255u8
			},
		);
		let instructions = [
			SystemProgramInstruction::advance_nonce_account(
				&NONCE_ACCOUNTS[0].into(),
				&agg_key_pubkey,
			),
			ComputeBudgetInstruction::set_compute_unit_price(COMPUTE_UNIT_PRICE),
			ComputeBudgetInstruction::set_compute_unit_limit(COMPUTE_UNIT_LIMIT),
			VaultProgram::with_id(VAULT_PROGRAM).fetch_tokens(
				seed.to_le_bytes().to_vec(),
				deposit_channel.bump,
				6,
				VAULT_PROGRAM_DATA_ACCOUNT,
				agg_key_pubkey,
				deposit_channel.address,
				deposit_channel_ata.address,
				USDC_TOKEN_VAULT_ASSOCIATED_TOKEN_ACCOUNT,
				USDC_TOKEN_MINT_PUB_KEY,
				TOKEN_PROGRAM_ID,
				deposit_channel_historical_fetch.address,
				SYSTEM_PROGRAM_ID,
			),
		];
		let message =
			Message::new_with_blockhash(&instructions, Some(&agg_key_pubkey), &durable_nonce);
		let mut tx = Transaction::new_unsigned(message);
		tx.sign(&[&agg_key_keypair], durable_nonce);

		let serialized_tx = tx.finalize_and_serialize().unwrap();
		let expected_serialized_tx = hex_literal::hex!("01e365eba1f7a74950e7de854a21f978496bc7ae715cbb3cc7f521b8fb724b122146e0fe31b987418b50769a60513fd34ea9c7a15e732120c5639751f05ec19a040100080df79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d19242ff6863b52c3f8faf95739e6541bda5d0ac593f00c6c07d9ab37096bf26d910ae85f2fb6289c70bfe37df150dddb17dd84f403fd0b1aa1bfee85795159de21fe91372b3d301c202a633da0a92365a736e462131aecfad1fac47322cf8863ada00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000006ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a90e14940a2247d0a8a33650d7dfe12d269ecabce61c1219b5a6dcdb6961026e090fb9ba52b1f09445f1e3a7508d59f0797923acf744fbe2da303fb06da859ee8746cd507258c10454d484e64ba59d3e7570658001c5f854b6b3ebb57be90e7a7072b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293cc27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890004050301070004040000000600090340420f000000000006000502e09304000c0909000b02040a08030516494710642cb0c646080000000000000000000000ff06").to_vec();

		// println!("{:?}", hex::encode(serialized_tx.clone()));

		assert_eq!(serialized_tx, expected_serialized_tx);
		assert!(serialized_tx.len() <= MAX_TRANSACTION_LENGTH)
	}

	#[test]
	fn create_batch_fetch() {
		let durable_nonce = TEST_DURABLE_NONCE.into();
		let agg_key_keypair = SolSigningKey::from_bytes(&RAW_KEYPAIR).unwrap();
		let agg_key_pubkey = agg_key_keypair.pubkey();
		let vault_program_id = VAULT_PROGRAM;
		let token_mint_pubkey = USDC_TOKEN_MINT_PUB_KEY;

		let deposit_channel_0 = derive_deposit_address(0u64, vault_program_id).unwrap();
		let deposit_channel_ata_0 =
			derive_associated_token_account(deposit_channel_0.address, token_mint_pubkey).unwrap();
		let deposit_channel_historical_fetch_0 =
			derive_fetch_account(deposit_channel_ata_0.address, vault_program_id).unwrap();

		let deposit_channel_1 = derive_deposit_address(1u64, vault_program_id).unwrap();
		let deposit_channel_ata_1 =
			derive_associated_token_account(deposit_channel_1.address, token_mint_pubkey).unwrap();
		let deposit_channel_historical_fetch_1 =
			derive_fetch_account(deposit_channel_ata_1.address, vault_program_id).unwrap();

		let deposit_channel_2 = derive_deposit_address(2u64, vault_program_id).unwrap();
		let deposit_channel_historical_fetch_2 =
			derive_fetch_account(deposit_channel_2.address, vault_program_id).unwrap();

		let instructions = [
			SystemProgramInstruction::advance_nonce_account(
				&NONCE_ACCOUNTS[0].into(),
				&agg_key_pubkey,
			),
			ComputeBudgetInstruction::set_compute_unit_price(COMPUTE_UNIT_PRICE),
			ComputeBudgetInstruction::set_compute_unit_limit(COMPUTE_UNIT_LIMIT),
			VaultProgram::with_id(VAULT_PROGRAM).fetch_tokens(
				0u64.to_le_bytes().to_vec(),
				deposit_channel_0.bump,
				6,
				VAULT_PROGRAM_DATA_ACCOUNT,
				agg_key_pubkey,
				deposit_channel_0.address,
				deposit_channel_ata_0.address,
				USDC_TOKEN_VAULT_ASSOCIATED_TOKEN_ACCOUNT,
				USDC_TOKEN_MINT_PUB_KEY,
				TOKEN_PROGRAM_ID,
				deposit_channel_historical_fetch_0.address,
				SYSTEM_PROGRAM_ID,
			),
			VaultProgram::with_id(VAULT_PROGRAM).fetch_tokens(
				1u64.to_le_bytes().to_vec(),
				deposit_channel_1.bump,
				6,
				VAULT_PROGRAM_DATA_ACCOUNT,
				agg_key_pubkey,
				deposit_channel_1.address,
				deposit_channel_ata_1.address,
				USDC_TOKEN_VAULT_ASSOCIATED_TOKEN_ACCOUNT,
				USDC_TOKEN_MINT_PUB_KEY,
				TOKEN_PROGRAM_ID,
				deposit_channel_historical_fetch_1.address,
				SYSTEM_PROGRAM_ID,
			),
			VaultProgram::with_id(VAULT_PROGRAM).fetch_native(
				2u64.to_le_bytes().to_vec(),
				deposit_channel_2.bump,
				VAULT_PROGRAM_DATA_ACCOUNT,
				agg_key_pubkey,
				deposit_channel_2.address,
				deposit_channel_historical_fetch_2.address,
				SYSTEM_PROGRAM_ID,
			),
		];
		let message = Message::new(&instructions, Some(&agg_key_pubkey));
		let mut tx = Transaction::new_unsigned(message);
		tx.sign(&[&agg_key_keypair], durable_nonce);

		let serialized_tx = tx.finalize_and_serialize().unwrap();
		let expected_serialized_tx = hex_literal::hex!("01b83f6669762c6f6987f1750aaab985da08a5ae70ad98ac525c19ee02d2909961bde19968c1e6cc80070f6c18bcd9bfc75b3541b0789bb38b83b2b7fd8e9e250e01000912f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d1921ad0968d57ee79348476716f9b2cd44ec4284b8f52c36648d560949e41589a5540de1c0451cccb6edd1fda9b4a48c282b279350b55a7a9716800cc0132b6f0b042ff6863b52c3f8faf95739e6541bda5d0ac593f00c6c07d9ab37096bf26d910a140fd3d05766f0087d57bf99df05731e894392ffcc8e8d7e960ba73c09824aaae85f2fb6289c70bfe37df150dddb17dd84f403fd0b1aa1bfee85795159de21fb4baefcd4965beb1c71311a2ffe76419d4b8f8d35fbc4cf514b1bd02da2df2e3e91372b3d301c202a633da0a92365a736e462131aecfad1fac47322cf8863ada00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000006ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a90e14940a2247d0a8a33650d7dfe12d269ecabce61c1219b5a6dcdb6961026e090fb9ba52b1f09445f1e3a7508d59f0797923acf744fbe2da303fb06da859ee8746cd507258c10454d484e64ba59d3e7570658001c5f854b6b3ebb57be90e7a7072b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293c8d9871ed5fb2ee05765af23b7cabcc0d6b08ed370bb9f616a0d4dea40a25f870c27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e48900060903010b0004040000000a00090340420f00000000000a000502e093040010090d000f04080e0c060916494710642cb0c646080000000000000000000000ff0610090d001102080e0c030916494710642cb0c646080000000100000000000000ff0610050d00050709158e24658f6c59298c080000000200000000000000ff").to_vec();

		// println!("{:?}", hex::encode(serialized_tx.clone()));

		assert_eq!(serialized_tx, expected_serialized_tx);
		assert!(serialized_tx.len() <= MAX_TRANSACTION_LENGTH)
	}

	#[test]
	fn create_transfer_tokens() {
		let durable_nonce = TEST_DURABLE_NONCE.into();
		let agg_key_keypair = SolSigningKey::from_bytes(&RAW_KEYPAIR).unwrap();
		let agg_key_pubkey = agg_key_keypair.pubkey();
		let token_mint_pubkey = USDC_TOKEN_MINT_PUB_KEY;

		let to_pubkey = TRANSFER_TO_ACCOUNT;
		let to_pubkey_ata = derive_associated_token_account(to_pubkey, token_mint_pubkey).unwrap();

		let instructions = [
			SystemProgramInstruction::advance_nonce_account(
				&NONCE_ACCOUNTS[0].into(),
				&agg_key_pubkey,
			),
			ComputeBudgetInstruction::set_compute_unit_price(COMPUTE_UNIT_PRICE),
			ComputeBudgetInstruction::set_compute_unit_limit(COMPUTE_UNIT_LIMIT),
			AssociatedTokenAccountInstruction::create_associated_token_account_idempotent_instruction(
				&agg_key_pubkey,
				&to_pubkey.into(),
				&USDC_TOKEN_MINT_PUB_KEY.into(),
				&to_pubkey_ata.address.into(),
			),
			VaultProgram::with_id(VAULT_PROGRAM).transfer_tokens(
				TRANSFER_AMOUNT,
				SOL_USDC_DECIMAL,
				VAULT_PROGRAM_DATA_ACCOUNT,
				agg_key_pubkey,
				TOKEN_VAULT_PDA_ACCOUNT,
				USDC_TOKEN_VAULT_ASSOCIATED_TOKEN_ACCOUNT,
				to_pubkey_ata.address,
				USDC_TOKEN_MINT_PUB_KEY,
				TOKEN_PROGRAM_ID,
			),
		];
		let message =
			Message::new_with_blockhash(&instructions, Some(&agg_key_pubkey), &durable_nonce);
		let mut tx = Transaction::new_unsigned(message);
		tx.sign(&[&agg_key_keypair], durable_nonce);

		let serialized_tx = tx.finalize_and_serialize().unwrap();
		let expected_serialized_tx = hex_literal::hex!("014b3dcc9d694f8f0175546e0c8b0cedbe4c1a371cac7108d5029b625ced6dee9d38a97458a3dfa3efbc0d26545fec4f7fa199b41317b219b6ff6c93070d8dd10501000a0ef79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d1925ec7baaea7200eb2a66ccd361ee73bc87a7e5222ecedcbc946e97afb59ec4616e91372b3d301c202a633da0a92365a736e462131aecfad1fac47322cf8863ada00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000006ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a90e14940a2247d0a8a33650d7dfe12d269ecabce61c1219b5a6dcdb6961026e090fb9ba52b1f09445f1e3a7508d59f0797923acf744fbe2da303fb06da859ee8731e9528aae784fecbbd0bee129d9539c57be0e90061af6b6f4a5e274654e5bd472b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293c8c97258f4e2489f1bb3d1029148e0d830b5a1399daff1084048e7bd8dbe9f859ab1d2a644046552e73f4d05b5a6ef53848973a9ee9febba42ddefb034b5f5130c27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890005040301060004040000000500090340420f000000000005000502e09304000c0600020a09040701010b0708000d030209071136b4eeaf4a557ebc00ca9a3b0000000006").to_vec();

		// println!("{:?}", hex::encode(serialized_tx.clone()));

		assert_eq!(serialized_tx, expected_serialized_tx);
		assert!(serialized_tx.len() <= MAX_TRANSACTION_LENGTH)
	}

	// Full rotation: Use nonce, rotate agg key, transfer nonce authority and transfer upgrade
	// manager's upgrade authority
	#[test]
	fn create_full_rotation() {
		let durable_nonce = TEST_DURABLE_NONCE.into();
		let agg_key_keypair = SolSigningKey::from_bytes(&RAW_KEYPAIR).unwrap();
		let agg_key_pubkey = agg_key_keypair.pubkey();
		let new_agg_key_pubkey = NEW_AGG_KEY.into();

		let mut instructions = vec![
			SystemProgramInstruction::advance_nonce_account(
				&NONCE_ACCOUNTS[0].into(),
				&agg_key_pubkey,
			),
			ComputeBudgetInstruction::set_compute_unit_price(COMPUTE_UNIT_PRICE),
			ComputeBudgetInstruction::set_compute_unit_limit(COMPUTE_UNIT_LIMIT),
			VaultProgram::with_id(VAULT_PROGRAM).rotate_agg_key(
				false,
				VAULT_PROGRAM_DATA_ACCOUNT,
				agg_key_pubkey,
				new_agg_key_pubkey,
				SYSTEM_PROGRAM_ID,
			),
		];
		instructions.extend(NONCE_ACCOUNTS.into_iter().map(|nonce_account| {
			SystemProgramInstruction::nonce_authorize(
				&nonce_account.into(),
				&agg_key_pubkey,
				&new_agg_key_pubkey,
			)
		}));

		let message =
			Message::new_with_blockhash(&instructions, Some(&agg_key_pubkey), &durable_nonce);
		let mut tx = Transaction::new_unsigned(message);
		tx.sign(&[&agg_key_keypair], durable_nonce);

		let serialized_tx = tx.finalize_and_serialize().unwrap();
		let expected_serialized_tx = hex_literal::hex!("017663fd8be6c54a3ce492a4aac1f50ed8a1589f8aa091d04b52e6fa8a43f22d359906e21630ca3dd93179e989bc1fdccbae8f9a30f6470ef9d5c17a7625f0050a01000411f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb0e14940a2247d0a8a33650d7dfe12d269ecabce61c1219b5a6dcdb6961026e0917eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d1926744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900448541f57201f277c5f3ffb631d0212e26e7f47749c26c4808718174a0ab2a09a18cd28baa84f2067bbdf24513c2d44e44bf408f2e6da6e60762e3faa4a62a0adbcd644e45426a41a7cb8369b8a0c1c89bb3f86cf278fdd9cc38b0f69784ad5667e392cd98d3284fd551604be95c14cc8e20123e2940ef9fb784e6b591c7442864e5e1869817a4fd88ddf7ab7a5f7252d7c345b39721769888608592912e8ca9acf0f13460b3fd04b7d53d7421fc874ec00eec769cf36480895e1a407bf1249475f2b2e24122be016983be9369965246cc45e1f621d40fba300c56c7ac50c3874df4f83bd213a59c9785110cf83c718f9486c3484f918593bce20c61dc6a96036afecc89e3b031824af6363174d19bbec12d3a13c4a173e5aeb349b63042bc138f00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000072b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293cc27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e489000e0d03020f0004040000000e00090340420f00000000000e000502e093040010040100030d094e518fabdda5d68b000d02020024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d020b0024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d02090024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d020a0024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d02070024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d02060024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d02040024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d020c0024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d02080024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be5439900440d02050024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be543990044").to_vec();

		// println!("tx:{:?}", hex::encode(serialized_tx.clone()));

		assert_eq!(serialized_tx, expected_serialized_tx);
		assert!(serialized_tx.len() <= MAX_TRANSACTION_LENGTH)
	}

	#[test]
	fn create_ccm_native_transfer() {
		let durable_nonce = TEST_DURABLE_NONCE.into();
		let agg_key_keypair = SolSigningKey::from_bytes(&RAW_KEYPAIR).unwrap();
		let agg_key_pubkey = agg_key_keypair.pubkey();
		let to_pubkey = TRANSFER_TO_ACCOUNT.into();
		let extra_accounts = ccm_accounts();

		let ccm_parameter = ccm_parameter();

		let instructions = [
			SystemProgramInstruction::advance_nonce_account(
				&NONCE_ACCOUNTS[0].into(),
				&agg_key_pubkey,
			),
			ComputeBudgetInstruction::set_compute_unit_price(COMPUTE_UNIT_PRICE),
			ComputeBudgetInstruction::set_compute_unit_limit(COMPUTE_UNIT_LIMIT),
			SystemProgramInstruction::transfer(&agg_key_pubkey, &to_pubkey, TRANSFER_AMOUNT),
			VaultProgram::with_id(VAULT_PROGRAM)
				.execute_ccm_native_call(
					ccm_parameter.source_chain as u32,
					ForeignChainAddress::to_source_address(ccm_parameter.source_address.unwrap()),
					ccm_parameter.channel_metadata.message.to_vec(),
					TRANSFER_AMOUNT,
					VAULT_PROGRAM_DATA_ACCOUNT,
					agg_key_pubkey,
					to_pubkey,
					extra_accounts.clone().cf_receiver,
					SYSTEM_PROGRAM_ID,
					SYS_VAR_INSTRUCTIONS,
				)
				.with_remaining_accounts(extra_accounts.remaining_account_metas()),
		];
		let message =
			Message::new_with_blockhash(&instructions, Some(&agg_key_pubkey), &durable_nonce);
		let mut tx = Transaction::new_unsigned(message);
		tx.sign(&[&agg_key_keypair], durable_nonce);

		let serialized_tx = tx.finalize_and_serialize().unwrap();
		let expected_serialized_tx = hex_literal::hex!("019934f0927bb3344080fc333956498280e7ff8959d7ad93e9f894cab5df74223752c3e6fc3607fec8b0a266d36a10b85bf3b9e4ab97f8204924130407c991690c0100070bf79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d19231e9528aae784fecbbd0bee129d9539c57be0e90061af6b6f4a5e274654e5bd47417da8b99d7748127a76b03d61fee69c80dfef73ad2d5503737beedc5a9ed4800000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517187bd16635dad40455fdc2c0c124c68f215675a5dbbacb5f0800000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea94000000e14940a2247d0a8a33650d7dfe12d269ecabce61c1219b5a6dcdb6961026e0972b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293ca73bdf31e341218a693b8772c43ecfcecd4cf35fada09a87ea0f860d028168e5c27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890005040301070004040000000500090340420f000000000005000502e0930400040200020c0200000000ca9a3b0000000009070800020304060a347d050be38042e0b20100000014000000ffffffffffffffffffffffffffffffffffffffff040000007c1d0f0700ca9a3b00000000").to_vec();
		// println!("{:?}", hex::encode(serialized_tx.clone()));

		assert_eq!(serialized_tx, expected_serialized_tx);
		assert!(serialized_tx.len() <= MAX_TRANSACTION_LENGTH)
	}

	#[test]
	fn create_ccm_token_transfer() {
		let durable_nonce = TEST_DURABLE_NONCE.into();
		let agg_key_keypair = SolSigningKey::from_bytes(&RAW_KEYPAIR).unwrap();
		let agg_key_pubkey = agg_key_keypair.pubkey();
		let amount = TRANSFER_AMOUNT;
		let token_mint_pubkey = USDC_TOKEN_MINT_PUB_KEY;
		let extra_accounts = ccm_accounts();
		let ccm_parameter = ccm_parameter();

		let to_pubkey = TRANSFER_TO_ACCOUNT;
		let to_pubkey_ata = derive_associated_token_account(to_pubkey, token_mint_pubkey).unwrap();

		let instructions = [
			SystemProgramInstruction::advance_nonce_account(
				&NONCE_ACCOUNTS[0].into(),
				&agg_key_pubkey,
			),
			ComputeBudgetInstruction::set_compute_unit_price(COMPUTE_UNIT_PRICE),
			ComputeBudgetInstruction::set_compute_unit_limit(COMPUTE_UNIT_LIMIT),
			AssociatedTokenAccountInstruction::create_associated_token_account_idempotent_instruction(
				&agg_key_pubkey,
				&to_pubkey.into(),
				&token_mint_pubkey.into(),
				&to_pubkey_ata.address.into(),
			),
			VaultProgram::with_id(VAULT_PROGRAM).transfer_tokens(
				amount,
				SOL_USDC_DECIMAL,
				VAULT_PROGRAM_DATA_ACCOUNT,
				agg_key_pubkey,
				TOKEN_VAULT_PDA_ACCOUNT,
				USDC_TOKEN_VAULT_ASSOCIATED_TOKEN_ACCOUNT,
				to_pubkey_ata.address,
				USDC_TOKEN_MINT_PUB_KEY,
				TOKEN_PROGRAM_ID,
			),
			VaultProgram::with_id(VAULT_PROGRAM).execute_ccm_token_call(
				ccm_parameter.source_chain as u32,
				ForeignChainAddress::to_source_address(ccm_parameter.source_address.unwrap()),
				ccm_parameter.channel_metadata.message.to_vec(),
				amount,
				VAULT_PROGRAM_DATA_ACCOUNT,
				agg_key_pubkey,
				to_pubkey_ata.address,
				extra_accounts.clone().cf_receiver,
				TOKEN_PROGRAM_ID,
				USDC_TOKEN_MINT_PUB_KEY,
				SYS_VAR_INSTRUCTIONS,
			).with_remaining_accounts(extra_accounts.remaining_account_metas()),
		];
		let message =
			Message::new_with_blockhash(&instructions, Some(&agg_key_pubkey), &durable_nonce);
		let mut tx = Transaction::new_unsigned(message);
		tx.sign(&[&agg_key_keypair], durable_nonce);
		// println!("{:?}", tx);

		let serialized_tx = tx.finalize_and_serialize().unwrap();
		let expected_serialized_tx = hex_literal::hex!("01b129476ffae4b80e116ceb457e9da19236c663373bc52d4e7cb5973429fb6157f74ac71e3168a286d7df90a1e259872cb64db6ee84fd6b44d504f529a5e8ea0c01000c11f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d1925ec7baaea7200eb2a66ccd361ee73bc87a7e5222ecedcbc946e97afb59ec46167417da8b99d7748127a76b03d61fee69c80dfef73ad2d5503737beedc5a9ed48e91372b3d301c202a633da0a92365a736e462131aecfad1fac47322cf8863ada00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517187bd16635dad40455fdc2c0c124c68f215675a5dbbacb5f0800000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000006ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a90e14940a2247d0a8a33650d7dfe12d269ecabce61c1219b5a6dcdb6961026e090fb9ba52b1f09445f1e3a7508d59f0797923acf744fbe2da303fb06da859ee8731e9528aae784fecbbd0bee129d9539c57be0e90061af6b6f4a5e274654e5bd472b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293c8c97258f4e2489f1bb3d1029148e0d830b5a1399daff1084048e7bd8dbe9f859a73bdf31e341218a693b8772c43ecfcecd4cf35fada09a87ea0f860d028168e5ab1d2a644046552e73f4d05b5a6ef53848973a9ee9febba42ddefb034b5f5130c27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e4890006050301080004040000000600090340420f000000000006000502e09304000e0600020c0b050901010d070a001004020b091136b4eeaf4a557ebc00ca9a3b00000000060d080a000203090b070f346cb8a27b9fdeaa230100000014000000ffffffffffffffffffffffffffffffffffffffff040000007c1d0f0700ca9a3b00000000").to_vec();

		// println!("{:?}", hex::encode(serialized_tx.clone()));

		assert_eq!(serialized_tx, expected_serialized_tx);
		assert!(serialized_tx.len() <= MAX_TRANSACTION_LENGTH)
	}

	#[test]
	fn create_idempotent_associated_token_account() {
		let durable_nonce = Hash::from_str("3GY33ibbFkTSdXeXuPAh2NxGTwm1TfEFNKKG9XjxFa67").unwrap();
		let agg_key_keypair = SolSigningKey::from_bytes(&RAW_KEYPAIR).unwrap();
		let agg_key_pubkey = agg_key_keypair.pubkey();

		// This is needed to derive the pda_ata to create the
		// createAssociatedTokenAccountIdempotentInstruction but for now we just derive it manually
		let to = Pubkey::from_str("pyq7ySiH5RvKteu2vdXKC7SNyNDp9vNDkGXdHxSpPtu").unwrap();
		let to_ata = Pubkey::from_str("EbarLzqEb9jf2ZHUdDf5nuBP52Ut3ddLZtYrGwKh3Bbd").unwrap();
		let mint_pubkey = Pubkey::from_str("21ySx9qZoscVT8ViTZjcudCCJeThnXfLPe1sLvezqRCv").unwrap();

		// This would lack the idempotent account creating but that's fine for the test
		let instructions = [
			SystemProgramInstruction::advance_nonce_account(
				&NONCE_ACCOUNTS[0].into(),
				&agg_key_pubkey,
			),
			AssociatedTokenAccountInstruction::create_associated_token_account_idempotent_instruction(
				&agg_key_pubkey,
				&to,
				&mint_pubkey,
				&to_ata
			),
		];
		let message =
			Message::new_with_blockhash(&instructions, Some(&agg_key_pubkey), &durable_nonce);
		let mut tx = Transaction::new_unsigned(message);
		tx.sign(&[&agg_key_keypair], durable_nonce);

		let serialized_tx = tx.finalize_and_serialize().unwrap();
		let expected_serialized_tx = hex_literal::hex!("01eb287ff9329fbaf83592ec56709d52d3d7f7edcab7ab53fc8371acff871016c51dfadde692630545a91d6534095bb5697b5fb9ee17dc292552eabf9ab6e3390601000609f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d192ca03f3e6d6fd79aaf8ebd4ce053492a34f22d0edafbfa88a380848d9a4735150000000000000000000000000000000000000000000000000000000000000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000006ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a90c4a8e3702f6e26d9d0c900c1461da4e3debef5743ce253bb9f0308a68c944220f1b83220b1108ea0e171b5391e6c0157370c8353516b74e962f855be6d787038c97258f4e2489f1bb3d1029148e0d830b5a1399daff1084048e7bd8dbe9f85921b22d7dfc8cdeba6027384563948d038a11eba06289de51a15c3d649d1f7e2c020303010400040400000008060002060703050101").to_vec();

		assert_eq!(serialized_tx, expected_serialized_tx);
		assert!(serialized_tx.len() <= MAX_TRANSACTION_LENGTH)
	}

	// Test taken from https://docs.rs/solana-sdk/latest/src/solana_sdk/transaction/mod.rs.html#1354
	// using current serialization (bincode::serde::encode_to_vec) and ensure that it's correct
	fn create_sample_transaction() -> Transaction {
		let keypair = SolSigningKey::from_bytes(&[
			255, 101, 36, 24, 124, 23, 167, 21, 132, 204, 155, 5, 185, 58, 121, 75, 156, 227, 116,
			193, 215, 38, 142, 22, 8, 14, 229, 239, 119, 93, 5, 218,
		])
		.unwrap();
		let to = Pubkey::from([
			1, 1, 1, 4, 5, 6, 7, 8, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 8, 7, 6, 5, 4,
			1, 1, 1,
		]);

		let program_id = Pubkey::from([
			2, 2, 2, 4, 5, 6, 7, 8, 9, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 9, 8, 7, 6, 5, 4,
			2, 2, 2,
		]);
		let account_metas =
			vec![AccountMeta::new(keypair.pubkey(), true), AccountMeta::new(to, false)];
		let instruction =
			Instruction::new_with_bincode(program_id, &(1u8, 2u8, 3u8), account_metas);
		let message = Message::new(&[instruction], Some(&keypair.pubkey()));
		let mut tx: Transaction = Transaction::new_unsigned(message);
		tx.sign(&[&keypair], Hash::default());
		tx
	}

	#[test]
	fn test_sdk_serialize() {
		let tx = create_sample_transaction();
		let serialized_tx = tx.finalize_and_serialize().unwrap();
		// SDK uses serde::serialize instead, but looks like this works.

		assert_eq!(
			serialized_tx,
			vec![
				1, 120, 138, 162, 185, 59, 209, 241, 157, 71, 157, 74, 131, 4, 87, 54, 28, 38, 180,
				222, 82, 64, 62, 61, 62, 22, 46, 17, 203, 187, 136, 62, 43, 11, 38, 235, 17, 239,
				82, 240, 139, 130, 217, 227, 214, 9, 242, 141, 223, 94, 29, 184, 110, 62, 32, 87,
				137, 63, 139, 100, 221, 20, 137, 4, 5, 1, 0, 1, 3, 36, 100, 158, 252, 33, 161, 97,
				185, 62, 89, 99, 195, 250, 249, 187, 189, 171, 118, 241, 90, 248, 14, 68, 219, 231,
				62, 157, 5, 142, 27, 210, 117, 1, 1, 1, 4, 5, 6, 7, 8, 9, 9, 9, 9, 9, 9, 9, 9, 9,
				9, 9, 9, 9, 9, 9, 9, 8, 7, 6, 5, 4, 1, 1, 1, 2, 2, 2, 4, 5, 6, 7, 8, 9, 1, 1, 1, 1,
				1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 9, 8, 7, 6, 5, 4, 2, 2, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0,
				0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 2, 0, 1,
				3, 1, 2, 3
			]
		);
	}

	// These tests can be used to manually serialize a transaction from a Solana Transaction in
	// storage, for instance if a transaction has failed to be broadcasted. While this won't be
	// necessary after PR#5229 it might be needed before that if we need to debug and/or manually
	// broadcast a transaction. The serialized output (hex string) can be broadcasted via the Solana
	// RPC call `sendTransaction` or the web3js `sendRawTransaction.
	// The Transaction values to serialize are obtained from querying storage element
	// solanaBroadcaster < pendingApiCalls. The signature of the transaction is what in storage is
	// named `transactionOutId`, since in Solana the signature is the transaction identifier.
	// The test parameters are from a localnet run where we get both the Transaction and the
	// resulting serialized transaction so we can compare that this serialization matches the
	// successful one.
	#[ignore]
	#[test]
	fn test_encode_tx() {
		let tx: Transaction = Transaction {
			signatures: vec![
				SolSignature(hex_literal::hex!(
					"d1144b223b6b600de4b2d96bdceb03573a3e9781953e4c668c57e505f017859d96543243b4d904dc2f02f2f5ab5db7ba4551c7e015e64078add4674ac2e7460c"
				)),
			],
			message: Message {
				header: MessageHeader {
					num_required_signatures: 1,
					num_readonly_signed_accounts: 0,
					num_readonly_unsigned_accounts: 8,
				},
				account_keys: vec![
					Pubkey(hex_literal::hex!(
						"2e8944a76efbece296221e736627f4528a947578263a1172a9786410702d2ef2"
					)),
					Pubkey(hex_literal::hex!(
						"22020a74fd97df45db96d2bbf4e485ccbec56945155ff8f668856be26c9de4a9"
					)),
					Pubkey(hex_literal::hex!(
						"79c03bceb9ddea819e956b2b332e87fbbf49fc8968df78488e88cfaa366f3036"
					)),
					Pubkey(hex_literal::hex!(
						"8cd28baa84f2067bbdf24513c2d44e44bf408f2e6da6e60762e3faa4a62a0adb"
					)),
					Pubkey(hex_literal::hex!(
						"8d9871ed5fb2ee05765af23b7cabcc0d6b08ed370bb9f616a0d4dea40a25f870"
					)),
					Pubkey(hex_literal::hex!(
						"b5b9d633289c8fd72fb05f33349bf4cc44e82add5d865311ae346d7c9a67b7dd"
					)),
					Pubkey(hex_literal::hex!(
						"f53a2f4350451db5595a75e231519bc2758798f72550e57487722e7cbe954dbc"
					)),
					Pubkey(hex_literal::hex!(
						"0000000000000000000000000000000000000000000000000000000000000000"
					)),
					Pubkey(hex_literal::hex!(
						"0306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a40000000"
					)),
					Pubkey(hex_literal::hex!(
						"06a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea9400000"
					)),
					Pubkey(hex_literal::hex!(
						"06ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a9"
					)),
					Pubkey(hex_literal::hex!(
						"0fb9ba52b1f09445f1e3a7508d59f0797923acf744fbe2da303fb06da859ee87"
					)),
					Pubkey(hex_literal::hex!(
						"72b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293c"
					)),
					Pubkey(hex_literal::hex!(
						"a140fd3d05766f0087d57bf99df05731e894392ffcc8e8d7e960ba73c09824aa"
					)),
					Pubkey(hex_literal::hex!(
						"a1e031c8bc9bec3b610cf7b36eb3bf3aa40237c9e5be2c7893878578439eb00b"
					)),
				],
				recent_blockhash: SolHash(hex_literal::hex!(
					"f7f02ac4729abaa97c01aa6526ba909c3bcb16c7f47c7e13dfdc5a1b15f647b4"
				))
				.into(),
				instructions: vec![
					CompiledInstruction {
						program_id_index: 7,
						accounts: hex_literal::hex!("030900").to_vec(),
						data: hex_literal::hex!("04000000").to_vec(),
					},
					CompiledInstruction {
						program_id_index: 8,
						accounts: vec![],
						data: hex_literal::hex!("030a00000000000000").to_vec(),
					},
					CompiledInstruction {
						program_id_index: 8,
						accounts: vec![],
						data: hex_literal::hex!("0233620100").to_vec(),
					},
					CompiledInstruction {
						program_id_index: 12,
						accounts: hex_literal::hex!("0e00040507").to_vec(),
						data: hex_literal::hex!("8e24658f6c59298c080000000100000000000000ff").to_vec(),
					},
					CompiledInstruction {
						program_id_index: 12,
						accounts: hex_literal::hex!("0e000d01020b0a0607").to_vec(),
						data: hex_literal::hex!("494710642cb0c646080000000200000000000000ff06").to_vec(),
					},
				],
			},
		};

		let serialized_tx = tx.finalize_and_serialize().unwrap();
		let expected_serialized_tx = hex_literal::hex!("01d1144b223b6b600de4b2d96bdceb03573a3e9781953e4c668c57e505f017859d96543243b4d904dc2f02f2f5ab5db7ba4551c7e015e64078add4674ac2e7460c0100080f2e8944a76efbece296221e736627f4528a947578263a1172a9786410702d2ef222020a74fd97df45db96d2bbf4e485ccbec56945155ff8f668856be26c9de4a979c03bceb9ddea819e956b2b332e87fbbf49fc8968df78488e88cfaa366f30368cd28baa84f2067bbdf24513c2d44e44bf408f2e6da6e60762e3faa4a62a0adb8d9871ed5fb2ee05765af23b7cabcc0d6b08ed370bb9f616a0d4dea40a25f870b5b9d633289c8fd72fb05f33349bf4cc44e82add5d865311ae346d7c9a67b7ddf53a2f4350451db5595a75e231519bc2758798f72550e57487722e7cbe954dbc00000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000006ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a90fb9ba52b1f09445f1e3a7508d59f0797923acf744fbe2da303fb06da859ee8772b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293ca140fd3d05766f0087d57bf99df05731e894392ffcc8e8d7e960ba73c09824aaa1e031c8bc9bec3b610cf7b36eb3bf3aa40237c9e5be2c7893878578439eb00bf7f02ac4729abaa97c01aa6526ba909c3bcb16c7f47c7e13dfdc5a1b15f647b40507030309000404000000080009030a0000000000000008000502336201000c050e00040507158e24658f6c59298c080000000100000000000000ff0c090e000d01020b0a060716494710642cb0c646080000000200000000000000ff06").to_vec();
		assert_eq!(serialized_tx, expected_serialized_tx);
	}

	#[ignore]
	#[test]
	fn test_encode_tx_fetch() {
		let tx: Transaction = Transaction {
			signatures: vec![
				SolSignature(hex_literal::hex!(
					"94b38e57e31dc130acdec802f60b2095b72916a44834f8b0a40b7e4949661c9e4e05aa3fa5a3dc3e285c8d16c8eaab079d4477daa76e9e4a1915603eda58bc0c"
				)),
			],
			message: Message {
				header: MessageHeader {
					num_required_signatures: 1,
					num_readonly_signed_accounts: 0,
					num_readonly_unsigned_accounts: 9,
				},
				account_keys: vec![
					Pubkey(hex_literal::hex!(
						"2e8944a76efbece296221e736627f4528a947578263a1172a9786410702d2ef2"
					)),
					Pubkey(hex_literal::hex!(
						"114f68f4ee9add615457c9a7791269b4d4ab3168d43d5da0e018e2d547d8be92"
					)),
					Pubkey(hex_literal::hex!(
						"287f3b39b93c6699d704cb3d3edcf633cb8068010c5e5f6e64583078f5cd370e"
					)),
					Pubkey(hex_literal::hex!(
						"3e1cb8c1bfc20346cebcaa28a53b234acf92771f72151b2d6aaa1d765be4b93c"
					)),
					Pubkey(hex_literal::hex!(
						"45f3121cddc0bab152917a22710c9fab5be66d121bf2474d4d484f0f2eed9780"
					)),
					Pubkey(hex_literal::hex!(
						"4813c8373d2bfc1592855e2d93b70ecd407fe9338b11ff0bb10650716709f6a7"
					)),
					Pubkey(hex_literal::hex!(
						"491102d3be1d348108b41a801904392e50cd5b443a0991f3c1db0427634627da"
					)),
					Pubkey(hex_literal::hex!(
						"5d89a80ca1700def3a784b845b59f9c2a61bb07941ddcb4fd2d709c3243c1350"
					)),
					Pubkey(hex_literal::hex!(
						"79c03bceb9ddea819e956b2b332e87fbbf49fc8968df78488e88cfaa366f3036"
					)),
					Pubkey(hex_literal::hex!(
						"c9b5b17535d2dcb7a1a505fbadc9ea27cddada4b7c144e549cf880e8db046d77"
					)),
					Pubkey(hex_literal::hex!(
						"ca586493b85289057a8661f9f2a81e546fcf8cc6f5c9df1f5441c822f6fabfc9"
					)),
					Pubkey(hex_literal::hex!(
						"e392cd98d3284fd551604be95c14cc8e20123e2940ef9fb784e6b591c7442864"
					)),
					Pubkey(hex_literal::hex!(
						"efe57cc00ff8edda422ba876d38f5905694bfbef1c35deaea90295968dc13339"
					)),
					Pubkey(hex_literal::hex!(
						"0000000000000000000000000000000000000000000000000000000000000000"
					)),
					Pubkey(hex_literal::hex!(
						"0306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a40000000"
					)),
					Pubkey(hex_literal::hex!(
						"06a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea9400000"
					)),
					Pubkey(hex_literal::hex!(
						"06ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a9"
					)),
					Pubkey(hex_literal::hex!(
						"0fb9ba52b1f09445f1e3a7508d59f0797923acf744fbe2da303fb06da859ee87"
					)),
					Pubkey(hex_literal::hex!(
						"2b635a1da73cd5bf15a26f1170f49366f0f48d28b0a8b1cebc5f98c75e475e68"
					)),
					Pubkey(hex_literal::hex!(
						"42be1bb8dfd763b0e83541c9767712ad0d89cecea13b46504370096a20c762fb"
					)),
					Pubkey(hex_literal::hex!(
						"72b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293c"
					)),
					Pubkey(hex_literal::hex!(
						"a1e031c8bc9bec3b610cf7b36eb3bf3aa40237c9e5be2c7893878578439eb00b"
					)),
				],
				recent_blockhash: SolHash(hex_literal::hex!(
					"9a5e41fc2cbe01a629ce980d5c6aa9c0a8b7be9d83ac835586feba35181d4246"
				))
				.into(),
				instructions: vec![
					CompiledInstruction {
						program_id_index: 13,
						accounts: hex_literal::hex!("0b0f00").to_vec(),
						data: hex_literal::hex!("04000000").to_vec(),
					},
					CompiledInstruction {
						program_id_index: 14,
						accounts: vec![],
						data: hex_literal::hex!("030a00000000000000").to_vec(),
					},
					CompiledInstruction {
						program_id_index: 14,
						accounts: vec![],
						data: hex_literal::hex!("02a7190300").to_vec(),
					},
					CompiledInstruction {
						program_id_index: 20,
						accounts: hex_literal::hex!("150012090811100a0d").to_vec(),
						data: hex_literal::hex!("494710642cb0c646080000001e00000000000000fd06").to_vec(),
					},
					CompiledInstruction {
						program_id_index: 20,
						accounts: hex_literal::hex!("150003010d").to_vec(),
						data: hex_literal::hex!("8e24658f6c59298c080000001400000000000000fd").to_vec(),
					},
					CompiledInstruction {
						program_id_index: 20,
						accounts: hex_literal::hex!("1500130c081110020d").to_vec(),
						data: hex_literal::hex!("494710642cb0c646080000000e00000000000000ff06").to_vec(),
					},
					CompiledInstruction {
						program_id_index: 20,
						accounts: hex_literal::hex!("150004060d").to_vec(),
						data: hex_literal::hex!("8e24658f6c59298c080000000f00000000000000fb").to_vec(),
					},
					CompiledInstruction {
						program_id_index: 20,
						accounts: hex_literal::hex!("150007050d").to_vec(),
						data: hex_literal::hex!("8e24658f6c59298c080000000500000000000000fe").to_vec(),
					},
				],
			},
		};

		let serialized_tx = tx.finalize_and_serialize().unwrap();
		let expected_serialized_tx = hex_literal::hex!("0194b38e57e31dc130acdec802f60b2095b72916a44834f8b0a40b7e4949661c9e4e05aa3fa5a3dc3e285c8d16c8eaab079d4477daa76e9e4a1915603eda58bc0c010009162e8944a76efbece296221e736627f4528a947578263a1172a9786410702d2ef2114f68f4ee9add615457c9a7791269b4d4ab3168d43d5da0e018e2d547d8be92287f3b39b93c6699d704cb3d3edcf633cb8068010c5e5f6e64583078f5cd370e3e1cb8c1bfc20346cebcaa28a53b234acf92771f72151b2d6aaa1d765be4b93c45f3121cddc0bab152917a22710c9fab5be66d121bf2474d4d484f0f2eed97804813c8373d2bfc1592855e2d93b70ecd407fe9338b11ff0bb10650716709f6a7491102d3be1d348108b41a801904392e50cd5b443a0991f3c1db0427634627da5d89a80ca1700def3a784b845b59f9c2a61bb07941ddcb4fd2d709c3243c135079c03bceb9ddea819e956b2b332e87fbbf49fc8968df78488e88cfaa366f3036c9b5b17535d2dcb7a1a505fbadc9ea27cddada4b7c144e549cf880e8db046d77ca586493b85289057a8661f9f2a81e546fcf8cc6f5c9df1f5441c822f6fabfc9e392cd98d3284fd551604be95c14cc8e20123e2940ef9fb784e6b591c7442864efe57cc00ff8edda422ba876d38f5905694bfbef1c35deaea90295968dc1333900000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000006ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a90fb9ba52b1f09445f1e3a7508d59f0797923acf744fbe2da303fb06da859ee872b635a1da73cd5bf15a26f1170f49366f0f48d28b0a8b1cebc5f98c75e475e6842be1bb8dfd763b0e83541c9767712ad0d89cecea13b46504370096a20c762fb72b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293ca1e031c8bc9bec3b610cf7b36eb3bf3aa40237c9e5be2c7893878578439eb00b9a5e41fc2cbe01a629ce980d5c6aa9c0a8b7be9d83ac835586feba35181d4246080d030b0f0004040000000e0009030a000000000000000e000502a71903001409150012090811100a0d16494710642cb0c646080000001e00000000000000fd061405150003010d158e24658f6c59298c080000001400000000000000fd14091500130c081110020d16494710642cb0c646080000000e00000000000000ff061405150004060d158e24658f6c59298c080000000f00000000000000fb1405150007050d158e24658f6c59298c080000000500000000000000fe").to_vec();
		assert_eq!(serialized_tx, expected_serialized_tx);
	}
}
