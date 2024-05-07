use core::str::FromStr;

use borsh::{BorshDeserialize, BorshSerialize};
#[cfg(test)]
use ed25519_dalek;
use generic_array::{typenum::U64, GenericArray};
use serde::{Deserialize, Serialize};
use sp_std::{collections::btree_map::BTreeMap, vec, vec::Vec};

#[cfg(test)]
use super::extra_types_for_testing::{SignerError, Signers, TransactionError};
#[cfg(test)]
use thiserror::Error;

pub mod short_vec;

use super::program_instructions::SystemProgramInstruction;

pub const SIGNATURE_BYTES: usize = 64;
pub const HASH_BYTES: usize = 32;
/// Maximum string length of a base58 encoded pubkey
const MAX_BASE58_LEN: usize = 44;

// Solana native programs
pub const SYSTEM_PROGRAM_ID: &str = "11111111111111111111111111111111";
pub const TOKEN_PROGRAM_ID: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
pub const ASSOCIATED_TOKEN_PROGRAM_ID: &str = "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL";
pub const SYS_VAR_RECENT_BLOCKHASHES: &str = "SysvarRecentB1ockHashes11111111111111111111";
pub const SYS_VAR_INSTRUCTIONS: &str = "Sysvar1nstructions1111111111111111111111111";
pub const SYS_VAR_RENT: &str = "SysvarRent111111111111111111111111111111111";
pub const SYS_VAR_CLOCK: &str = "SysvarC1ock11111111111111111111111111111111";
pub const BPF_LOADER_UPGRADEABLE_ID: &str = "BPFLoaderUpgradeab1e11111111111111111111111";
pub const COMPUTE_BUDGET_PROGRAM: &str = "ComputeBudget111111111111111111111111111111";
pub const MAX_TRANSACTION_LENGTH: usize = 1_232;

// Chainflip addresses - can be constants on a per-chain basis inserted on runtime upgrade.
// Addresses and bumps can be derived from the seeds. However, for most of them there is no
// need to rederive them every time so we could include them as constants. The token vault
// ata's for each of the tokens can be derived on the fly but we might want to also store
// them as constants for simplicity. Initial nonce account hashes should probably also be
// inserted on chain genesis but won't be constant.
pub const VAULT_PROGRAM: &str = "8inHGLHXegST3EPLcpisQe9D1hDT9r7DJjS395L3yuYf";
pub const VAULT_PROGRAM_DATA_ADDRESS: &str = "3oEKmL4nsw6RDZWhkYTdCUmjxDrzVkm1cWayPsvn3p57";
pub const VAULT_PROGRAM_DATA_ACCOUNT: &str = "623nEsyGYWKYggY1yHxQFJiBarL9jdWdrMr7ASiCKP6a";
// MIN_PUB_KEY per supported spl-token
pub const MINT_PUB_KEY: &str = "24PNhTaNtomHhoy3fTRaMhAFCRj4uHqhZEEoWrKDbR5p";
pub const TOKEN_VAULT_PDA_ACCOUNT: &str = "9j17hjg8wR2uFxJAJDAFahwsgTCNx35sc5qXSxDmuuF6";
// This can be derived from the TOKEN_VAULT_PDA_ACCOUNT and the mintPubKey but we can have it stored
// There will be a different one per each supported spl-token
pub const TOKEN_VAULT_ASSOCIATED_TOKEN_ACCOUNT: &str =
	"DUjCLckPi4g7QAwBEwuFL1whpgY6L9fxwXnqbWvS2pcW";
pub const UPGRADE_MANAGER_PROGRAM: &str = "274BzCz5RPHJZsxdcSGySahz4qAWqwSDcmz1YEKkGaZC";
pub const UPGRADE_MANAGER_PROGRAM_DATA_ACCOUNT: &str =
	"CAGADTb6bdpm4L4esntbLQovDyg6Wutiot2DNkMR8wZa";
pub const UPGRADE_MANAGER_PDA_SIGNER_SEED: [u8; 6] = [115u8, 105u8, 103u8, 110u8, 101u8, 114u8];
pub const UPGRADE_MANAGER_PDA_SIGNER_BUMP: u8 = 255;
pub const UPGRADE_MANAGER_PDA_SIGNER: &str = "2SAhe89c1umM2JvCnmqCEnY8UCQtNPEKGe7UXA8KSQqH";
pub const NONCE_ACCOUNTS: [&str; 10] = [
	"2cNMwUCF51djw2xAiiU54wz1WrU8uG4Q8Kp8nfEuwghw",
	"HVG21SovGzMBJDB9AQNuWb6XYq4dDZ6yUwCbRUuFnYDo",
	"HDYArziNzyuNMrK89igisLrXFe78ti8cvkcxfx4qdU2p",
	"HLPsNyxBqfq2tLE31v6RiViLp2dTXtJRgHgsWgNDRPs2",
	"GKMP63TqzbueWTrFYjRwMNkAyTHpQ54notRbAbMDmePM",
	"EpmHm2aSPsB5ZZcDjqDhQ86h1BV32GFCbGSMuC58Y2tn",
	"9yBZNMrLrtspj4M7bEf2X6tqbqHxD2vNETw8qSdvJHMa",
	"J9dT7asYJFGS68NdgDCYjzU2Wi8uBoBusSHN1Z6JLWna",
	"GUMpVpQFNYJvSbyTtUarZVL7UDUgErKzDTSVJhekUX55",
	"AUiHYbzH7qLZSkb3u7nAqtvqC7e41sEzgWjBEvXrpfGv",
];

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
		// NOTE: the solana-sdk uses bincode version 1.3.3 which has a dependency on serde which
		// depends on std and so it cannot be used with our runtime. Fortunately, the new version of
		// bincode (bincode 2) has an optional dependency on serde and we can use the serializer
		// without serde. However, bincode 2 is a complete rewrite of bincode 1 and so to mimic the
		// exact bahaviour of serializaition that is used by the solana-sdk with bincode 1, we need
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

impl Signature {
	#[cfg(test)]
	pub(self) fn verify_verbose(
		&self,
		pubkey_bytes: &[u8],
		message_bytes: &[u8],
	) -> Result<(), ed25519_dalek::SignatureError> {
		let publickey = ed25519_dalek::PublicKey::from_bytes(pubkey_bytes)?;
		let signature = self.0.as_slice().try_into()?;
		publickey.verify_strict(message_bytes, &signature)
	}

	#[cfg(test)]
	pub fn verify(&self, pubkey_bytes: &[u8], message_bytes: &[u8]) -> bool {
		self.verify_verbose(pubkey_bytes, message_bytes).is_ok()
	}
}

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
		bpf_loader_instructions::set_upgrade_authority,
		compute_budget::ComputeBudgetInstruction,
		extra_types_for_testing::{Keypair, Signer},
		program_instructions::{
			ProgramInstruction, SystemProgramInstruction, UpgradeManagerProgram, VaultProgram,
		},
		sol_tx_building_blocks::{
			AccountMeta, BorshDeserialize, BorshSerialize, Hash, Instruction, Message, Pubkey,
			Transaction, BPF_LOADER_UPGRADEABLE_ID, MAX_TRANSACTION_LENGTH, MINT_PUB_KEY,
			NONCE_ACCOUNTS, SYSTEM_PROGRAM_ID, SYS_VAR_CLOCK, SYS_VAR_INSTRUCTIONS, SYS_VAR_RENT,
			TOKEN_PROGRAM_ID, TOKEN_VAULT_ASSOCIATED_TOKEN_ACCOUNT, TOKEN_VAULT_PDA_ACCOUNT,
			UPGRADE_MANAGER_PDA_SIGNER, UPGRADE_MANAGER_PDA_SIGNER_BUMP,
			UPGRADE_MANAGER_PDA_SIGNER_SEED, UPGRADE_MANAGER_PROGRAM_DATA_ACCOUNT, VAULT_PROGRAM,
			VAULT_PROGRAM_DATA_ACCOUNT, VAULT_PROGRAM_DATA_ADDRESS,
		},
		token_instructions::AssociatedTokenAccountInstruction,
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
			tx.sign(&[payer], Default::default());
			Ok(())
		}

		// let client = RpcClient::new(String::new());
		let program_id = Pubkey([0u8; 32]);
		let payer = Keypair::new();
		let _ = send_initialize_tx(program_id, &payer);
	}

	#[test]
	fn create_transfer_native() {
		let durable_nonce = Hash::from_str("F5HaggF8o2jESnoFi7sSdgy2qhz4amp3miev144Cfp49").unwrap();
		let agg_key_keypair = Keypair::from_bytes(&RAW_KEYPAIR).unwrap();
		let agg_key_pubkey = agg_key_keypair.pubkey();
		let to_pubkey = Pubkey::from_str("4MqL4qy2W1yXzuF3PiuSMehMbJzMuZEcBwVvrgtuhx7V").unwrap();
		let instructions = [
			SystemProgramInstruction::advance_nonce_account(
				&Pubkey::from_str(NONCE_ACCOUNTS[0]).unwrap(),
				&agg_key_pubkey,
			),
			SystemProgramInstruction::transfer(&agg_key_pubkey, &to_pubkey, 1000000000),
		];
		let message = Message::new(&instructions, Some(&agg_key_pubkey));
		let mut tx = Transaction::new_unsigned(message);
		tx.sign(&[&agg_key_keypair], durable_nonce);

		let serialized_tx = bincode::serde::encode_to_vec(tx, bincode::config::legacy()).unwrap();
		let expected_serialized_tx = hex_literal::hex!("01b0c5753a71484e74a73f01e8a373cd2170285afa09ecf83174de8701a469d150e195cc24ad915024614932248d1f036823d814545d6475df814dfaa7f85bd20301000205f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d19231e9528aae784fecbbd0bee129d9539c57be0e90061af6b6f4a5e274654e5bd4000000000000000000000000000000000000000000000000000000000000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea9400000d11cb0294f1fde6725b37bc3f341f5083378cb8f543019218dba6f9d53e12a920203030104000404000000030200020c0200000000ca9a3b00000000").to_vec();

		assert_eq!(serialized_tx, expected_serialized_tx);
		assert!(serialized_tx.len() <= MAX_TRANSACTION_LENGTH)
	}

	#[test]
	fn create_transfer_cu_priority_fees() {
		let durable_nonce = Hash::from_str("2GGxiEHwtWPGNKH5czvxRGvQTayRvCT1PFsA9yK2iMnq").unwrap();
		let agg_key_keypair = Keypair::from_bytes(&RAW_KEYPAIR).unwrap();
		let agg_key_pubkey = agg_key_keypair.pubkey();
		let to_pubkey = Pubkey::from_str("4MqL4qy2W1yXzuF3PiuSMehMbJzMuZEcBwVvrgtuhx7V").unwrap();
		let compute_unit_price = 100_0000;
		let compute_unit_limit = 300_000;
		let lamports = 1_000_000;
		let instructions = [
			SystemProgramInstruction::advance_nonce_account(
				&Pubkey::from_str(NONCE_ACCOUNTS[0]).unwrap(),
				&agg_key_pubkey,
			),
			ComputeBudgetInstruction::set_compute_unit_price(compute_unit_price),
			ComputeBudgetInstruction::set_compute_unit_limit(compute_unit_limit),
			SystemProgramInstruction::transfer(&agg_key_pubkey, &to_pubkey, lamports),
		];
		let message = Message::new(&instructions, Some(&agg_key_pubkey));
		let mut tx = Transaction::new_unsigned(message);
		tx.sign(&[&agg_key_keypair], durable_nonce);

		let serialized_tx = bincode::serde::encode_to_vec(tx, bincode::config::legacy()).unwrap();
		let expected_serialized_tx = hex_literal::hex!("017036ecc82313548a7f1ef280b9d7c53f9747e23abcb4e76d86c8df6aa87e82d460ad7cea2e8d972a833d3e1802341448a99be200ad4648c454b9d5a5e2d5020d01000306f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d19231e9528aae784fecbbd0bee129d9539c57be0e90061af6b6f4a5e274654e5bd400000000000000000000000000000000000000000000000000000000000000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000012c57218f6315b83818802f3522fe7e04c596ae4fe08841e7940bc2f958aaaea04030301050004040000000400090340420f000000000004000502e0930400030200020c0200000040420f0000000000").to_vec();

		assert_eq!(serialized_tx, expected_serialized_tx);
		assert!(serialized_tx.len() <= MAX_TRANSACTION_LENGTH)
	}

	#[test]
	fn create_fetch_native() {
		let durable_nonce = Hash::from_str("E6E2bNxGcgFyqeVRT3FSjw7YFbbMAZVQC21ZLVwrztRm").unwrap();
		let agg_key_keypair = Keypair::from_bytes(&RAW_KEYPAIR).unwrap();
		let agg_key_pubkey = agg_key_keypair.pubkey();
		let deposit_channel =
			Pubkey::from_str("DWHmaNGBzwMGjb6WP7G2Y6fbLunj6jjqHKjvxGSNo81G").unwrap();

		let instructions = [
			SystemProgramInstruction::advance_nonce_account(
				&Pubkey::from_str(NONCE_ACCOUNTS[0]).unwrap(),
				&agg_key_pubkey,
			),
			ProgramInstruction::get_instruction(
				&VaultProgram::FetchNative { seed: vec![11u8, 12u8, 13u8, 55u8], bump: 249 },
				vec![
					AccountMeta::new_readonly(
						Pubkey::from_str(VAULT_PROGRAM_DATA_ACCOUNT).unwrap(),
						false,
					),
					AccountMeta::new(agg_key_pubkey, true),
					AccountMeta::new(deposit_channel, false),
					AccountMeta::new_readonly(Pubkey::from_str(SYSTEM_PROGRAM_ID).unwrap(), false),
				],
			),
		];
		let message = Message::new(&instructions, Some(&agg_key_pubkey));
		let mut tx = Transaction::new_unsigned(message);
		tx.sign(&[&agg_key_keypair], durable_nonce);

		let serialized_tx = bincode::serde::encode_to_vec(tx, bincode::config::legacy()).unwrap();
		let expected_serialized_tx = hex_literal::hex!("018876d689319695f4e695d1bc6dc71b36cb7e81093d7122aa6153de24aeafe251d589f63321272cb2f195f81acf5617b16994bcdbdc070cfd459a2f76d0c4650701000407f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d192b9cd0bfce0d0c993da26980648022f34b2e9a33794312b94eb3f8cad440e3e6b000000000000000000000000000000000000000000000000000000000000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea94000004a8f28a600d49f666140b8b7456aedd064455f0aa5b8008894baf6ff84ed723b72b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293cc27e9074fac5e8d36cf04f94a0606fdd8ddbb420e99a489c7915ce5699e489000203030104000404000000060405000203118e24658f6c59298c040000000b0c0d37f9").to_vec();

		assert_eq!(serialized_tx, expected_serialized_tx);
		assert!(serialized_tx.len() <= MAX_TRANSACTION_LENGTH)
	}

	#[test]
	fn create_fetch_tokens() {
		let durable_nonce = Hash::from_str("DtoFEdczFkeephCjnSgh5ZqMpqdNeW5whSuhEKFjZEjK").unwrap();
		let agg_key_keypair = Keypair::from_bytes(&RAW_KEYPAIR).unwrap();
		let agg_key_pubkey = agg_key_keypair.pubkey();

		// Deposit channel derived from the Vault address from the seed and the bump
		let deposit_channel =
			Pubkey::from_str("EVW7c69WQENzFTc3QephEez8G8HKNE9FAWChvmv7CBbV").unwrap();
		// Derived from the deposit_channel
		let deposit_channel_ata =
			Pubkey::from_str("35zpEiFdRte7goYtbGYd8M1h5wjybGhPyGUHHU83CqBJ").unwrap();

		let instructions = [
			SystemProgramInstruction::advance_nonce_account(
				&Pubkey::from_str(NONCE_ACCOUNTS[0]).unwrap(),
				&agg_key_pubkey,
			),
			ProgramInstruction::get_instruction(
				&VaultProgram::FetchTokens {
					seed: vec![19u8, 2u8, 11u8],
					bump: 254,
					amount: 100000000,
					decimals: 6,
				},
				vec![
					AccountMeta::new_readonly(
						Pubkey::from_str(VAULT_PROGRAM_DATA_ACCOUNT).unwrap(),
						false,
					),
					AccountMeta::new_readonly(agg_key_pubkey, true),
					AccountMeta::new_readonly(deposit_channel, false),
					AccountMeta::new(deposit_channel_ata, false),
					AccountMeta::new(
						Pubkey::from_str(TOKEN_VAULT_ASSOCIATED_TOKEN_ACCOUNT).unwrap(),
						false,
					),
					AccountMeta::new_readonly(Pubkey::from_str(MINT_PUB_KEY).unwrap(), false),
					AccountMeta::new_readonly(Pubkey::from_str(TOKEN_PROGRAM_ID).unwrap(), false),
					AccountMeta::new_readonly(Pubkey::from_str(SYSTEM_PROGRAM_ID).unwrap(), false),
				],
			),
		];
		let message = Message::new(&instructions, Some(&agg_key_pubkey));
		let mut tx = Transaction::new_unsigned(message);
		tx.sign(&[&agg_key_keypair], durable_nonce);

		let serialized_tx = bincode::serde::encode_to_vec(tx, bincode::config::legacy()).unwrap();
		let expected_serialized_tx = hex_literal::hex!("01ae1c08f2bb80bd9eea640dea37d0bb5bbeb057715a49165542b8c8c6456225ea08266a69e3d2ca3bad21a3d573ba60990eb4a632bdcb31fdd06f494d1768370e0100070bf79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d1921eff116c91f924b871717959253d68219f0750a644b4fc95d9a3e5cda6cd250db966a2b36557938f49cc5d00f8f12d86f16f48e03b63c8422967dba621ab60bf000000000000000000000000000000000000000000000000000000000000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000006ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a90fb9ba52b1f09445f1e3a7508d59f0797923acf744fbe2da303fb06da859ee874a8f28a600d49f666140b8b7456aedd064455f0aa5b8008894baf6ff84ed723b72b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293cc8751ca3279285b0e42cb03e396d7a90fa147d917443e9c437fe48e4ccd954f0bf91277d430e44664449bc744abfff1c3da747f82bee786eb50e219be4c279ca0204030105000404000000090808000a020307060419494710642cb0c6460300000013020bfe00e1f5050000000006").to_vec();

		assert_eq!(serialized_tx, expected_serialized_tx);
		assert!(serialized_tx.len() <= MAX_TRANSACTION_LENGTH)
	}

	#[test]
	fn create_transfer_tokens() {
		let durable_nonce = Hash::from_str("aFgZn7H7bSMDWw3vDaUnkcboH7DCPAiu9bZP5p55mxq").unwrap();
		let agg_key_keypair = Keypair::from_bytes(&RAW_KEYPAIR).unwrap();
		let agg_key_pubkey = agg_key_keypair.pubkey();

		let to = Pubkey::from_str("pyq7ySiH5RvKteu2vdXKC7SNyNDp9vNDkGXdHxSpPtu").unwrap();
		let to_token_account =
			Pubkey::from_str("7iBzy7rBaX4tEtRwXaanXdMhAeDPf1S5fBcCnKeK39kK").unwrap();

		let instructions = [
			SystemProgramInstruction::advance_nonce_account(
				&Pubkey::from_str(NONCE_ACCOUNTS[0]).unwrap(),
				&agg_key_pubkey,
			),
			AssociatedTokenAccountInstruction::create_associated_token_account_idempotent_instruction(
				&agg_key_pubkey,
				&to,
				&Pubkey::from_str(MINT_PUB_KEY).unwrap(),
				&to_token_account
			),
			ProgramInstruction::get_instruction(
				&VaultProgram::TransferTokens { amount: 2, decimals: 6 },
				vec![
					AccountMeta::new_readonly(
						Pubkey::from_str(VAULT_PROGRAM_DATA_ACCOUNT).unwrap(),
						false,
					),
					AccountMeta::new_readonly(agg_key_pubkey, true),
					AccountMeta::new_readonly(Pubkey::from_str(TOKEN_VAULT_PDA_ACCOUNT).unwrap(), false),
					AccountMeta::new(Pubkey::from_str(TOKEN_VAULT_ASSOCIATED_TOKEN_ACCOUNT).unwrap(), false),
					AccountMeta::new(to_token_account, false),
					AccountMeta::new_readonly(Pubkey::from_str(MINT_PUB_KEY).unwrap(), false),
					AccountMeta::new_readonly(Pubkey::from_str(TOKEN_PROGRAM_ID).unwrap(), false),
					AccountMeta::new_readonly(Pubkey::from_str(SYSTEM_PROGRAM_ID).unwrap(), false),
				],
			),
		];
		let message = Message::new(&instructions, Some(&agg_key_pubkey));
		let mut tx = Transaction::new_unsigned(message);
		tx.sign(&[&agg_key_keypair], durable_nonce);

		let serialized_tx = bincode::serde::encode_to_vec(tx, bincode::config::legacy()).unwrap();
		let expected_serialized_tx = hex_literal::hex!("019cae816a9a1d6dcaf6a327e208999e8dd68ab2202d11199abd9ebe2f5e8c68ba1340169eb21bd47b2bb1ece8e7c617e347d223dad08deccdcfd17602d4b48c050100090df79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d19263b35f30ba8a5f9e80b8258b6e39ef4062e5f55c60a8217df3ec39457331cc80b966a2b36557938f49cc5d00f8f12d86f16f48e03b63c8422967dba621ab60bf000000000000000000000000000000000000000000000000000000000000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000006ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a90c4a8e3702f6e26d9d0c900c1461da4e3debef5743ce253bb9f0308a68c944220fb9ba52b1f09445f1e3a7508d59f0797923acf744fbe2da303fb06da859ee874a8f28a600d49f666140b8b7456aedd064455f0aa5b8008894baf6ff84ed723b72b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293c81a0052237ad76cb6e88fe505dc3d96bba6d8889f098b1eaa342ec84458805218c97258f4e2489f1bb3d1029148e0d830b5a1399daff1084048e7bd8dbe9f8590884c492fcd363de27fa07c2ae01f440765ce921938329887831c64d5f6dc81603040301050004040000000c0600020708040601010a0809000b03020806041136b4eeaf4a557ebc020000000000000006").to_vec();

		assert_eq!(serialized_tx, expected_serialized_tx);
		assert!(serialized_tx.len() <= MAX_TRANSACTION_LENGTH)
	}

	// Full rotation: Use nonce, rotate agg key, transfer nonce authority and transfer upgrade
	// manager's upgrade authority
	#[test]
	fn create_full_rotation() {
		let durable_nonce = Hash::from_str("CW1aUc4krwqNiMfZ9J4D7wWHd5GXCZFkBNJYJg3tRd1Y").unwrap();
		let agg_key_keypair = Keypair::from_bytes(&RAW_KEYPAIR).unwrap();
		let agg_key_pubkey = agg_key_keypair.pubkey();
		let new_agg_key_pubkey =
			Pubkey::from_str("7x7wY9yfXjRmusDEfPPCreU4bP49kmH4mqjYUXNAXJoM").unwrap();

		let mut instructions = vec![
			SystemProgramInstruction::advance_nonce_account(
				&Pubkey::from_str(NONCE_ACCOUNTS[0]).unwrap(),
				&agg_key_pubkey,
			),
			ComputeBudgetInstruction::set_compute_unit_price(100_0000),
			ComputeBudgetInstruction::set_compute_unit_limit(300_000),
			ProgramInstruction::get_instruction(
				&VaultProgram::RotateAggKey { skip_transfer_funds: false },
				vec![
					AccountMeta::new(Pubkey::from_str(VAULT_PROGRAM_DATA_ACCOUNT).unwrap(), false),
					AccountMeta::new(agg_key_pubkey, true),
					AccountMeta::new(new_agg_key_pubkey, false),
					AccountMeta::new_readonly(Pubkey::from_str(SYSTEM_PROGRAM_ID).unwrap(), false),
				],
			),
			set_upgrade_authority(
				Pubkey::from_str(UPGRADE_MANAGER_PROGRAM_DATA_ACCOUNT).unwrap(),
				&agg_key_pubkey,
				Some(&new_agg_key_pubkey),
			),
			SystemProgramInstruction::nonce_authorize(
				&Pubkey::from_str(NONCE_ACCOUNTS[0]).unwrap(),
				&agg_key_pubkey,
				&new_agg_key_pubkey,
			),
		];
		let message = Message::new(&instructions, Some(&agg_key_pubkey));
		let mut tx = Transaction::new_unsigned(message);
		tx.sign(&[&agg_key_keypair], durable_nonce);

		let serialized_tx = bincode::serde::encode_to_vec(tx, bincode::config::legacy()).unwrap();
		let expected_serialized_tx = hex_literal::hex!("01fed383fc6dce627eb80c846b416966ff97965a9803512978883e68c9fc46340e31bbaa57c280b231f7de1c0763fb254ebd6ae85ce927f06338d3edebaf4288070100050af79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d1924a8f28a600d49f666140b8b7456aedd064455f0aa5b8008894baf6ff84ed723b6744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be543990044a5cfec75730f8780ded36a7c8ae1dcc60d84e1a830765fc6108e7b40402e4951000000000000000000000000000000000000000000000000000000000000000002a8f6914e88a1b0e210153ef763ae2b00c2b93d16c124d2c0537a10048000000306466fe5211732ffecadba72c39be7bc8ce5bbc5f7126b2c439b3a4000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000072b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293caadf0d6118bacf2a2a5c3d141e72c8733db3de162cc364a7f779b7bb4670e59f06050301080004040000000700090340420f000000000007000502e0930400090402000305094e518fabdda5d68b00060304000304040000000502010024070000006744e9d9790761c45a800a074687b5ff47b449a90c722a3852543be543990044").to_vec();

		assert_eq!(serialized_tx, expected_serialized_tx);

		for (_, nonce_account) in NONCE_ACCOUNTS.iter().enumerate().skip(1) {
			instructions.push(SystemProgramInstruction::nonce_authorize(
				&Pubkey::from_str(nonce_account).unwrap(),
				&agg_key_pubkey,
				&new_agg_key_pubkey,
			));
			let message = Message::new(&instructions, Some(&agg_key_pubkey));
			let mut tx = Transaction::new_unsigned(message);
			tx.sign(&[&agg_key_keypair], durable_nonce);
			let serialized_tx =
				bincode::serde::encode_to_vec(tx, bincode::config::legacy()).unwrap();

			assert!(serialized_tx.len() <= MAX_TRANSACTION_LENGTH)
		}
	}

	#[test]
	fn create_ccm_native_transfer() {
		let durable_nonce = Hash::from_str("FJzAoeurcnAKG7eNFhzixySntXkDzoEh2bcRNfKm1gsy").unwrap();
		let agg_key_keypair = Keypair::from_bytes(&RAW_KEYPAIR).unwrap();
		let agg_key_pubkey = agg_key_keypair.pubkey();
		let to_pubkey = Pubkey::from_str("pyq7ySiH5RvKteu2vdXKC7SNyNDp9vNDkGXdHxSpPtu").unwrap();
		let cf_receiver = Pubkey::from_str("NJusJ7itnSsh4jSi43i9MMKB9sF4VbNvdSwUA45gPE6").unwrap();
		let amount: u64 = 1000000000;

		let instructions = [
			SystemProgramInstruction::advance_nonce_account(
				&Pubkey::from_str(NONCE_ACCOUNTS[0]).unwrap(),
				&agg_key_pubkey,
			),
			SystemProgramInstruction::transfer(&agg_key_pubkey, &to_pubkey, amount),
			ProgramInstruction::get_instruction(
				&VaultProgram::ExecuteCcmNativeCall {
					source_chain: 1,
					source_address: vec![11u8, 6u8, 152u8, 22u8, 3u8, 1u8],
					message: vec![124u8, 29u8, 15u8, 7u8],
					amount,
				},
				vec![
					AccountMeta::new_readonly(
						Pubkey::from_str(VAULT_PROGRAM_DATA_ACCOUNT).unwrap(),
						false,
					),
					AccountMeta::new_readonly(agg_key_pubkey, true),
					AccountMeta::new(to_pubkey, false),
					AccountMeta::new_readonly(cf_receiver, false),
					AccountMeta::new_readonly(Pubkey::from_str(SYSTEM_PROGRAM_ID).unwrap(), false),
					AccountMeta::new_readonly(
						Pubkey::from_str(SYS_VAR_INSTRUCTIONS).unwrap(),
						false,
					),
				],
			),
		];
		let message = Message::new(&instructions, Some(&agg_key_pubkey));
		let mut tx = Transaction::new_unsigned(message);
		tx.sign(&[&agg_key_keypair], durable_nonce);

		let serialized_tx = bincode::serde::encode_to_vec(tx, bincode::config::legacy()).unwrap();
		let expected_serialized_tx = hex_literal::hex!("01217162fc43cb4bb7aeaec8d386feda3de3eb82cf86373f9d06fc5eea953684b7fec12c88b916bc902db521942d170b5e190f5e1d8578e7eb27d5b8d2beb3a00301000609f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb0c4a8e3702f6e26d9d0c900c1461da4e3debef5743ce253bb9f0308a68c9442217eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d19200000000000000000000000000000000000000000000000000000000000000000575731869899efe0bd5d9161ad9f1db7c582c48c0b4ea7cff6a637c55c7310706a7d517187bd16635dad40455fdc2c0c124c68f215675a5dbbacb5f0800000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea94000004a8f28a600d49f666140b8b7456aedd064455f0aa5b8008894baf6ff84ed723b72b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293cd49f21c8621074e921e03bfa822094631b67b33facb1ad598841a5b91d2390080303030206000404000000030200010c0200000000ca9a3b000000000806070001040305267d050be38042e0b201000000060000000b0698160301040000007c1d0f0700ca9a3b00000000").to_vec();

		assert_eq!(serialized_tx, expected_serialized_tx);
		assert!(serialized_tx.len() <= MAX_TRANSACTION_LENGTH)
	}

	#[test]
	fn create_ccm_token_transfer() {
		let durable_nonce = Hash::from_str("B5SuVcUSTScrPyYsexYXECDTNysCSsy6tjJjGssDSVJg").unwrap();
		let agg_key_keypair = Keypair::from_bytes(&RAW_KEYPAIR).unwrap();
		let agg_key_pubkey = agg_key_keypair.pubkey();

		let cf_receiver = Pubkey::from_str("8pBPaVfTAcjLeNfC187Fkvi9b1XEFhRNJ95BQXXVksmH").unwrap();
		let amount: u64 = 10000000;

		// Associated token account from "pyq7ySiH5RvKteu2vdXKC7SNyNDp9vNDkGXdHxSpPtu"
		let to_token_account =
			Pubkey::from_str("7iBzy7rBaX4tEtRwXaanXdMhAeDPf1S5fBcCnKeK39kK").unwrap();

		let remaining_account =
			Pubkey::from_str("2npYpAQcNWcZo85eB43DnSMyeeVCiks7g65YaWVKp8TX").unwrap();

		// This would lack the idempotent account creating but that's fine for the test
		let instructions = [
			SystemProgramInstruction::advance_nonce_account(
				&Pubkey::from_str(NONCE_ACCOUNTS[0]).unwrap(),
				&agg_key_pubkey,
			),
			ProgramInstruction::get_instruction(
				&VaultProgram::TransferTokens { amount, decimals: 6 },
				vec![
					AccountMeta::new_readonly(
						Pubkey::from_str(VAULT_PROGRAM_DATA_ACCOUNT).unwrap(),
						false,
					),
					AccountMeta::new_readonly(agg_key_pubkey, true),
					AccountMeta::new_readonly(
						Pubkey::from_str(TOKEN_VAULT_PDA_ACCOUNT).unwrap(),
						false,
					),
					AccountMeta::new(
						Pubkey::from_str(TOKEN_VAULT_ASSOCIATED_TOKEN_ACCOUNT).unwrap(),
						false,
					),
					AccountMeta::new(to_token_account, false),
					AccountMeta::new_readonly(Pubkey::from_str(MINT_PUB_KEY).unwrap(), false),
					AccountMeta::new_readonly(Pubkey::from_str(TOKEN_PROGRAM_ID).unwrap(), false),
					AccountMeta::new_readonly(Pubkey::from_str(SYSTEM_PROGRAM_ID).unwrap(), false),
				],
			),
			ProgramInstruction::get_instruction(
				&VaultProgram::ExecuteCcmTokenCall {
					source_chain: 1,
					source_address: vec![11u8, 6u8, 152u8, 22u8, 3u8, 1u8],
					message: vec![124u8, 29u8, 15u8, 7u8],
					amount,
				},
				vec![
					AccountMeta::new_readonly(
						Pubkey::from_str(VAULT_PROGRAM_DATA_ACCOUNT).unwrap(),
						false,
					),
					AccountMeta::new_readonly(agg_key_pubkey, true),
					AccountMeta::new(to_token_account, false),
					AccountMeta::new_readonly(cf_receiver, false),
					AccountMeta::new_readonly(Pubkey::from_str(TOKEN_PROGRAM_ID).unwrap(), false),
					AccountMeta::new_readonly(Pubkey::from_str(MINT_PUB_KEY).unwrap(), false),
					AccountMeta::new_readonly(
						Pubkey::from_str(SYS_VAR_INSTRUCTIONS).unwrap(),
						false,
					),
					AccountMeta::new(remaining_account, false),
				],
			),
		];
		let message = Message::new(&instructions, Some(&agg_key_pubkey));
		let mut tx = Transaction::new_unsigned(message);
		tx.sign(&[&agg_key_keypair], durable_nonce);

		let serialized_tx = bincode::serde::encode_to_vec(tx, bincode::config::legacy()).unwrap();
		let expected_serialized_tx = hex_literal::hex!("01044b82cf77f58d01af97090e5b392317f2b3f3037b16f264a0114791d7cc6b4d234f1fc518ad5d28c1cd1cac0fc5b4969f7e77e07ba0577f9b44fade4f28ec090100090ef79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d1921a98962b4689f93f09c8bbcf32aec09ab57e8ea65a02cc814e335cbf3e853ab663b35f30ba8a5f9e80b8258b6e39ef4062e5f55c60a8217df3ec39457331cc80b966a2b36557938f49cc5d00f8f12d86f16f48e03b63c8422967dba621ab60bf000000000000000000000000000000000000000000000000000000000000000006a7d517187bd16635dad40455fdc2c0c124c68f215675a5dbbacb5f0800000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000006ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a90fb9ba52b1f09445f1e3a7508d59f0797923acf744fbe2da303fb06da859ee874a8f28a600d49f666140b8b7456aedd064455f0aa5b8008894baf6ff84ed723b72b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293c7417da8b99d7748127a76b03d61fee69c80dfef73ad2d5503737beedc5a9ed4881a0052237ad76cb6e88fe505dc3d96bba6d8889f098b1eaa342ec844588052195b87bc2cbdf23ee31be8254d8d7fcd7c31aaa6930621a20d5841df15672a1d903050301070004040000000b080a000d04030908051136b4eeaf4a557ebc8096980000000000060b080a00030c08090602266cb8a27b9fdeaa2301000000060000000b0698160301040000007c1d0f078096980000000000").to_vec();

		assert_eq!(serialized_tx, expected_serialized_tx);
		assert!(serialized_tx.len() <= MAX_TRANSACTION_LENGTH)
	}

	#[test]
	fn create_upgrade_vault_program() {
		let durable_nonce = Hash::from_str("6Wj7BNUVAhoMpqeHATYqRs7waV5s8JKNosmMNkzfCJxy").unwrap();

		let agg_key_keypair = Keypair::from_bytes(&RAW_KEYPAIR).unwrap();
		let agg_key_pubkey = agg_key_keypair.pubkey();
		let govkey_pubkey = agg_key_keypair.pubkey();
		let buffer_address =
			Pubkey::from_str("3Cj5FNvkm8eGz64t8TFPw2PTuFHUheqtmHhhozm8RsuT").unwrap();
		let spill_address =
			Pubkey::from_str("HfasueN6RNPjSM6rKGH5dga6kS2oUF8siGH3m4MXPURp").unwrap();

		let instructions = [
			SystemProgramInstruction::advance_nonce_account(
				&Pubkey::from_str(NONCE_ACCOUNTS[0]).unwrap(),
				&agg_key_pubkey,
			),
			ProgramInstruction::get_instruction(
				&UpgradeManagerProgram::UpgradeVaultProgram {
					seed: UPGRADE_MANAGER_PDA_SIGNER_SEED.to_vec(),
					bump: UPGRADE_MANAGER_PDA_SIGNER_BUMP,
				},
				vec![
					AccountMeta::new_readonly(
						Pubkey::from_str(VAULT_PROGRAM_DATA_ACCOUNT).unwrap(),
						false,
					),
					AccountMeta::new_readonly(govkey_pubkey, true),
					AccountMeta::new(Pubkey::from_str(VAULT_PROGRAM_DATA_ADDRESS).unwrap(), false),
					AccountMeta::new(Pubkey::from_str(VAULT_PROGRAM).unwrap(), false),
					AccountMeta::new(buffer_address, false),
					AccountMeta::new(spill_address, false),
					AccountMeta::new_readonly(Pubkey::from_str(SYS_VAR_RENT).unwrap(), false),
					AccountMeta::new_readonly(Pubkey::from_str(SYS_VAR_CLOCK).unwrap(), false),
					AccountMeta::new_readonly(
						Pubkey::from_str(UPGRADE_MANAGER_PDA_SIGNER).unwrap(),
						false,
					),
					AccountMeta::new_readonly(
						Pubkey::from_str(BPF_LOADER_UPGRADEABLE_ID).unwrap(),
						false,
					),
				],
			),
		];
		let message = Message::new(&instructions, Some(&agg_key_pubkey));
		let mut tx = Transaction::new_unsigned(message);
		tx.sign(&[&agg_key_keypair], durable_nonce);

		let serialized_tx = bincode::serde::encode_to_vec(tx, bincode::config::legacy()).unwrap();
		let expected_serialized_tx = hex_literal::hex!("01ab10abaa3ebb93f14f783053d9b9e23c95fc1c76b6b72ebb285c8f1d61acebc0b7ad323c9d370c00b8867f6b1f4aa92a974a879ecc9f3e63ae63fb9062bf0d0f0100080df79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d19220b855ca2d3763c4f0f055c8ec3523199c684abe93655a9cde1bc843da5ef2da298f27f13ce155954657f0238e63932beb510964abd44e20e9603e6b6f2b424a72b5d2051d300b10b74314b7e25ace9998ca66eb2c7fbc10ef130dd67028293c000000000000000000000000000000000000000000000000000000000000000002a8f6914e88a1b0e210153ef763ae2b00c2b93d16c124d2c0537a100480000006a7d51718c774c928566398691d5eb68b5eb8a39b4b6d5c73555b210000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000006a7d517192c5c51218cc94c3d4af17f58daee089ba1fd44e3dbd98a000000001068c72f83398081684c491910b66f8d8cca0edc00cbcf11c89f86c5c39d80f7154e2cfe8c12c99db54b553ffeae05145bcafaab8b11ea3d1a8c45f98764bfd44a8f28a600d49f666140b8b7456aedd064455f0aa5b8008894baf6ff84ed723b51e7e35a9b795b74ca6d292d9949701179cf2a5a20621893d50b6e4dadfcca0a02050301080004040000000a0a0c000304020009070b061348d34cbd54b03e65060000007369676e6572ff").to_vec();

		assert_eq!(serialized_tx, expected_serialized_tx);
		assert!(serialized_tx.len() <= MAX_TRANSACTION_LENGTH)
	}

	#[test]
	fn create_idempotent_associated_token_account() {
		let durable_nonce = Hash::from_str("3GY33ibbFkTSdXeXuPAh2NxGTwm1TfEFNKKG9XjxFa67").unwrap();
		let agg_key_keypair = Keypair::from_bytes(&RAW_KEYPAIR).unwrap();
		let agg_key_pubkey = agg_key_keypair.pubkey();

		// This is needed to derive the pda_ata to create the
		// createAssociatedTokenAccountIdempotentInstruction but for now we just derive it manually
		let to = Pubkey::from_str("pyq7ySiH5RvKteu2vdXKC7SNyNDp9vNDkGXdHxSpPtu").unwrap();
		let to_ata = Pubkey::from_str("EbarLzqEb9jf2ZHUdDf5nuBP52Ut3ddLZtYrGwKh3Bbd").unwrap();
		let mint_pubkey = Pubkey::from_str("21ySx9qZoscVT8ViTZjcudCCJeThnXfLPe1sLvezqRCv").unwrap();

		// This would lack the idempotent account creating but that's fine for the test
		let instructions = [
			SystemProgramInstruction::advance_nonce_account(
				&Pubkey::from_str(NONCE_ACCOUNTS[0]).unwrap(),
				&agg_key_pubkey,
			),
			AssociatedTokenAccountInstruction::create_associated_token_account_idempotent_instruction(
				&agg_key_pubkey,
				&to,
				&mint_pubkey,
				&to_ata
			),
		];
		let message = Message::new(&instructions, Some(&agg_key_pubkey));
		let mut tx = Transaction::new_unsigned(message);
		tx.sign(&[&agg_key_keypair], durable_nonce);

		let serialized_tx = bincode::serde::encode_to_vec(tx, bincode::config::legacy()).unwrap();
		let expected_serialized_tx = hex_literal::hex!("01eb287ff9329fbaf83592ec56709d52d3d7f7edcab7ab53fc8371acff871016c51dfadde692630545a91d6534095bb5697b5fb9ee17dc292552eabf9ab6e3390601000609f79d5e026f12edc6443a534b2cdd5072233989b415d7596573e743f3e5b386fb17eb2b10d3377bda2bc7bea65bec6b8372f4fc3463ec2cd6f9fde4b2c633d192ca03f3e6d6fd79aaf8ebd4ce053492a34f22d0edafbfa88a380848d9a4735150000000000000000000000000000000000000000000000000000000000000000006a7d517192c568ee08a845f73d29788cf035c3145b21ab344d8062ea940000006ddf6e1d765a193d9cbe146ceeb79ac1cb485ed5f5b37913a8cf5857eff00a90c4a8e3702f6e26d9d0c900c1461da4e3debef5743ce253bb9f0308a68c944220f1b83220b1108ea0e171b5391e6c0157370c8353516b74e962f855be6d787038c97258f4e2489f1bb3d1029148e0d830b5a1399daff1084048e7bd8dbe9f85921b22d7dfc8cdeba6027384563948d038a11eba06289de51a15c3d649d1f7e2c020303010400040400000008060002060703050101").to_vec();

		assert_eq!(serialized_tx, expected_serialized_tx);
		assert!(serialized_tx.len() <= MAX_TRANSACTION_LENGTH)
	}

	// TODO: Pull and compare discriminators and function from the contracts-interfaces
	#[test]
	fn test_function_discriminators() {
		assert_eq!(
			VaultProgram::function_discriminator(&VaultProgram::RotateAggKey {
				skip_transfer_funds: false
			}),
			vec![78u8, 81u8, 143u8, 171u8, 221u8, 165u8, 214u8, 139u8]
		);
		assert_eq!(
			VaultProgram::function_discriminator(&VaultProgram::FetchTokens {
				seed: vec![34u8, 27u8, 77u8],
				bump: 2,
				amount: 6,
				decimals: 6
			}),
			vec![73u8, 71u8, 16u8, 100u8, 44u8, 176u8, 198u8, 70u8]
		);
		assert_eq!(
			VaultProgram::function_discriminator(&VaultProgram::TransferTokens {
				amount: 6,
				decimals: 6
			}),
			vec![54u8, 180u8, 238u8, 175u8, 74u8, 85u8, 126u8, 188u8]
		);
		assert_eq!(
			VaultProgram::function_discriminator(&VaultProgram::FetchNative {
				seed: vec![1u8, 2u8, 3u8],
				bump: 13
			}),
			vec![142u8, 36u8, 101u8, 143u8, 108u8, 89u8, 41u8, 140u8]
		);
		assert_eq!(
			VaultProgram::function_discriminator(&VaultProgram::ExecuteCcmNativeCall {
				source_chain: 1,
				source_address: vec![2u8, 2u8, 67u8],
				message: vec![2u8],
				amount: 4
			}),
			vec![125u8, 5u8, 11u8, 227u8, 128u8, 66u8, 224u8, 178u8]
		);
		assert_eq!(
			VaultProgram::function_discriminator(&VaultProgram::ExecuteCcmTokenCall {
				source_chain: 1,
				source_address: vec![2u8, 2u8, 67u8],
				message: vec![3u8],
				amount: 1
			}),
			vec![108u8, 184u8, 162u8, 123u8, 159u8, 222u8, 170u8, 35u8]
		);
		assert_eq!(
			UpgradeManagerProgram::function_discriminator(
				&UpgradeManagerProgram::UpgradeVaultProgram { seed: vec![31u8, 1u8, 5u8], bump: 3 }
			),
			vec![72u8, 211u8, 76u8, 189u8, 84u8, 176u8, 62u8, 101u8]
		);
		assert_eq!(
			UpgradeManagerProgram::function_discriminator(
				&UpgradeManagerProgram::TransferVaultUpgradeAuthority {
					seed: vec![1u8, 5u8, 7u8],
					bump: 3,
				}
			),
			vec![114u8, 247u8, 72u8, 110u8, 145u8, 65u8, 236u8, 153u8]
		);
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
