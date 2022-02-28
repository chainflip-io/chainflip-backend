use std::collections::{BTreeSet, HashMap};

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

/// Check that the reported indexes match the expected ones exactly
fn check_verification_message_indexes<T>(
    message: &BroadcastVerificationMessage<T>,
    expected_idxs: &BTreeSet<usize>,
) -> bool
where
    T: Clone,
{
    let received_idxs: BTreeSet<_> = message.data.keys().copied().collect();

    &received_idxs == expected_idxs
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
    T: Clone + serde::Serialize + serde::de::DeserializeOwned + std::fmt::Debug,
{
    let num_parties = verification_messages.len();

    // We know these indexes to be correct, as this data structure is constructed
    // locally based on ceremony parameters
    let participating_idxs: BTreeSet<_> = verification_messages.keys().copied().collect();

    // Even if we haven't received data from all parties at this point, we
    // might still be able to recover as long as there is a quorum agreement
    // on every value.
    let verification_messages: HashMap<_, _> = verification_messages
        .into_iter()
        .filter_map(|(k, v)| v.map(|unwrapped_v| (k, unwrapped_v)))
        // We ignore all messages that don't contain all (and only) expected signer indexes
        .filter(|(_sender, messages)| {
            check_verification_message_indexes(messages, &participating_idxs)
        })
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

    for idx in &participating_idxs {
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
    fn to_broadcast_verification_messages(
        test_data: Vec<(usize, Option<Vec<Option<i32>>>)>,
    ) -> HashMap<usize, Option<BroadcastVerificationMessage<i32>>> {
        test_data
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
            .collect()
    }

    /// check that the result matches `expected` (transforming Vec into a Set
    /// to make it *NOT* sensitive to the order of elements)
    fn check_broadcast_verification(
        verification_messages: HashMap<usize, Option<BroadcastVerificationMessage<i32>>>,
        expected: Result<Vec<(usize, i32)>, Vec<usize>>,
    ) {
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

        let all_messages = to_broadcast_verification_messages(vec![
            (1usize, Some(vec![Some(1), Some(1), Some(1), Some(1)])),
            (2, Some(vec![Some(1), None, Some(1), Some(1)])),
            (3, Some(vec![Some(2), Some(1), None, Some(1)])),
            (4, Some(vec![Some(1), Some(1), Some(1), Some(2)])),
        ]);

        // Expect all to agree on the following values:
        check_broadcast_verification(all_messages, Ok(vec![(1, 1), (2, 1), (3, 1), (4, 1)]));
    }

    #[test]
    fn check_incorrect_broadcast() {
        // We can't achieve consensus on values from parties
        // 2 and 4 (indexes in inner vectors), which we assume
        // is due to them sending messages inconsistently

        let all_messages = to_broadcast_verification_messages(vec![
            (1usize, Some(vec![Some(1), None, Some(1), Some(2)])),
            (2, Some(vec![Some(1), None, Some(1), Some(1)])),
            (3, Some(vec![Some(2), Some(1), Some(2), Some(1)])),
            (4, Some(vec![Some(1), Some(1), Some(1), Some(2)])),
        ]);

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
        let all_messages = to_broadcast_verification_messages(vec![
            (1usize, Some(vec![Some(1), Some(1), Some(1), Some(1)])),
            (2, Some(vec![Some(1), Some(1), Some(1), Some(1)])),
            (3, None),
            (4, Some(vec![Some(1), Some(1), Some(1), Some(1)])),
        ]);

        // Expect all to agree on the following values:
        check_broadcast_verification(all_messages, Ok(vec![(1, 1), (2, 1), (3, 1), (4, 1)]));
    }

    #[test]
    fn can_recover_from_missing_signer_indexes() {
        // Note that party 2's message is missing an "inner" message
        // for party 4.
        let all_messages = to_broadcast_verification_messages(vec![
            (1usize, Some(vec![Some(1), Some(1), Some(1), Some(1)])),
            (2, Some(vec![Some(1), Some(1), Some(1)])),
            (3, Some(vec![Some(1), Some(1), Some(1), Some(1)])),
            (4, Some(vec![Some(1), Some(1), Some(1), Some(1)])),
        ]);

        // Expect all to agree on the following values:
        check_broadcast_verification(all_messages, Ok(vec![(1, 1), (2, 1), (3, 1), (4, 1)]));
    }

    #[test]
    fn can_recover_from_extraneous_signer_indexes() {
        // Note that party 2's message contains an extra message
        // for non-existent party 5.
        let all_messages = to_broadcast_verification_messages(vec![
            (1usize, Some(vec![Some(1), Some(1), Some(1), Some(1)])),
            (2, Some(vec![Some(1), Some(1), Some(1), Some(1), Some(1)])),
            (3, Some(vec![Some(1), Some(1), Some(1), Some(1)])),
            (4, Some(vec![Some(1), Some(1), Some(1), Some(1)])),
        ]);

        // Expect all to agree on the following values:
        check_broadcast_verification(all_messages, Ok(vec![(1, 1), (2, 1), (3, 1), (4, 1)]));
    }

    #[test]
    fn can_recover_from_unexpected_signer_indexes() {
        // Note that party 2's message is missing an "inner" message
        // for party 4. It will be "replaced" by a non-existent index below
        let mut all_messages = to_broadcast_verification_messages(vec![
            (1usize, Some(vec![Some(1), Some(1), Some(1), Some(1)])),
            (2, Some(vec![Some(1), Some(1), Some(1)])),
            (3, Some(vec![Some(1), Some(1), Some(1), Some(1)])),
            (4, Some(vec![Some(1), Some(1), Some(1), Some(1)])),
        ]);

        // Insert a non-existent index 5 for party 2 (the number of messages is correct however)
        all_messages
            .get_mut(&2)
            .unwrap()
            .as_mut()
            .unwrap()
            .data
            .insert(5, None);

        // Expect all to agree on the following values:
        check_broadcast_verification(all_messages, Ok(vec![(1, 1), (2, 1), (3, 1), (4, 1)]));
    }
}
