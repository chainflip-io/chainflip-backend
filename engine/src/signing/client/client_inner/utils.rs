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

pub fn reorg_vector<T: Clone>(v: &mut Vec<T>, order: &[usize]) {
    assert_eq!(v.len(), order.len());

    let owned_v = v.split_off(0);

    let mut combined: Vec<_> = owned_v.into_iter().zip(order.iter()).collect();

    combined.sort_by_key(|(_data, idx)| *idx);

    *v = combined.into_iter().map(|(data, _idx)| data).collect();
}

#[cfg(test)]
#[test]
fn reorg_vector_works() {
    {
        let mut v = vec![1, 2, 3];
        let order = [2, 1, 3];
        reorg_vector(&mut v, &order);
        assert_eq!(v, [2, 1, 3]);
    }

    {
        let mut v = vec![2, 1, 3];
        let order = [3, 2, 1];
        reorg_vector(&mut v, &order);
        assert_eq!(v, [3, 1, 2]);
    }
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

// TODO: remove this in favor of always using ValidatorMaps?
/// Sort validators and find our index
pub fn get_our_idx(signers: &[AccountId], id: &AccountId) -> Option<usize> {
    let mut signers = signers.to_owned();

    signers.sort();

    let pos = signers.iter().position(|s| s == id);

    // idx in multisig start at 1
    pos.map(|idx| idx + 1)
}

// TODO: should this be a part of ValidatorMaps?
/// Map all signer ids to their corresponding signer idx
pub fn project_signers(
    signer_ids: &[AccountId],
    validator_maps: &ValidatorMaps,
) -> Result<Vec<usize>, ()> {
    // There is probably a more efficient way of doing this
    // for for now this should be good enough

    let mut results = Vec::with_capacity(signer_ids.len());
    for id in signer_ids {
        let idx = validator_maps.get_idx(id).ok_or(())?;
        results.push(idx);
    }

    Ok(results)
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
    fn get_our_idx_works() {
        let a = AccountId(['A' as u8; 32]);
        let b = AccountId(['B' as u8; 32]);
        let c = AccountId(['C' as u8; 32]);

        let signers = [c, a, b.clone()];

        let idx = get_our_idx(&signers, &b);

        // AccountID 'b' is in the second position in the list.
        assert_eq!(idx, Some(2));
    }

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
