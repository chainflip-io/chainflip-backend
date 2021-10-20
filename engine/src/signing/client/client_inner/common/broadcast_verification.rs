use std::collections::HashMap;

use super::super::utils::threshold_from_share_count;
use super::BroadcastVerificationMessage;

// This might result in an error if we don't get 2/3 of parties agreeing on the same value.
// If we don't, this means that either the broadcaster did an inconsitent broadcast or that
// 1/3 of parties colluded to slash the broadcasting party. (Should we reduce the threshold to 50%
// for symmetry?)
pub fn verify_broadcasts<T: Clone + serde::Serialize + serde::de::DeserializeOwned>(
    signer_idxs: &[usize],
    verification_messages: &HashMap<usize, BroadcastVerificationMessage<T>>,
) -> Result<Vec<T>, Vec<usize>> {
    let num_parties = signer_idxs.len();

    // Sanity check: we should have N messages, each containing N messages
    assert_eq!(verification_messages.len(), num_parties);

    assert!(verification_messages
        .iter()
        .all(|(_, m)| m.data.len() == num_parties));

    let threshold = threshold_from_share_count(num_parties);

    // NOTE: ideally we wouldn't need to serialize the messages again here, but
    // we can't use T as key directly (in our case it holds third-party structs)
    // and delaying deserialization when we receive these over p2p would would make
    // our code more complicated than necessary.

    let mut agreed_on_values: Vec<T> = Vec::with_capacity(num_parties);

    let mut blamed_parties = vec![];

    for i in 0..num_parties {
        use itertools::Itertools;

        if let Some((data, _)) = verification_messages
            .values()
            .map(|m| bincode::serialize(&m.data[i]).unwrap())
            .sorted()
            .group_by(|x| x.clone())
            .into_iter()
            .map(|(data, group)| (data, group.count()))
            .find(|(_, count)| *count > threshold)
        {
            let data = bincode::deserialize(&data).unwrap();
            agreed_on_values.push(data);
        } else {
            blamed_parties.push(i + 1);
        }
    }

    if blamed_parties.is_empty() {
        Ok(agreed_on_values)
    } else {
        Err(blamed_parties)
    }
}

#[cfg(test)]
#[test]
fn check_correct_broadcast() {
    let mut verification_messages = HashMap::new();

    // There is a concensus on each of the values,
    // even though some parties disagree on some values

    let all_messages = vec![
        vec![1, 1, 1, 1], // "correct" message
        vec![1, 2, 1, 1],
        vec![2, 1, 2, 1],
        vec![1, 1, 1, 2],
    ];

    for (i, m) in all_messages.into_iter().enumerate() {
        verification_messages.insert(i + 1, BroadcastVerificationMessage { data: m });
    }

    assert_eq!(
        verify_broadcasts(&[1, 2, 3, 4], &verification_messages),
        Ok(vec![1, 1, 1, 1])
    );
}

#[cfg(test)]
#[test]
fn check_incorrect_broadcast() {
    let mut verification_messages = HashMap::new();

    // We can't achieve consensus on values from parties
    // 2 and 4 (indexes in inner vectors), which we assume
    // is due to them sending messages inconsistently

    let all_messages = vec![
        vec![1, 2, 1, 2],
        vec![1, 2, 1, 1],
        vec![2, 1, 2, 1],
        vec![1, 1, 1, 2],
    ];

    for (i, m) in all_messages.into_iter().enumerate() {
        verification_messages.insert(i + 1, BroadcastVerificationMessage { data: m });
    }

    assert_eq!(
        verify_broadcasts(&[1, 2, 3, 4], &verification_messages),
        Err(vec![2, 4])
    );
}
