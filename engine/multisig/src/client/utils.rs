use std::collections::{BTreeMap, BTreeSet};

use cf_primitives::AuthorityCount;
use itertools::Itertools;
use state_chain_runtime::AccountId;

use serde::{Deserialize, Serialize};

/// Find an element that appears more than `threshold` times
pub fn find_frequent_element<T, Iter>(iter: Iter, threshold: usize) -> Option<T>
where
	T: Clone + std::fmt::Debug + Ord,
	Iter: Iterator<Item = T>,
{
	iter.sorted_unstable()
		.group_by(|x| x.clone())
		.into_iter()
		.map(|(_, mut group)| {
			let first = group.next().expect("must have at least one element");
			(first, group.count() + 1)
		})
		.find(|(_, count)| *count > threshold)
		.map(|(x, _)| x)
}

/// The threshold that determines the number of parties that we must exceed
/// in order to agree on the outcome of some broadcast.
pub fn threshold_for_broadcast_verification(total_parties: usize) -> usize {
	// We require (one more than) half of participants to agree in order to
	// maximise the number of colluding parties required to do harm. Note that
	// if we used the usual 2/3 threshold, it would only take 1/3 of colluding
	// participants to result in slashing of honest participants.
	total_parties / 2
}

#[test]
fn test_threshold_for_broadcast_verification() {
	assert_eq!(threshold_for_broadcast_verification(1), 0);
	assert_eq!(threshold_for_broadcast_verification(2), 1);
	assert_eq!(threshold_for_broadcast_verification(3), 1);
	assert_eq!(threshold_for_broadcast_verification(99), 49);
	assert_eq!(threshold_for_broadcast_verification(100), 50);
	assert_eq!(threshold_for_broadcast_verification(150), 75);
}

/// Mappings from signer_idx to Validator Id and back
/// for the corresponding ceremony
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PartyIdxMapping {
	id_to_idx: BTreeMap<AccountId, AuthorityCount>,
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

impl Serialize for PartyIdxMapping {
	fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
		// We leave off the id_to_idx because they can be derived from the account_ids during
		// deserialization
		self.account_ids.serialize(serializer)
	}
}

impl<'de> Deserialize<'de> for PartyIdxMapping {
	fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
		let account_ids = BTreeSet::<AccountId>::deserialize(deserializer)?;
		Ok(PartyIdxMapping::from_participants(account_ids))
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

#[cfg(test)]
mod utils_tests {
	use utilities::assert_panics;

	use crate::client::helpers::ACCOUNT_IDS;

	use super::*;

	#[test]
	fn test_party_idx_mapping_serialization() {
		let party_mapping =
			PartyIdxMapping::from_participants(BTreeSet::from_iter(ACCOUNT_IDS.iter().cloned()));

		let serialized = bincode::serialize(&party_mapping).unwrap();
		let deserialized: PartyIdxMapping = bincode::deserialize(&serialized).unwrap();

		assert_eq!(party_mapping, deserialized);
	}

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
		assert_eq!(find_frequent_element::<u32, _>([].into_iter(), 3), None);
	}
}
