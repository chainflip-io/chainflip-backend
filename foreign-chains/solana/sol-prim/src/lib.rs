#![cfg_attr(not(feature = "std"), no_std)]

pub use crate::{address::Address, digest::Digest, signature::Signature};
use codec::{Decode, Encode};
use core::str::FromStr;
use generic_array::{typenum::U64, GenericArray};
use scale_info::TypeInfo;

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use sp_std::{collections::btree_map::BTreeMap, vec::Vec};

#[macro_use]
mod macros;

#[cfg(feature = "pda")]
pub mod pda;

#[cfg(test)]
mod tests;

pub mod address_derivation;
pub mod alt;
pub mod consts;
pub mod errors;
pub mod instructions;
pub mod short_vec;
pub mod transaction;

pub use alt::*;
pub use instructions::*;

#[cfg(feature = "std")]
pub mod signer;

mod utils;

pub type Amount = u64;
pub type SlotNumber = u64;
pub type ComputeLimit = u32;
pub type AccountBump = u8;

use crate::consts::{HASH_BYTES, MAX_BASE58_LEN, SOLANA_SIGNATURE_LEN};

define_binary!(address, Address, crate::consts::SOLANA_ADDRESS_LEN, "A");
define_binary!(digest, Digest, crate::consts::SOLANA_DIGEST_LEN, "D");
define_binary!(signature, Signature, crate::consts::SOLANA_SIGNATURE_LEN, "S");

/// Represents a derived Associated Token Account to be used as deposit channels.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct PdaAndBump {
	pub address: Address,
	pub bump: AccountBump,
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
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TypeInfo)]
pub struct Instruction<Address = Pubkey> {
	/// Pubkey of the program that executes this instruction.
	pub program_id: Address,
	/// Metadata describing accounts that should be passed to the program.
	pub accounts: Vec<AccountMeta<Address>>,
	/// Opaque data passed to the program for its own interpretation.
	#[serde(with = "sp_core::bytes")]
	pub data: Vec<u8>,
}

/// Instruction type used when being presented to the end user.
/// Serializes addresses into bs58 format.
pub type InstructionRpc = Instruction<Address>;

impl From<Instruction> for InstructionRpc {
	fn from(value: Instruction) -> Self {
		InstructionRpc {
			program_id: value.program_id.into(),
			accounts: value.accounts.into_iter().map(|a| a.into()).collect(),
			data: value.data,
		}
	}
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
#[derive(
	Debug, Default, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TypeInfo,
)]
pub struct AccountMeta<Address = Pubkey> {
	/// An account's public key.
	pub pubkey: Address,
	/// True if an `Instruction` requires a `Transaction` signature matching `pubkey`.
	pub is_signer: bool,
	/// True if the account data or metadata may be mutated during program execution.
	pub is_writable: bool,
}

/// Type used to be presented to the user. Serializes address into bs58 string.
pub type AccountMetaRpc = AccountMeta<Address>;

impl From<AccountMeta> for AccountMetaRpc {
	fn from(value: AccountMeta) -> Self {
		AccountMetaRpc {
			pubkey: value.pubkey.into(),
			is_signer: value.is_signer,
			is_writable: value.is_writable,
		}
	}
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
pub struct CompiledKeys {
	payer: Option<Pubkey>,
	key_meta_map: BTreeMap<Pubkey, CompiledKeyMeta>,
}

impl CompiledKeys {
	/// Compiles the pubkeys referenced by a list of instructions and organizes by
	/// signer/non-signer and writable/readonly.
	pub fn compile(instructions: &[Instruction], payer: Option<Pubkey>) -> Self {
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

	pub fn try_into_message_components(self) -> Result<(MessageHeader, Vec<Pubkey>), CompileError> {
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

	pub fn try_extract_table_lookup(
		&mut self,
		lookup_table_account: &AddressLookupTableAccount,
	) -> Result<Option<(MessageAddressTableLookup, LoadedAddresses)>, CompileError> {
		let (writable_indexes, drained_writable_keys) = self
			.try_drain_keys_found_in_lookup_table(&lookup_table_account.addresses, |meta| {
				!meta.is_signer && !meta.is_invoked && meta.is_writable
			})?;
		let (readonly_indexes, drained_readonly_keys) = self
			.try_drain_keys_found_in_lookup_table(&lookup_table_account.addresses, |meta| {
				!meta.is_signer && !meta.is_invoked && !meta.is_writable
			})?;

		// Don't extract lookup if no keys were found
		if writable_indexes.is_empty() && readonly_indexes.is_empty() {
			return Ok(None);
		}

		Ok(Some((
			MessageAddressTableLookup {
				account_key: lookup_table_account.key,
				writable_indexes,
				readonly_indexes,
			},
			LoadedAddresses { writable: drained_writable_keys, readonly: drained_readonly_keys },
		)))
	}

	fn try_drain_keys_found_in_lookup_table(
		&mut self,
		lookup_table_addresses: &[Pubkey],
		key_meta_filter: impl Fn(&CompiledKeyMeta) -> bool,
	) -> Result<(Vec<u8>, Vec<Pubkey>), CompileError> {
		let mut lookup_table_indexes = Vec::new();
		let mut drained_keys = Vec::new();

		for search_key in self
			.key_meta_map
			.iter()
			.filter_map(|(key, meta)| key_meta_filter(meta).then_some(key))
		{
			for (key_index, key) in lookup_table_addresses.iter().enumerate() {
				if key == search_key {
					let lookup_table_index = u8::try_from(key_index)
						.map_err(|_| CompileError::AddressTableLookupIndexOverflow)?;

					lookup_table_indexes.push(lookup_table_index);
					drained_keys.push(*search_key);
					break;
				}
			}
		}

		for key in &drained_keys {
			self.key_meta_map.remove_entry(key);
		}

		Ok((lookup_table_indexes, drained_keys))
	}
}

#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct CompiledKeyMeta {
	is_signer: bool,
	is_writable: bool,
	is_invoked: bool,
}

pub fn position(keys: &[Pubkey], key: &Pubkey) -> u8 {
	keys.iter().position(|k| k == key).unwrap() as u8
}

pub fn compile_instruction(ix: &Instruction, keys: &[Pubkey]) -> CompiledInstruction {
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

pub fn compile_instructions(ixs: &[Instruction], keys: &[Pubkey]) -> Vec<CompiledInstruction> {
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

impl From<Address> for Pubkey {
	fn from(from: Address) -> Self {
		Self(from.0)
	}
}
impl From<Pubkey> for Address {
	fn from(from: Pubkey) -> Address {
		Address::from(from.0)
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
pub struct RawSignature(GenericArray<u8, U64>);

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

impl From<[u8; SOLANA_SIGNATURE_LEN]> for RawSignature {
	fn from(signature: [u8; SOLANA_SIGNATURE_LEN]) -> Self {
		Self(GenericArray::from(signature))
	}
}

impl From<Signature> for RawSignature {
	fn from(from: Signature) -> Self {
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
impl From<Digest> for Hash {
	fn from(from: Digest) -> Self {
		Self::from(from.0)
	}
}
impl From<Hash> for Digest {
	fn from(from: Hash) -> Digest {
		Digest::from(from.0)
	}
}

/// Used only for tests
impl FromStr for Hash {
	type Err = ();

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		if s.len() > MAX_BASE58_LEN {
			return Err(())
		}
		let bytes = bs58::decode(s).into_vec().map_err(|_| ())?;
		if bytes.len() != HASH_BYTES {
			Err(())
		} else {
			Ok(Hash::new(&bytes))
		}
	}
}
