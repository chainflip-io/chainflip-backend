use std::collections::{BTreeSet, HashMap};

use state_chain_runtime::AccountId;

use serde::{Deserialize, Serialize};

/// Mappings from signer_idx to Validator Id and back
/// for the corresponding ceremony
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PartyIdxMapping {
    id_to_idx: HashMap<AccountId, usize>,
    // TODO: create SortedVec and use it here:
    // Sorted Account Ids
    account_ids: Vec<AccountId>,
}

impl PartyIdxMapping {
    /// Get party index based on their account id
    pub fn get_idx(&self, id: &AccountId) -> Option<usize> {
        self.id_to_idx.get(id).copied()
    }

    /// Get party account id based on their index
    pub fn get_id(&self, idx: usize) -> Option<&AccountId> {
        let idx = idx.checked_sub(1)?;
        self.account_ids.get(idx)
    }

    /// Map all signer ids to their corresponding signer idx
    pub fn get_all_idxs(&self, signer_ids: &[AccountId]) -> Result<BTreeSet<usize>, ()> {
        signer_ids
            .iter()
            .map(|id| self.get_idx(id).ok_or(()))
            .collect()
    }

    /// Convert all indexes to Account Ids. Precondition: the indexes must be
    /// valid for the ceremony
    pub fn get_ids(&self, idxs: Vec<usize>) -> Vec<AccountId> {
        idxs.iter()
            .map(|idx| {
                self.get_id(*idx)
                    .expect("Precondition violation: unknown idx")
                    .clone()
            })
            .collect()
    }

    pub fn from_unsorted_signers(signers: &[AccountId]) -> Self {
        let mut signers = signers.to_owned();
        signers.sort();

        let mut id_to_idx = HashMap::new();

        for (i, account_id) in signers.iter().enumerate() {
            // indexes start with 1 for signing
            id_to_idx.insert(account_id.clone(), i + 1);
        }

        PartyIdxMapping {
            id_to_idx,
            account_ids: signers,
        }
    }
}

macro_rules! derive_from_enum {
    ($variant: ty, $variant_path: path, $enum: ty) => {
        impl From<$variant> for $enum {
            fn from(x: $variant) -> Self {
                $variant_path(x)
            }
        }
    };
}

macro_rules! derive_try_from_variant {
    ($variant: ty, $variant_path: path, $enum: ty) => {
        impl std::convert::TryFrom<$enum> for $variant {
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
    ($variant: ty, $variant_path: path, $enum: ty) => {
        derive_from_enum!($variant, $variant_path, $enum);
        derive_try_from_variant!($variant, $variant_path, $enum);
        derive_display_as_type_name!($variant);
    };
}

/// Derive display to match the type's name
macro_rules! derive_display_as_type_name {
    ($name: ty) => {
        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, stringify!($name))
            }
        }
    };
}

#[cfg(test)]
mod utils_tests {
    use super::*;

    #[test]
    fn get_index_mapping_works() {
        let a = AccountId::new(['A' as u8; 32]);
        let b = AccountId::new(['B' as u8; 32]);
        let c = AccountId::new(['C' as u8; 32]);

        let signers = [a, c.clone(), b];

        let map = PartyIdxMapping::from_unsorted_signers(&signers);

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
