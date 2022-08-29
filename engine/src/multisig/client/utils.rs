use std::collections::{BTreeSet, HashMap};

use cf_traits::AuthorityCount;
use state_chain_runtime::AccountId;

use serde::{Deserialize, Serialize};

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
    pub fn get_id(&self, idx: AuthorityCount) -> Option<&AccountId> {
        let idx = idx.checked_sub(1)?;
        self.account_ids.iter().nth(idx as usize)
    }

    /// Map all signer ids to their corresponding signer idx
    #[allow(clippy::result_unit_err)]
    pub fn get_all_idxs(
        &self,
        signer_ids: &BTreeSet<AccountId>,
    ) -> Result<BTreeSet<AuthorityCount>, ()> {
        signer_ids
            .iter()
            .map(|id| self.get_idx(id).ok_or(()))
            .collect()
    }

    /// Convert all indexes to Account Ids. Precondition: the indexes must be
    /// valid for the ceremony
    pub fn get_ids(&self, idxs: BTreeSet<AuthorityCount>) -> BTreeSet<AccountId> {
        idxs.iter()
            .map(|idx| {
                self.get_id(*idx)
                    .expect("Precondition violation: unknown idx")
                    .clone()
            })
            .collect()
    }

    pub fn from_participants(participants: BTreeSet<AccountId>) -> Self {
        let mut id_to_idx = HashMap::new();

        for (i, account_id) in participants.iter().enumerate() {
            // indexes start with 1 for signing
            id_to_idx.insert(account_id.clone(), i as AuthorityCount + 1);
        }

        PartyIdxMapping {
            id_to_idx,
            account_ids: participants,
        }
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
    use super::*;

    #[test]
    fn get_index_mapping_works() {
        let a = AccountId::new([b'A'; 32]);
        let b = AccountId::new([b'B'; 32]);
        let c = AccountId::new([b'C'; 32]);

        let signers = BTreeSet::from_iter([a, c.clone(), b]);

        let map = PartyIdxMapping::from_participants(signers);

        assert_eq!(map.get_idx(&c), Some(3));
        assert_eq!(map.get_id(3), Some(&c));
    }
}

#[cfg(test)]
pub fn ensure_unsorted<T>(mut v: Vec<T>, seed: u64) -> Vec<T>
where
    T: Clone + Ord,
{
    use itertools::Itertools;
    use rand::prelude::*;

    assert!(v.len() > 1);
    let mut rng = StdRng::seed_from_u64(seed);
    let sorted = v.iter().cloned().sorted().collect::<Vec<_>>();

    while v != sorted {
        v.shuffle(&mut rng);
    }

    v
}
