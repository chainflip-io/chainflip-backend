use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use utilities::threshold_from_share_count;

/// Data received by a single party for a given
/// stage from all parties (includes our own for
/// simplicity). Used for broadcast verification.
/// `None` indicates that the data hasn't been received.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BroadcastVerificationMessage<T: Clone> {
    pub data: HashMap<usize, Option<T>>,
}

fn hash<T: Clone + Serialize>(data: &T) -> [u8; 32] {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();

    hasher.update(bincode::serialize(data).unwrap());

    *hasher.finalize().as_ref()
}

// This might result in an error if we don't get 2/3 of parties agreeing on the same value.
// If we don't, this means that either (a) the broadcaster did an inconsistent broadcast,
// (b) that the broadcaster failed to deliver the message to large enough number of parties,
// or (c) that ~1/3 of parties colluded to slash the broadcasting party. (Should we reduce
// the threshold to 50% for symmetry?)
pub fn verify_broadcasts<T>(
    verification_messages: HashMap<usize, Option<BroadcastVerificationMessage<T>>>,
) -> Result<HashMap<usize, T>, Vec<usize>>
where
    T: Clone + serde::Serialize + serde::de::DeserializeOwned,
{
    let num_parties = verification_messages.len();

    // Even if we haven't received data from all parties at this point, we
    // might still be able to recover as long as there is a quorum agreement
    // on every value.
    let verification_messages: HashMap<_, _> = verification_messages
        .into_iter()
        .filter_map(|(k, v)| v.map(|unwrapped_v| (k, unwrapped_v)))
        .collect();

    assert!(verification_messages
        .iter()
        .all(|(_, m)| m.data.len() == num_parties));

    let threshold = threshold_from_share_count(num_parties as u32) as usize;

    // NOTE: ideally we wouldn't need to serialize the messages again here, but
    // we can't use T as key directly (in our case it holds third-party structs)
    // and delaying deserialization when we receive these over p2p would would make
    // our code more complicated than necessary.

    let mut agreed_on_values = HashMap::<usize, T>::new();

    let mut blamed_parties = vec![];

    // We know all indexes to be correct (and the same for all senders) as our
    // node constructed this datastructure locally
    let participating_idxs = verification_messages
        .iter()
        .next()
        .expect("must be non-empty")
        .1
        .data
        .keys();

    for idx in participating_idxs {
        use itertools::Itertools;

        if let Some((Some(data), _)) = verification_messages
            .values()
            .map(|m| (m.data[idx].clone(), hash::<Option<T>>(&m.data[idx])))
            .sorted_by_key(|(_, hash)| *hash)
            .group_by(|(_, hash)| *hash)
            .into_iter()
            .map(|(_, mut group)| {
                let first = group.next().expect("must have at least one element").0;
                (first, group.count() + 1)
            })
            .find(|(_, count)| *count > threshold)
        {
            agreed_on_values.insert(*idx, data);
        } else {
            blamed_parties.push(*idx);
        }
    }

    if blamed_parties.is_empty() {
        Ok(agreed_on_values)
    } else {
        Err(blamed_parties)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// Transforms the (more concise) test data into the expected "shape";
    /// check that the result matches `expected` (transforming Vec into a Set
    /// to make it *NOT* sensitive to the order of elements)
    fn check_broadcast_verification(
        test_data: Vec<(usize, Option<Vec<Option<i32>>>)>,
        expected: Result<Vec<(usize, i32)>, Vec<usize>>,
    ) {
        let verification_messages: HashMap<_, _> = test_data
            .into_iter()
            .map(|(idx, opt_values)| {
                let opt_data = opt_values.map(|values| {
                    let data: HashMap<_, _> = values
                        .iter()
                        .enumerate()
                        .map(|(i, d)| (i + 1, *d))
                        .collect();

                    BroadcastVerificationMessage { data }
                });

                (idx, opt_data)
            })
            .collect();

        let res = verify_broadcasts(verification_messages)
            .map_err(|reported_idxs| reported_idxs.iter().copied().collect::<BTreeSet<usize>>());

        let expected = expected
            .map(|values| values.into_iter().collect::<HashMap<_, _>>())
            .map_err(|reported_idxs| reported_idxs.iter().copied().collect::<BTreeSet<usize>>());

        assert_eq!(res, expected);
    }

    #[test]
    fn check_correct_broadcast() {
        // There is a consensus on each of the values,
        // even though some parties disagree on some values

        let all_messages = vec![
            (1usize, Some(vec![Some(1), Some(1), Some(1), Some(1)])),
            (2, Some(vec![Some(1), None, Some(1), Some(1)])),
            (3, Some(vec![Some(2), Some(1), None, Some(1)])),
            (4, Some(vec![Some(1), Some(1), Some(1), Some(2)])),
        ];

        // Expect all to agree on the following values:
        check_broadcast_verification(all_messages, Ok(vec![(1, 1), (2, 1), (3, 1), (4, 1)]));
    }

    #[test]
    fn check_incorrect_broadcast() {
        // We can't achieve consensus on values from parties
        // 2 and 4 (indexes in inner vectors), which we assume
        // is due to them sending messages inconsistently

        let all_messages = vec![
            (1usize, Some(vec![Some(1), None, Some(1), Some(2)])),
            (2, Some(vec![Some(1), None, Some(1), Some(1)])),
            (3, Some(vec![Some(2), Some(1), Some(2), Some(1)])),
            (4, Some(vec![Some(1), Some(1), Some(1), Some(2)])),
        ];

        // Expect parties 2 and 4 to be reported
        check_broadcast_verification(all_messages, Err(vec![2, 4]));
    }

    #[test]
    fn can_recover_from_small_number_of_missing_messages() {
        // If a small number of parties timeout during a
        // broadcast verification stage, we should be able
        // to recover the missing messages (even if the
        // recovered message is `None`)

        // Note that party 3's message is missing
        let all_messages = vec![
            (1usize, Some(vec![Some(1), Some(1), Some(1), Some(1)])),
            (2, Some(vec![Some(1), Some(1), Some(1), Some(1)])),
            (3, None),
            (4, Some(vec![Some(1), Some(1), Some(1), Some(1)])),
        ];

        // Expect all to agree on the following values:
        check_broadcast_verification(all_messages, Ok(vec![(1, 1), (2, 1), (3, 1), (4, 1)]));
    }
}
