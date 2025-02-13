pub use crate::{
	short_vec, Address, CompileError, CompiledInstruction, Digest, Instruction, Pubkey, Signature,
};
use codec::{Decode, Encode};
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_std::{collections::btree_map::BTreeMap, vec::Vec};

/// Address table lookups describe an on-chain address lookup table to use
/// for loading more readonly and writable accounts in a single tx.
#[derive(
	Serialize, Deserialize, Default, Debug, PartialEq, Eq, Clone, Encode, Decode, TypeInfo,
)]
#[serde(rename_all = "camelCase")]
pub struct MessageAddressTableLookup {
	/// Address lookup table account key
	pub account_key: Pubkey,
	/// List of indexes used to load writable account addresses
	#[serde(with = "short_vec")]
	pub writable_indexes: Vec<u8>,
	/// List of indexes used to load readonly account addresses
	#[serde(with = "short_vec")]
	pub readonly_indexes: Vec<u8>,
}

/// The definition of address lookup table accounts.
///
/// As used by the `crate::message::v0` message format.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct AddressLookupTableAccount {
	pub key: Pubkey,
	pub addresses: Vec<Pubkey>,
}

impl AddressLookupTableAccount {
	pub fn new(key: Address, addresses: Vec<Address>) -> AddressLookupTableAccount {
		AddressLookupTableAccount {
			key: key.into(),
			addresses: addresses.into_iter().map(|a| a.into()).collect::<Vec<_>>(),
		}
	}

	pub fn is_empty(&self) -> bool {
		self.addresses.is_empty()
	}
}

/// Collection of static and dynamically loaded keys used to load accounts
/// during transaction processing.
#[derive(Clone, Default, Debug, Eq)]
pub struct AccountKeys<'a> {
	static_keys: &'a [Pubkey],
	dynamic_keys: Option<&'a LoadedAddresses>,
}

impl<'a> AccountKeys<'a> {
	pub fn new(static_keys: &'a [Pubkey], dynamic_keys: Option<&'a LoadedAddresses>) -> Self {
		Self { static_keys, dynamic_keys }
	}

	/// Returns an iterator of account key segments. The ordering of segments
	/// affects how account indexes from compiled instructions are resolved and
	/// so should not be changed.
	#[inline]
	fn key_segment_iter(&self) -> impl Iterator<Item = &'a [Pubkey]> + Clone {
		if let Some(dynamic_keys) = self.dynamic_keys {
			[self.static_keys, &dynamic_keys.writable, &dynamic_keys.readonly].into_iter()
		} else {
			// empty segments added for branch type compatibility
			[self.static_keys, &[], &[]].into_iter()
		}
	}

	/// Returns the address of the account at the specified index of the list of
	/// message account keys constructed from static keys, followed by dynamically
	/// loaded writable addresses, and lastly the list of dynamically loaded
	/// readonly addresses.
	#[inline]
	pub fn get(&self, mut index: usize) -> Option<&'a Pubkey> {
		for key_segment in self.key_segment_iter() {
			if index < key_segment.len() {
				return Some(&key_segment[index]);
			}
			index = index.saturating_sub(key_segment.len());
		}

		None
	}

	/// Returns the total length of loaded accounts for a message
	#[inline]
	pub fn len(&self) -> usize {
		let mut len = 0usize;
		for key_segment in self.key_segment_iter() {
			len = len.saturating_add(key_segment.len());
		}
		len
	}

	/// Returns true if this collection of account keys is empty
	pub fn is_empty(&self) -> bool {
		self.len() == 0
	}

	/// Iterator for the addresses of the loaded accounts for a message
	#[inline]
	pub fn iter(&self) -> impl Iterator<Item = &'a Pubkey> + Clone {
		self.key_segment_iter().flatten()
	}

	/// Compile instructions using the order of account keys to determine
	/// compiled instruction account indexes.
	///
	/// # Panics
	///
	/// Panics when compiling fails. See [`AccountKeys::try_compile_instructions`]
	/// for a full description of failure scenarios.
	pub fn compile_instructions(&self, instructions: &[Instruction]) -> Vec<CompiledInstruction> {
		self.try_compile_instructions(instructions).expect("compilation failure")
	}

	/// Compile instructions using the order of account keys to determine
	/// compiled instruction account indexes.
	///
	/// # Errors
	///
	/// Compilation will fail if any `instructions` use account keys which are not
	/// present in this account key collection.
	///
	/// Compilation will fail if any `instructions` use account keys which are located
	/// at an index which cannot be cast to a `u8` without overflow.
	pub fn try_compile_instructions(
		&self,
		instructions: &[Instruction],
	) -> Result<Vec<CompiledInstruction>, CompileError> {
		let mut account_index_map = BTreeMap::<&Pubkey, u8>::new();
		for (index, key) in self.iter().enumerate() {
			let index = u8::try_from(index).map_err(|_| CompileError::AccountIndexOverflow)?;
			account_index_map.insert(key, index);
		}

		let get_account_index = |key: &Pubkey| -> Result<u8, CompileError> {
			account_index_map
				.get(key)
				.cloned()
				.ok_or(CompileError::UnknownInstructionKey(*key))
		};

		instructions
			.iter()
			.map(|ix| {
				let accounts: Vec<u8> = ix
					.accounts
					.iter()
					.map(|account_meta| get_account_index(&account_meta.pubkey))
					.collect::<Result<Vec<u8>, CompileError>>()?;

				Ok(CompiledInstruction {
					program_id_index: get_account_index(&ix.program_id)?,
					data: ix.data.clone(),
					accounts,
				})
			})
			.collect()
	}
}

impl PartialEq for AccountKeys<'_> {
	fn eq(&self, other: &Self) -> bool {
		self.iter().zip(other.iter()).all(|(a, b)| a == b)
	}
}

/// Collection of addresses loaded from on-chain lookup tables, split
/// by readonly and writable.
#[derive(Clone, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoadedAddresses {
	/// List of addresses for writable loaded accounts
	pub writable: Vec<Pubkey>,
	/// List of addresses for read-only loaded accounts
	pub readonly: Vec<Pubkey>,
}

impl FromIterator<LoadedAddresses> for LoadedAddresses {
	fn from_iter<T: IntoIterator<Item = LoadedAddresses>>(iter: T) -> Self {
		let (writable, readonly): (Vec<Vec<Pubkey>>, Vec<Vec<Pubkey>>) = iter
			.into_iter()
			.map(|addresses| (addresses.writable, addresses.readonly))
			.unzip();
		LoadedAddresses {
			writable: writable.into_iter().flatten().collect(),
			readonly: readonly.into_iter().flatten().collect(),
		}
	}
}

impl LoadedAddresses {
	/// Checks if there are no writable or readonly addresses
	pub fn is_empty(&self) -> bool {
		self.len() == 0
	}

	/// Combined length of loaded writable and readonly addresses
	pub fn len(&self) -> usize {
		self.writable.len().saturating_add(self.readonly.len())
	}
}
