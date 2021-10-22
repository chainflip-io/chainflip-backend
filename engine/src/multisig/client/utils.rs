use std::collections::HashMap;

use crate::p2p::AccountId;

use serde::{Deserialize, Serialize};

/// Note that the resulting `threshold` is the maximum number
/// of parties *not* enough to generate a signature,
/// i.e. at least `t+1` parties are required.
/// This follows the notation in the multisig library that
/// we are using and in the corresponding literature.
pub fn threshold_from_share_count(share_count: usize) -> usize {
    ((share_count * 2) - 1) / 3
}

#[cfg(test)]
#[test]
fn check_threshold_calculation() {
    assert_eq!(threshold_from_share_count(150), 99);
    assert_eq!(threshold_from_share_count(100), 66);
    assert_eq!(threshold_from_share_count(90), 59);
    assert_eq!(threshold_from_share_count(3), 1);
    assert_eq!(threshold_from_share_count(4), 2);
}

/// Mappings from signer_idx to Validator Id and back
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ValidatorMaps {
    id_to_idx: HashMap<AccountId, usize>,
    // TODO: create SortedVec and use it here:
    // Sorted Validator Ids
    validator_ids: Vec<AccountId>,
}

impl ValidatorMaps {
    pub fn get_idx(&self, id: &AccountId) -> Option<usize> {
        self.id_to_idx.get(id).copied()
    }

    pub fn get_id(&self, idx: usize) -> Option<&AccountId> {
        let idx = idx.checked_sub(1)?;
        self.validator_ids.get(idx)
    }
}

pub fn get_index_mapping(signers: &[AccountId]) -> ValidatorMaps {
    let idxs: Vec<_> = (1..=signers.len()).collect();

    debug_assert_eq!(idxs.len(), signers.len());

    let mut combined: Vec<_> = signers.iter().zip(idxs).collect();

    combined.sort_by_key(|(v, _)| *v);

    let mut id_to_idx = HashMap::new();

    let mut sorted_validator_ids = Vec::with_capacity(signers.len());

    for (i, (vid, _)) in combined.into_iter().enumerate() {
        // indexes start with 1 for siging
        id_to_idx.insert(vid.clone(), i + 1);
        sorted_validator_ids.push(vid.clone());
    }

    ValidatorMaps {
        id_to_idx,
        validator_ids: sorted_validator_ids,
    }
}

// TODO: should this be a part of ValidatorMaps?
/// Map all signer ids to their corresponding signer idx
pub fn project_signers(
    signer_ids: &[AccountId],
    validator_maps: &ValidatorMaps,
) -> Result<Vec<usize>, ()> {
    signer_ids
        .iter()
        .map(|id| validator_maps.get_idx(id).ok_or(()))
        .collect()
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
            type Error = &'static str;

            fn try_from(data: $enum) -> Result<Self, Self::Error> {
                if let $variant_path(x) = data {
                    Ok(x)
                } else {
                    Err(stringify!($enum))
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
        let a = AccountId(['A' as u8; 32]);
        let b = AccountId(['B' as u8; 32]);
        let c = AccountId(['C' as u8; 32]);

        let signers = [a, c.clone(), b];

        let map = get_index_mapping(&signers);

        assert_eq!(map.get_idx(&c), Some(3));
        assert_eq!(map.get_id(3), Some(&c));
    }
}
