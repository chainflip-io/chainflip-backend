use core::str::FromStr;

use borsh::{BorshDeserialize, BorshSerialize};
use generic_array::{typenum::U64, GenericArray};
use serde::{Deserialize, Serialize};
use sp_std::{collections::btree_map::BTreeMap, vec, vec::Vec};

#[cfg(test)]
use extra_types_for_testing::{SignerError, Signers, TransactionError};
#[cfg(test)]
pub mod extra_types_for_testing;
#[cfg(test)]
use thiserror::Error;

use self::program_instructions::SystemProgramInstruction;

pub mod program_instructions;
pub mod short_vec;

pub const SIGNATURE_BYTES: usize = 64;
pub const HASH_BYTES: usize = 32;
/// Maximum string length of a base58 encoded pubkey
const MAX_BASE58_LEN: usize = 44;
pub const SYSTEM_PROGRRAM_ID: &str = "11111111111111111111111111111111";
pub const VAULT_PROGRAM: &str = "632bJHVLPj6XPLVgrabFwxogtAQQ5zb8hwm9zqZuCcHo";
pub const SYS_VAR_RECENT_BLOCKHASHES: &str = "SysvarRecentB1ockHashes11111111111111111111";

/// An atomically-commited sequence of instructions.
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
#[derive(Debug, PartialEq, Default, Eq, Clone, Serialize, Deserialize)]
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
	pub signatures: Vec<Signature>,

	/// The message to sign.
	pub message: Message,
}

impl Transaction {
	pub fn new_unsigned(message: Message) -> Self {
		Self {
			signatures: vec![Signature::default(); message.header.num_required_signatures as usize],
			message,
		}
	}

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
				.for_each(|signature| *signature = Signature::default());
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

	#[cfg(test)]
	pub fn is_signed(&self) -> bool {
		self.signatures.iter().all(|signature| *signature != Signature::default())
	}

	/// Return the message containing all data that should be signed.
	#[cfg(test)]
	pub fn message(&self) -> &Message {
		&self.message
	}

	/// Return the serialized message data to sign.
	#[cfg(test)]
	pub fn message_data(&self) -> Vec<u8> {
		self.message().serialize()
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
		// the solana-sdk uses bincode version 1.3.3 which has a dependency on serde which depends on std and so it cannot be used with our runtime. Fortunately, the new version of bincode (bincode 2) has an optional dependency on serde and we can use the serializer without serde. However, bincode 2 is a complete rewrite of bincode 1 and so to mimic the exact bahaviour of serializaition that is used by the solana-sdk with bincode 1, we need to use the legacy config for serialization according to the migration guide provided by bincode here: https://github.com/bincode-org/bincode/blob/v2.0.0-rc.3/docs/migration_guide.md. Original serialization call in solana sdk: let data = bincode::serialize(data).unwrap();
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
#[derive(Serialize, Deserialize, Default, Debug, PartialEq, Eq, Clone, Copy)]
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
#[derive(Serialize, Deserialize, Default, Debug, PartialEq, Eq, Clone)]
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
	pub fn new(instructions: &[Instruction], payer: Option<&Pubkey>) -> Self {
		Self::new_with_blockhash(instructions, payer, &Hash::default())
	}

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

	pub fn new_with_nonce(
		mut instructions: Vec<Instruction>,
		payer: Option<&Pubkey>,
		nonce_account_pubkey: &Pubkey,
		nonce_authority_pubkey: &Pubkey,
	) -> Self {
		let nonce_ix = SystemProgramInstruction::advance_nonce_account(
			nonce_account_pubkey,
			nonce_authority_pubkey,
		);
		instructions.insert(0, nonce_ix);
		Self::new(&instructions, payer)
	}

	pub fn new_with_compiled_instructions(
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

	#[cfg(test)]
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
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
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

#[derive(Debug, PartialEq, Default, Eq, Clone, Serialize, Deserialize, Ord, PartialOrd, Copy)]
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

#[derive(Debug, PartialEq, Default, Eq, Clone, Serialize, Deserialize, Copy)]
pub struct Signature(GenericArray<u8, U64>);

// impl Signature {
// 	pub(self) fn verify_verbose(
// 		&self,
// 		pubkey_bytes: &[u8],
// 		message_bytes: &[u8],
// 	) -> Result<(), ed25519_dalek::SignatureError> {
// 		let publickey = ed25519_dalek::PublicKey::from_bytes(pubkey_bytes)?;
// 		let signature = self.0.as_slice().try_into()?;
// 		publickey.verify_strict(message_bytes, &signature)
// 	}

// 	pub fn verify(&self, pubkey_bytes: &[u8], message_bytes: &[u8]) -> bool {
// 		self.verify_verbose(pubkey_bytes, message_bytes).is_ok()
// 	}
// }

impl From<[u8; SIGNATURE_BYTES]> for Signature {
	fn from(signature: [u8; SIGNATURE_BYTES]) -> Self {
		Self(GenericArray::from(signature))
	}
}

#[derive(
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

#[cfg(test)]
mod tests {

	use core::str::FromStr;

	use crate::sol::{
		program_instructions::{SystemProgramInstruction, VaultProgram},
		BorshDeserialize, BorshSerialize, SYSTEM_PROGRRAM_ID,
	};

	use super::{
		extra_types_for_testing::{Keypair, Signer},
		AccountMeta, Hash, Instruction, Message, Pubkey, Transaction,
	};

	#[derive(BorshSerialize, BorshDeserialize)]
	enum BankInstruction {
		Initialize,
		Deposit { lamports: u64 },
		Withdraw { lamports: u64 },
	}

	const RAW_KEYPAIR: [u8; 64] = [
		6, 151, 150, 20, 145, 210, 176, 113, 98, 200, 192, 80, 73, 63, 133, 232, 208, 124, 81, 213,
		117, 199, 196, 243, 219, 33, 79, 217, 157, 69, 205, 140, 247, 157, 94, 2, 111, 18, 237,
		198, 68, 58, 83, 75, 44, 221, 80, 114, 35, 57, 137, 180, 21, 215, 89, 101, 115, 231, 67,
		243, 229, 179, 134, 251,
	];

	#[test]
	fn create_simple_tx() {
		fn send_initialize_tx(program_id: Pubkey, payer: &Keypair) -> Result<(), ()> {
			let bank_instruction = BankInstruction::Initialize;

			let instruction = Instruction::new_with_borsh(program_id, &bank_instruction, vec![]);

			let mut tx = Transaction::new_with_payer(&[instruction], Some(&payer.pubkey()));
			//let blockhash = client.get_latest_blockhash()?;
			tx.sign(&[payer], Default::default());
			println!("tx:{:?}", tx);
			Ok(())
		}

		// let client = RpcClient::new(String::new());
		let program_id = Pubkey([0u8; 32]);
		let payer = Keypair::new();
		let _ = send_initialize_tx(program_id, &payer);
	}

	#[test]
	fn create_nonced_transfer() {
		let durable_nonce = Hash::from_str("F5HaggF8o2jESnoFi7sSdgy2qhz4amp3miev144Cfp49").unwrap();
		let from_keypair = Keypair::from_bytes(&RAW_KEYPAIR).unwrap();
		let from_pubkey = from_keypair.pubkey();
		let nonce_account_pubkey =
			Pubkey::from_str("2cNMwUCF51djw2xAiiU54wz1WrU8uG4Q8Kp8nfEuwghw").unwrap();
		let to_pubkey = Pubkey::from_str("4MqL4qy2W1yXzuF3PiuSMehMbJzMuZEcBwVvrgtuhx7V").unwrap();
		let instructions = [
			SystemProgramInstruction::advance_nonce_account(&nonce_account_pubkey, &from_pubkey),
			SystemProgramInstruction::transfer(&from_pubkey, &to_pubkey, 1000000000),
		];
		let message = Message::new(&instructions, Some(&from_pubkey));
		let mut tx = Transaction::new_unsigned(message);
		tx.sign(&[&from_keypair], durable_nonce);

		let serialized_tx = bincode::serde::encode_to_vec(tx, bincode::config::legacy()).unwrap();
		let expected_serialized_tx = hex_literal::hex!("01b0c5753a71484e74a73f01e8a373cd2170285afa09ecf83174de8701a469d150e195cc24ad915024614932248d1f036823d814545d6475df814dfaa7f85bd20301000205f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d19231e9528aae784fecbbd0bee129d9539c57be0e90061af6b6f4a5e274654e5bd4000000000000000000000000000000000000000000000000000000000000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea9400000d11cb0294f1fde6725b37bc3f341f5083378cb8f543019218dba6f9d53e12a920203030104000404000000030200020c0200000000ca9a3b00000000").to_vec();

		assert_eq!(serialized_tx, expected_serialized_tx);
		println!("tx:{:?}", hex::encode(serialized_tx));
	}

	#[test]
	fn create_nonced_fetch() {
		let durable_nonce = Hash::from_str("GzgDQ7iaz34Atkc1Ziq6GPCgMaWNbHb8rMJe8QswhPqq").unwrap();
		let vault_account = Keypair::from_bytes(&RAW_KEYPAIR).unwrap();
		let vault_account_pubkey = vault_account.pubkey();
		let nonce_account_pubkey =
			Pubkey::from_str("2cNMwUCF51djw2xAiiU54wz1WrU8uG4Q8Kp8nfEuwghw").unwrap();
		let data_account_pubkey =
			Pubkey::from_str("5yhN4QzBFg9jKhLfVHcS5apMB7e3ftofCkzkNH6dZctC").unwrap();
		let pda = Pubkey::from_str("FezduHnFMTMTiZuWLgGwrbGmzeeRZytzDPQA2kF8Atqx").unwrap();

		let instructions = [
			SystemProgramInstruction::advance_nonce_account(
				&nonce_account_pubkey,
				&vault_account_pubkey,
			),
			VaultProgram::get_instruction(
				VaultProgram::FetchSol { seed: vec![1u8, 2u8, 3u8], bump: 255 },
				vec![
					AccountMeta::new_readonly(data_account_pubkey, false),
					AccountMeta::new_readonly(vault_account_pubkey, true),
					AccountMeta::new(pda, false),
					AccountMeta::new(vault_account_pubkey, false),
					AccountMeta::new_readonly(Pubkey::from_str(SYSTEM_PROGRRAM_ID).unwrap(), false),
				],
			),
		];
		let message = Message::new(&instructions, Some(&vault_account_pubkey));
		let mut tx = Transaction::new_unsigned(message);
		tx.sign(&[&vault_account], durable_nonce);
		println!("{:?}", tx);

		let serialized_tx = bincode::serde::encode_to_vec(tx, bincode::config::legacy()).unwrap();
		let expected_serialized_tx = hex_literal::hex!("0132f44cdc2295a63c3f5bdb46eafef39d12f405f102b79fb453107a90f58ed57a7c2e805e25ad26bf4e8bf74afb1b1442ef5ecb445d44b7f62fb2c53f11984a0401000407f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d192d9bf46c241ddb58f1c4141c19dee9a8b3cf064c11d6ba6524886e82a4e6a3d1b000000000000000000000000000000000000000000000000000000000000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000049f4e96507a68c8d673696ffd7e551091e62a0a603c6585b79d8707f807238654acf654557d0c27ec71e80b3ed7d0a6f7baa05717b5bf6060e6b9e6f5d3a5532eda5bfc90c0490edbe517064d3976dfb32d433e6162772be2127e11e2dfcdd4802030301040004040000000605050002000310bd9939fa08c006ec03000000010203ff").to_vec();
		println!("tx:{:?}", hex::encode(serialized_tx.clone()));

		assert_eq!(serialized_tx, expected_serialized_tx);
	}

	#[test]
	fn playground() {
		println!(
			"{:?}",
			hex::encode(
				VaultProgram::get_instruction(
					VaultProgram::FetchSol { seed: vec![1u8, 2u8, 3u8], bump: 255 },
					vec![],
				)
				.data
			)
		)
	}

	// Test taken from https://docs.rs/solana-sdk/latest/src/solana_sdk/transaction/mod.rs.html#1354
	// using current serialization (bincode::serde::encode_to_vec) and ensure that it's correct
	fn create_sample_transaction() -> Transaction {
        let keypair = Keypair::from_bytes(&[
            255, 101, 36, 24, 124, 23, 167, 21, 132, 204, 155, 5, 185, 58, 121, 75, 156, 227, 116,
            193, 215, 38, 142, 22, 8, 14, 229, 239, 119, 93, 5, 218, 36, 100, 158, 252, 33, 161,
            97, 185, 62, 89, 99, 195, 250, 249, 187, 189, 171, 118, 241, 90, 248, 14, 68, 219, 231,
            62, 157, 5, 142, 27, 210, 117,
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
        let account_metas = vec![
            AccountMeta::new(keypair.pubkey(), true),
            AccountMeta::new(to, false),
        ];
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
		let serialized_tx = bincode::serde::encode_to_vec(tx, bincode::config::legacy()).unwrap();
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
}

