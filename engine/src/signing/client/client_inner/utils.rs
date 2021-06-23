use std::collections::HashMap;

use crate::p2p::ValidatorId;

#[allow(dead_code)]
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
#[derive(Clone, Debug)]
pub(super) struct ValidatorMaps {
    id_to_idx: HashMap<ValidatorId, usize>,
    // Sorted Validator Ids
    validator_ids: Vec<ValidatorId>,
}

impl ValidatorMaps {
    pub(super) fn get_idx(&self, id: &ValidatorId) -> Option<usize> {
        self.id_to_idx.get(id).copied()
    }

    pub(super) fn get_id(&self, idx: usize) -> Option<&ValidatorId> {
        let idx = idx.checked_sub(1)?;
        self.validator_ids.get(idx)
    }
}

pub(super) fn get_index_mapping(signers: &[ValidatorId]) -> ValidatorMaps {
    let signers = signers.clone();

    let idxs: Vec<_> = (1..=signers.len()).collect();

    debug_assert_eq!(idxs.len(), signers.len());

    let mut combined: Vec<_> = signers.into_iter().zip(idxs).collect();

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

/// Sort validators and find our index
pub(super) fn get_our_idx(signers: &[ValidatorId], id: &ValidatorId) -> Option<usize> {
    let mut signers = signers.to_owned();

    signers.sort();

    let pos = signers.iter().position(|s| s == id);

    // idx in multisig start at 1
    pos.map(|idx| idx + 1)
}

#[cfg(test)]
mod utils_tests {
    use super::*;

    #[test]
    fn get_our_idx_works() {
        let a = ValidatorId("A".to_string());
        let b = ValidatorId("B".to_string());
        let c = ValidatorId("C".to_string());

        let signers = [c, a, b.clone()];

        let idx = get_our_idx(&signers, &b);

        assert_eq!(idx, Some(2));
    }

    #[test]
    fn get_index_mapping_works() {
        let a = ValidatorId("A".to_string());
        let b = ValidatorId("B".to_string());
        let c = ValidatorId("C".to_string());

        let signers = [a, c.clone(), b];

        let map = get_index_mapping(&signers);

        assert_eq!(map.get_idx(&c), Some(3));
        assert_eq!(map.get_id(3), Some(&c));
    }
}
