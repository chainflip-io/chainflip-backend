use crate::{
	compile_instructions, consts::MESSAGE_VERSION_PREFIX, short_vec, AddressLookupTableAccount,
	CompiledInstruction, CompiledKeys, Hash, Instruction, MessageAddressTableLookup, MessageHeader,
	Pubkey, RawSignature, Signature,
};
use codec::{Decode, Encode};
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_std::{vec, vec::Vec};

use serde::{
	de::{self, SeqAccess, Unexpected, Visitor},
	ser::SerializeTuple,
	Deserializer, Serializer,
};
use sp_std::fmt;

#[cfg(feature = "std")]
use crate::{
	errors::TransactionError,
	signer::{Signer, SignerError, TestSigners},
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", untagged)]
pub enum TransactionVersion {
	Legacy(Legacy),
	Number(u8),
}
impl TransactionVersion {
	pub const LEGACY: Self = Self::Legacy(Legacy::Legacy);
}

/// Type that serializes to the string "legacy"
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Legacy {
	Legacy,
}

/// Either a legacy message or a v0 message.
///
/// # Serialization
///
/// If the first bit is set, the remaining 7 bits will be used to determine
/// which message version is serialized starting from version `0`. If the first
/// is bit is not set, all bytes are used to encode the legacy `Message`
/// format.
#[derive(Debug, PartialEq, Eq, Clone, Encode, Decode, TypeInfo)]
pub enum VersionedMessage {
	Legacy(legacy::LegacyMessage),
	V0(v0::VersionedMessageV0),
}

impl VersionedMessage {
	pub fn new(
		instructions: &[Instruction],
		payer: Option<Pubkey>,
		blockhash: Option<Hash>,
		lookup_tables: &[AddressLookupTableAccount],
	) -> VersionedMessage {
		VersionedMessage::V0(v0::VersionedMessageV0::new_with_blockhash(
			instructions,
			payer,
			blockhash.unwrap_or_default(),
			lookup_tables,
		))
	}

	pub fn header(&self) -> &MessageHeader {
		match self {
			Self::Legacy(message) => &message.header,
			Self::V0(message) => &message.header,
		}
	}

	pub fn static_account_keys(&self) -> &[Pubkey] {
		match self {
			Self::Legacy(message) => &message.account_keys,
			Self::V0(message) => &message.account_keys,
		}
	}

	pub fn map_static_account_keys(&mut self, f: impl Fn(Pubkey) -> Pubkey) {
		match self {
			Self::Legacy(message) =>
				for k in message.account_keys.iter_mut() {
					*k = f(*k);
				},
			Self::V0(message) =>
				for k in message.account_keys.iter_mut() {
					*k = f(*k);
				},
		}
	}

	pub fn set_static_account_keys(&mut self, new_keys: Vec<Pubkey>) {
		match self {
			Self::Legacy(message) => message.account_keys = new_keys,
			Self::V0(message) => message.account_keys = new_keys,
		}
	}

	pub fn address_table_lookups(&self) -> Option<&[MessageAddressTableLookup]> {
		match self {
			Self::Legacy(_) => None,
			Self::V0(message) => Some(&message.address_table_lookups),
		}
	}

	/// Returns true if the account at the specified index signed this
	/// message.
	pub fn is_signer(&self, index: usize) -> bool {
		index < usize::from(self.header().num_required_signatures)
	}

	pub fn recent_blockhash(&self) -> &Hash {
		match self {
			Self::Legacy(message) => &message.recent_blockhash,
			Self::V0(message) => &message.recent_blockhash,
		}
	}

	pub fn set_recent_blockhash(&mut self, recent_blockhash: Hash) {
		match self {
			Self::Legacy(message) => message.recent_blockhash = recent_blockhash,
			Self::V0(message) => message.recent_blockhash = recent_blockhash,
		}
	}

	/// Program instructions that will be executed in sequence and committed in
	/// one atomic transaction if all succeed.
	pub fn instructions(&self) -> &[CompiledInstruction] {
		match self {
			Self::Legacy(message) => &message.instructions,
			Self::V0(message) => &message.instructions,
		}
	}

	pub fn serialize(&self) -> Vec<u8> {
		bincode::serde::encode_to_vec(self, bincode::config::legacy()).unwrap()
	}
}

impl Default for VersionedMessage {
	fn default() -> Self {
		Self::Legacy(legacy::LegacyMessage::default())
	}
}

impl serde::Serialize for VersionedMessage {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: Serializer,
	{
		match self {
			Self::Legacy(message) => {
				let mut seq = serializer.serialize_tuple(1)?;
				seq.serialize_element(message)?;
				seq.end()
			},
			Self::V0(message) => {
				let mut seq = serializer.serialize_tuple(2)?;
				seq.serialize_element(&MESSAGE_VERSION_PREFIX)?;
				seq.serialize_element(message)?;
				seq.end()
			},
		}
	}
}
enum MessagePrefix {
	Legacy(u8),
	Versioned(u8),
}

#[allow(clippy::needless_lifetimes)]
impl<'de> serde::Deserialize<'de> for MessagePrefix {
	fn deserialize<D>(deserializer: D) -> Result<MessagePrefix, D::Error>
	where
		D: Deserializer<'de>,
	{
		struct PrefixVisitor;

		impl<'de> Visitor<'de> for PrefixVisitor {
			type Value = MessagePrefix;

			fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
				formatter.write_str("message prefix byte")
			}

			// Serde's integer visitors bubble up to u64 so check the prefix
			// with this function instead of visit_u8. This approach is
			// necessary because serde_json directly calls visit_u64 for
			// unsigned integers.
			fn visit_u64<E: de::Error>(self, value: u64) -> Result<MessagePrefix, E> {
				if value > u8::MAX as u64 {
					Err(de::Error::invalid_type(Unexpected::Unsigned(value), &self))?;
				}

				let byte = value as u8;
				if byte & MESSAGE_VERSION_PREFIX != 0 {
					Ok(MessagePrefix::Versioned(byte & !MESSAGE_VERSION_PREFIX))
				} else {
					Ok(MessagePrefix::Legacy(byte))
				}
			}
		}

		deserializer.deserialize_u8(PrefixVisitor)
	}
}

impl<'de> serde::Deserialize<'de> for VersionedMessage {
	fn deserialize<D>(deserializer: D) -> Result<VersionedMessage, D::Error>
	where
		D: Deserializer<'de>,
	{
		struct MessageVisitor;

		impl<'de> Visitor<'de> for MessageVisitor {
			type Value = VersionedMessage;

			fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
				formatter.write_str("message bytes")
			}

			fn visit_seq<A>(self, mut seq: A) -> Result<VersionedMessage, A::Error>
			where
				A: SeqAccess<'de>,
			{
				let prefix: MessagePrefix =
					seq.next_element()?.ok_or_else(|| de::Error::invalid_length(0, &self))?;

				match prefix {
					MessagePrefix::Legacy(num_required_signatures) => {
						// The remaining fields of the legacy Message struct after the first
						// byte.
						#[derive(Serialize, Deserialize)]
						struct RemainingLegacyMessage {
							pub num_readonly_signed_accounts: u8,
							pub num_readonly_unsigned_accounts: u8,
							#[serde(with = "short_vec")]
							pub account_keys: Vec<Pubkey>,
							pub recent_blockhash: Hash,
							#[serde(with = "short_vec")]
							pub instructions: Vec<CompiledInstruction>,
						}

						let message: RemainingLegacyMessage =
							seq.next_element()?.ok_or_else(|| {
								// will never happen since tuple length is always 2
								de::Error::invalid_length(1, &self)
							})?;

						Ok(VersionedMessage::Legacy(legacy::LegacyMessage {
							header: MessageHeader {
								num_required_signatures,
								num_readonly_signed_accounts: message.num_readonly_signed_accounts,
								num_readonly_unsigned_accounts: message
									.num_readonly_unsigned_accounts,
							},
							account_keys: message.account_keys,
							recent_blockhash: message.recent_blockhash,
							instructions: message.instructions,
						}))
					},
					MessagePrefix::Versioned(version) => {
						match version {
							0 => {
								Ok(VersionedMessage::V0(seq.next_element()?.ok_or_else(|| {
									// will never happen since tuple length is always 2
									de::Error::invalid_length(1, &self)
								})?))
							},
							127 => {
								// 0xff is used as the first byte of the off-chain messages
								// which corresponds to version 127 of the versioned messages.
								// This explicit check is added to prevent the usage of version
								// 127 in the runtime as a valid transaction.
								Err(de::Error::custom("off-chain messages are not accepted"))
							},
							_ => Err(de::Error::invalid_value(
								de::Unexpected::Unsigned(version as u64),
								&"a valid transaction message version",
							)),
						}
					},
				}
			}
		}

		deserializer.deserialize_tuple(2, MessageVisitor)
	}
}

// NOTE: Serialization-related changes must be paired with the direct read at sigverify.
/// An atomic transaction
#[derive(
	Debug, PartialEq, Default, Eq, Clone, Serialize, Deserialize, Encode, Decode, TypeInfo,
)]
pub struct VersionedTransaction {
	/// List of signatures
	#[serde(with = "short_vec")]
	pub signatures: Vec<Signature>,
	/// Message to sign.
	pub message: VersionedMessage,
}

impl From<legacy::LegacyTransaction> for VersionedTransaction {
	fn from(transaction: legacy::LegacyTransaction) -> Self {
		Self {
			signatures: transaction.signatures,
			message: VersionedMessage::Legacy(transaction.message),
		}
	}
}

impl VersionedTransaction {
	pub fn new_unsigned(message: VersionedMessage) -> Self {
		Self {
			signatures: vec![
				Signature::default();
				message.header().num_required_signatures as usize
			],
			message,
		}
	}

	#[cfg(feature = "std")]
	pub fn test_only_sign<S: Signer>(&mut self, signers: TestSigners<S>, recent_blockhash: Hash) {
		let positions = self.get_signing_keypair_positions(signers.pubkeys());

		// if you change the blockhash, you're re-signing...
		if recent_blockhash != *self.message.recent_blockhash() {
			self.message.set_recent_blockhash(recent_blockhash);
			self.signatures
				.iter_mut()
				.for_each(|signature| *signature = Signature::default());
		}

		let signatures = signers
			.try_sign_message(&self.message_data())
			.expect("Transaction signing should never fail.");
		for i in 0..positions.len() {
			self.signatures[positions[i]] = signatures[i];
		}
	}

	#[cfg(feature = "std")]
	fn get_signing_keypair_positions(&self, pubkeys: Vec<Pubkey>) -> Vec<usize> {
		let account_keys = self.message.static_account_keys();
		let required_sigs = self.message.header().num_required_signatures as usize;
		if account_keys.len() < required_sigs {
			panic!("Too many signing keys provided.");
		}

		let signed_keys = account_keys[0..required_sigs].to_vec();

		pubkeys
			.iter()
			.map(|pubkey| signed_keys.iter().position(|x| x == pubkey))
			.map(|index| index.expect("Signing key must be part of account_keys."))
			.collect()
	}

	pub fn is_signed(&self) -> bool {
		self.signatures.iter().all(|signature| *signature != Signature::default())
	}

	/// Return the message containing all data that should be signed.
	pub fn message(&self) -> &VersionedMessage {
		&self.message
	}

	/// Return the serialized message data to sign.
	pub fn message_data(&self) -> Vec<u8> {
		self.message().serialize()
	}

	/// Due to different Serialization between Signature and Solana native Signature type,
	/// the Signatures needs to be converted into the RawSignature type before the
	/// transaction is serialized as whole.
	pub fn finalize_and_serialize(self) -> Result<Vec<u8>, bincode::error::EncodeError> {
		bincode::serde::encode_to_vec(RawTransaction::from(self), bincode::config::legacy())
	}

	/// Returns the version of the transaction
	pub fn version(&self) -> TransactionVersion {
		match self.message {
			VersionedMessage::Legacy(_) => TransactionVersion::LEGACY,
			VersionedMessage::V0(_) => TransactionVersion::Number(0),
		}
	}

	/// Returns a legacy transaction if the transaction message is legacy.
	pub fn into_legacy_transaction(self) -> Option<legacy::LegacyTransaction> {
		match self.message {
			VersionedMessage::Legacy(message) =>
				Some(legacy::LegacyTransaction { signatures: self.signatures, message }),
			_ => None,
		}
	}
}

pub mod v0 {
	use super::*;
	use crate::{AccountKeys, CompileError, MessageAddressTableLookup};

	#[cfg(test)]
	use crate::instructions::program_instructions;

	/// A Solana transaction message (v0).
	///
	/// This message format supports succinct account loading with
	/// on-chain address lookup tables.
	///
	/// See the [`message`] module documentation for further description.
	///
	/// [`message`]: crate::message
	#[derive(
		Serialize, Deserialize, Default, Debug, PartialEq, Eq, Clone, Encode, Decode, TypeInfo,
	)]
	#[serde(rename_all = "camelCase")]
	pub struct VersionedMessageV0 {
		/// The message header, identifying signed and read-only `account_keys`.
		/// Header values only describe static `account_keys`, they do not describe
		/// any additional account keys loaded via address table lookups.
		pub header: MessageHeader,

		/// List of accounts loaded by this transaction.
		#[serde(with = "short_vec")]
		pub account_keys: Vec<Pubkey>,

		/// The blockhash of a recent block.
		pub recent_blockhash: Hash,

		/// Instructions that invoke a designated program, are executed in sequence,
		/// and committed in one atomic transaction if all succeed.
		///
		/// # Notes
		///
		/// Program indexes must index into the list of message `account_keys` because
		/// program id's cannot be dynamically loaded from a lookup table.
		///
		/// Account indexes must index into the list of addresses
		/// constructed from the concatenation of three key lists:
		///   1) message `account_keys`
		///   2) ordered list of keys loaded from `writable` lookup table indexes
		///   3) ordered list of keys loaded from `readable` lookup table indexes
		#[serde(with = "short_vec")]
		pub instructions: Vec<CompiledInstruction>,

		/// List of address table lookups used to load additional accounts
		/// for this transaction.
		#[serde(with = "short_vec")]
		pub address_table_lookups: Vec<MessageAddressTableLookup>,
	}

	impl VersionedMessageV0 {
		pub fn new_with_blockhash(
			instructions: &[Instruction],
			payer: Option<Pubkey>,
			blockhash: Hash,
			lookup_tables: &[AddressLookupTableAccount],
		) -> Self {
			Self::try_compile(payer, instructions, lookup_tables, blockhash)
				.expect("Message construction should never fail.")
		}

		pub fn new(
			instructions: &[Instruction],
			payer: Option<Pubkey>,
			lookup_tables: &[AddressLookupTableAccount],
		) -> Self {
			Self::new_with_blockhash(instructions, payer, Hash::default(), lookup_tables)
		}

		#[cfg(test)]
		pub fn new_with_nonce(
			mut instructions: Vec<Instruction>,
			payer: Option<Pubkey>,
			nonce_account_pubkey: &Pubkey,
			nonce_authority_pubkey: &Pubkey,
			lookup_tables: &[AddressLookupTableAccount],
		) -> Self {
			let nonce_ix = program_instructions::SystemProgramInstruction::advance_nonce_account(
				nonce_account_pubkey,
				nonce_authority_pubkey,
			);
			instructions.insert(0, nonce_ix);
			Self::new(&instructions, payer, lookup_tables)
		}

		/// Create a signable transaction message from a `payer` public key,
		/// `recent_blockhash`, list of `instructions`, and a list of
		/// `address_lookup_table_accounts`.
		///
		/// # Examples
		///
		/// This example uses the [`solana_rpc_client`], [`solana_sdk`], and [`anyhow`] crates.
		///
		/// [`solana_rpc_client`]: https://docs.rs/solana-rpc-client
		/// [`solana_sdk`]: https://docs.rs/solana-sdk
		/// [`anyhow`]: https://docs.rs/anyhow
		///
		/// ```ignore
		/// # use solana_program::example_mocks::{
		/// #     solana_rpc_client,
		/// #     solana_sdk,
		/// # };
		/// # use std::borrow::Cow;
		/// # use solana_sdk::account::Account;
		/// use anyhow::Result;
		/// use solana_rpc_client::rpc_client::RpcClient;
		/// use solana_program::address_lookup_table::{self, state::{AddressLookupTable, LookupTableMeta}};
		/// use solana_sdk::{
		///      address_lookup_table::AddressLookupTableAccount,
		///      instruction::{AccountMeta, Instruction},
		///      message::{VersionedMessage, v0},
		///      pubkey::Pubkey,
		///      signature::{Keypair, Signer},
		///      transaction::VersionedTransaction,
		/// };
		///
		/// fn create_tx_with_address_table_lookup(
		///     client: &RpcClient,
		///     instruction: Instruction,
		///     address_lookup_table_key: Pubkey,
		///     payer: &Keypair,
		/// ) -> Result<VersionedTransaction> {
		///     # client.set_get_account_response(address_lookup_table_key, Account {
		///     #   lamports: 1,
		///     #   data: AddressLookupTable {
		///     #     meta: LookupTableMeta::default(),
		///     #     addresses: Cow::Owned(instruction.accounts.iter().map(|meta| meta.pubkey).collect()),
		///     #   }.serialize_for_tests().unwrap(),
		///     #   owner: address_lookup_table::program::id(),
		///     #   executable: false,
		///     #   rent_epoch: 1,
		///     # });
		///     let raw_account = client.get_account(&address_lookup_table_key)?;
		///     let address_lookup_table = AddressLookupTable::deserialize(&raw_account.data)?;
		///     let address_lookup_table_account = AddressLookupTableAccount {
		///         key: address_lookup_table_key,
		///         addresses: address_lookup_table.addresses.to_vec(),
		///     };
		///
		///     let blockhash = client.get_latest_blockhash()?;
		///     let tx = VersionedTransaction::try_new(
		///         VersionedMessage::V0(v0::Message::try_compile(
		///             &payer.pubkey(),
		///             &[instruction],
		///             &[address_lookup_table_account],
		///             blockhash,
		///         )?),
		///         &[payer],
		///     )?;
		///
		///     # assert!(tx.message.address_table_lookups().unwrap().len() > 0);
		///     Ok(tx)
		/// }
		/// #
		/// # let client = RpcClient::new(String::new());
		/// # let payer = Keypair::new();
		/// # let address_lookup_table_key = Pubkey::new_unique();
		/// # let instruction = Instruction::new_with_bincode(Pubkey::new_unique(), &(), vec![
		/// #   AccountMeta::new(Pubkey::new_unique(), false),
		/// # ]);
		/// # create_tx_with_address_table_lookup(&client, instruction, address_lookup_table_key, &payer)?;
		/// # Ok::<(), anyhow::Error>(())
		/// ```
		pub fn try_compile(
			payer: Option<Pubkey>,
			instructions: &[Instruction],
			address_lookup_table_accounts: &[AddressLookupTableAccount],
			recent_blockhash: Hash,
		) -> Result<Self, CompileError> {
			let mut compiled_keys = CompiledKeys::compile(instructions, payer);

			let mut address_table_lookups = Vec::with_capacity(address_lookup_table_accounts.len());
			let mut loaded_addresses_list = Vec::with_capacity(address_lookup_table_accounts.len());
			for lookup_table_account in address_lookup_table_accounts {
				if let Some((lookup, loaded_addresses)) =
					compiled_keys.try_extract_table_lookup(lookup_table_account)?
				{
					address_table_lookups.push(lookup);
					loaded_addresses_list.push(loaded_addresses);
				}
			}

			let (header, static_keys) = compiled_keys.try_into_message_components()?;
			let dynamic_keys = loaded_addresses_list.into_iter().collect();
			let account_keys = AccountKeys::new(&static_keys, Some(&dynamic_keys));
			let instructions = account_keys.try_compile_instructions(instructions)?;

			Ok(Self {
				header,
				account_keys: static_keys,
				recent_blockhash,
				instructions,
				address_table_lookups,
			})
		}

		/// Serialize this message with a version #0 prefix using bincode encoding.
		/// MODIFIED: use `encode_to_vec` instead - since special version don't have `serialize()`.
		pub fn serialize(&self) -> Vec<u8> {
			bincode::serde::encode_to_vec(self, bincode::config::legacy()).unwrap()
		}
	}
}

pub mod legacy {
	use super::*;

	#[cfg(test)]
	use crate::instructions::program_instructions;

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
	// for versioned messages in the `RemainingMessage` struct.
	#[derive(
		Encode, Decode, TypeInfo, Serialize, Deserialize, Default, Debug, PartialEq, Eq, Clone,
	)]
	#[serde(rename_all = "camelCase")]
	pub struct LegacyMessage {
		/// The message header, identifying signed and read-only `account_keys`.
		// NOTE: Serialization-related changes must be paired with the direct read at sigverify.
		pub header: MessageHeader,

		/// All the account keys used by this transaction.
		#[serde(with = "short_vec")]
		pub account_keys: Vec<Pubkey>,

		/// The id of a recent ledger entry.
		pub recent_blockhash: Hash,

		/// Programs that will be executed in sequence and committed in one atomic transaction if
		/// all succeed.
		#[serde(with = "short_vec")]
		pub instructions: Vec<CompiledInstruction>,
	}

	impl LegacyMessage {
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
	pub struct LegacyTransaction {
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
		pub message: LegacyMessage,
	}

	impl LegacyTransaction {
		pub fn new_unsigned(message: LegacyMessage) -> Self {
			Self {
				signatures: vec![
					Signature::default();
					message.header.num_required_signatures as usize
				],
				message,
			}
		}

		#[cfg(feature = "std")]
		pub fn test_only_sign<S: Signer>(
			&mut self,
			signers: TestSigners<S>,
			recent_blockhash: Hash,
		) {
			if let Err(e) = self.try_sign(signers, recent_blockhash) {
				panic!("Transaction::sign failed with error {e:?}");
			}
		}

		#[cfg(feature = "std")]
		fn try_sign<S: Signer>(
			&mut self,
			signers: TestSigners<S>,
			recent_blockhash: Hash,
		) -> Result<(), SignerError> {
			self.try_partial_sign(signers, recent_blockhash)?;

			if !self.is_signed() {
				Err(SignerError::NotEnoughSigners)
			} else {
				Ok(())
			}
		}

		#[cfg(feature = "std")]
		fn try_partial_sign<S: Signer>(
			&mut self,
			signers: TestSigners<S>,
			recent_blockhash: Hash,
		) -> Result<(), SignerError> {
			let positions = self.get_signing_keypair_positions(signers.pubkeys())?;
			if positions.iter().any(|pos| pos.is_none()) {
				return Err(SignerError::KeypairPubkeyMismatch)
			}
			let positions: Vec<usize> = positions.iter().map(|pos| pos.unwrap()).collect();
			self.try_partial_sign_unchecked(signers, positions, recent_blockhash)
		}

		#[cfg(feature = "std")]
		fn try_partial_sign_unchecked<S: Signer>(
			&mut self,
			signers: TestSigners<S>,
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

			let signatures = signers.try_sign_message(&self.message_data())?;
			for i in 0..positions.len() {
				self.signatures[positions[i]] = signatures[i];
			}
			Ok(())
		}

		#[cfg(feature = "std")]
		fn get_signing_keypair_positions(
			&self,
			pubkeys: Vec<Pubkey>,
		) -> Result<Vec<Option<usize>>, TransactionError> {
			if self.message.account_keys.len() <
				self.message.header.num_required_signatures as usize
			{
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
			self.signatures.iter().all(|signature| *signature != Signature::default())
		}

		/// Return the message containing all data that should be signed.
		pub fn message(&self) -> &LegacyMessage {
			&self.message
		}

		/// Return the serialized message data to sign.
		pub fn message_data(&self) -> Vec<u8> {
			self.message().serialize()
		}

		/// Due to different Serialization between Signature and Solana native Signature type,
		/// the Signatures needs to be converted into the RawSignature type before the
		/// transaction is serialized as whole.
		pub fn finalize_and_serialize(self) -> Result<Vec<u8>, bincode::error::EncodeError> {
			bincode::serde::encode_to_vec(RawTransaction::from(self), bincode::config::legacy())
		}
	}
}

/// Internal raw transaction type used for correct Serialization and Encoding
#[derive(Debug, PartialEq, Default, Eq, Clone, Serialize, Deserialize)]
struct RawTransaction<Message> {
	#[serde(with = "short_vec")]
	pub signatures: Vec<RawSignature>,
	pub message: Message,
}

impl From<legacy::LegacyTransaction> for RawTransaction<legacy::LegacyMessage> {
	fn from(from: legacy::LegacyTransaction) -> Self {
		Self {
			signatures: from.signatures.into_iter().map(RawSignature::from).collect(),
			message: from.message,
		}
	}
}

impl From<VersionedTransaction> for RawTransaction<VersionedMessage> {
	fn from(from: VersionedTransaction) -> Self {
		Self {
			signatures: from.signatures.into_iter().map(RawSignature::from).collect(),
			message: from.message,
		}
	}
}
