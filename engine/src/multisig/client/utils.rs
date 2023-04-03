use std::collections::{BTreeSet, HashMap};

use cf_primitives::AuthorityCount;
use itertools::Itertools;
use state_chain_runtime::AccountId;

use serde::{Deserialize, Serialize};

fn hash_serialized<T: Clone + Serialize>(data: &T) -> [u8; 32] {
	use sha2::{Digest, Sha256};

	let mut hasher = Sha256::new();

	hasher.update(bincode::serialize(data).unwrap());

	*hasher.finalize().as_ref()
}

/// Find an element that appears more than `threshold` times
pub fn find_frequent_element<T, Iter>(iter: Iter, threshold: usize) -> Option<T>
where
	T: Serialize + Clone + std::fmt::Debug,
	Iter: Iterator<Item = T>,
{
	iter.map(|x| (x.clone(), hash_serialized(&x)))
		.sorted_by_key(|(_, hash)| *hash)
		.group_by(|(_, hash)| *hash)
		.into_iter()
		.map(|(_, mut group)| {
			let first = group.next().expect("must have at least one element").0;
			(first, group.count() + 1)
		})
		.find(|(_, count)| *count > threshold)
		.map(|(x, _)| x)
}

/// Mappings from signer_idx to Validator Id and back
/// for the corresponding ceremony
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PartyIdxMapping {
	id_to_idx: HashMap<AccountId, AuthorityCount>,
	account_ids: BTreeSet<AccountId>,
}

impl PartyIdxMapping {
	/// Get party index based on their account id
	pub fn get_idx(&self, id: &AccountId) -> Option<AuthorityCount> {
		self.id_to_idx.get(id).copied()
	}

	/// Get party account id based on their index
	pub fn get_id(&self, idx: AuthorityCount) -> &AccountId {
		let idx_sub_one = idx.checked_sub(1).expect("Party mapping index must be larger than 0");

		self.account_ids
			.iter()
			.nth(idx_sub_one as usize)
			.unwrap_or_else(|| panic!("Party index of [{idx}] is invalid"))
	}

	/// Map all signer ids to their corresponding signer idx
	#[allow(clippy::result_unit_err)]
	pub fn get_all_idxs(
		&self,
		signer_ids: &BTreeSet<AccountId>,
	) -> Result<BTreeSet<AuthorityCount>, ()> {
		signer_ids.iter().map(|id| self.get_idx(id).ok_or(())).collect()
	}

	/// Convert all indexes to Account Ids. Precondition: the indexes must be
	/// valid for the ceremony
	pub fn get_ids(&self, idxs: BTreeSet<AuthorityCount>) -> BTreeSet<AccountId> {
		idxs.iter().map(|idx| self.get_id(*idx).clone()).collect()
	}

	pub fn get_all_ids(&self) -> &BTreeSet<AccountId> {
		&self.account_ids
	}

	pub fn from_participants(participants: BTreeSet<AccountId>) -> Self {
		assert!(participants.len() <= AuthorityCount::MAX as usize);

		// The protocol requires that the indexes start at 1
		let id_to_idx = participants
			.iter()
			.enumerate()
			.map(|(i, account_id)| (account_id.clone(), i as AuthorityCount + 1))
			.collect();

		PartyIdxMapping { id_to_idx, account_ids: participants }
	}
}

macro_rules! derive_from_enum {
    (impl $(< $( $lt:tt $( : $clt:tt $(+ $dlt:tt )* )? ),+ >)? for $variant: ty, $variant_path: path, $enum: ty) => {
        impl $(< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)? From<$variant> for $enum {
            fn from(x: $variant) -> Self {
                $variant_path(x)
            }
        }
    };
}

macro_rules! derive_try_from_variant {
    (impl $(< $( $lt:tt $( : $clt:tt $(+ $dlt:tt )* )? ),+ >)? for $variant: ty, $variant_path: path, $enum: ty) => {
        impl $(< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)? TryFrom<$enum> for $variant {
            type Error = $enum;

            fn try_from(data: $enum) -> Result<Self, Self::Error> {
                if let $variant_path(x) = data {
                    Ok(x)
                } else {
                    Err(data)
                }
            }
        }
    };
}

macro_rules! derive_impls_for_enum_variants {
    (impl $(< $( $lt:tt $( : $clt:tt $(+ $dlt:tt )* )? ),+ >)? for $variant:ty, $variant_path:path, $enum:ty) => {
        derive_from_enum!(impl $(< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)? for $variant, $variant_path, $enum);
        derive_try_from_variant!(impl $(< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)? for $variant, $variant_path, $enum);
    };
}

/// Derive display to match the type's name
macro_rules! derive_display_as_type_name {
    ( $name:ident $(< $( $lt:tt $( : $clt:tt $(+ $dlt:tt )* )? ),+ >)? ) => {
        impl $(< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)? std::fmt::Display for $name $(< $( $lt ),+ >)?
        {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, stringify!($name))
            }
        }
    }
}

#[cfg(test)]
mod utils_tests {
	use utilities::assert_panics;

	use crate::multisig::client::helpers::ACCOUNT_IDS;

	use super::*;

	#[test]
	fn get_index_mapping_works() {
		let a = AccountId::new([b'A'; 32]);
		let b = AccountId::new([b'B'; 32]);
		let c = AccountId::new([b'C'; 32]);

		let map = PartyIdxMapping::from_participants(BTreeSet::from_iter([a, c.clone(), b]));

		assert_eq!(map.get_idx(&c), Some(3));
		assert_eq!(map.get_id(3), &c);
	}

	#[test]
	fn get_id_panics_if_index_is_zero() {
		let map =
			PartyIdxMapping::from_participants(BTreeSet::from_iter(ACCOUNT_IDS.iter().cloned()));

		assert_panics!(map.get_id(0));
	}

	#[test]
	fn get_id_panics_if_index_is_too_large() {
		let map =
			PartyIdxMapping::from_participants(BTreeSet::from_iter(ACCOUNT_IDS.iter().cloned()));

		assert_panics!(map.get_id((ACCOUNT_IDS.len() + 1) as u32));
	}

	#[test]
	fn check_find_frequent_element() {
		assert_eq!(find_frequent_element([1, 2, 3, 2, 3, 3].into_iter(), 2), Some(3));
		assert_eq!(find_frequent_element([1, 2, 3, 2, 3, 3].into_iter(), 3), None);
	}
}

#[cfg(test)]
pub fn ensure_unsorted<T>(mut v: Vec<T>, seed: u64) -> Vec<T>
where
	T: Clone + Ord,
{
	use rand::prelude::*;

	assert!(v.len() > 1);
	let mut rng = StdRng::seed_from_u64(seed);
	let sorted = v.iter().cloned().sorted().collect::<Vec<_>>();

	while v != sorted {
		v.shuffle(&mut rng);
	}

	v
}
