use std::collections::{BTreeMap, BTreeSet};

use cf_primitives::AuthorityCount;
use serde::{Deserialize, Serialize};
use tracing::warn;
use utils::threshold_from_share_count;

use crate::multisig::client::utils::find_frequent_element;

use super::BroadcastFailureReason;

/// Data received by a single party for a given
/// stage from all parties (includes our own for
/// simplicity). Used for broadcast verification.
/// `None` indicates that the data hasn't been received.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BroadcastVerificationMessage<T: Clone> {
	pub data: BTreeMap<AuthorityCount, Option<T>>,
}

/// Check that the reported indexes match the expected ones exactly
fn check_verification_message_indexes<T>(
	message: &BroadcastVerificationMessage<T>,
	expected_idxs: &BTreeSet<AuthorityCount>,
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
	verification_messages: BTreeMap<AuthorityCount, Option<BroadcastVerificationMessage<T>>>,
) -> Result<BTreeMap<AuthorityCount, T>, (BTreeSet<AuthorityCount>, BroadcastFailureReason)>
where
	T: Clone + serde::Serialize + serde::de::DeserializeOwned + std::fmt::Debug,
{
	let num_parties = verification_messages.len();
	let threshold = threshold_from_share_count(num_parties as AuthorityCount) as usize;

	// We know these indexes to be correct, as this data structure is constructed
	// locally based on ceremony parameters
	let participating_idxs: BTreeSet<_> = verification_messages.keys().copied().collect();

	// Even if we haven't received data from all parties at this point, we
	// might still be able to recover as long as there is a quorum agreement
	// on every value.
	let verification_messages: BTreeMap<_, _> = verification_messages
		.into_iter()
		.filter_map(|(k, v)| v.map(|unwrapped_v| (k, unwrapped_v)))
		// We ignore all messages that don't contain all (and only) expected signer indexes
		.filter(|(sender, message)| {
			let valid = check_verification_message_indexes(message, &participating_idxs);
			if !(valid) {
				warn!("Disregarding verification message from: {sender}");
			}
			valid
		})
		.collect();

	// Too few messages during this broadcast verification stage
	if verification_messages.len() <= threshold {
		// TODO: consider reporting the parties that didn't send broadcast verification messages
		// (one thing to consider is whether we are going to be in trouble if we report more parties
		// than other nodes?)
		return Err((BTreeSet::new(), BroadcastFailureReason::InsufficientVerificationMessages))
	}

	// This should not panic due to the check above (`check_verification_message_indexes`)
	assert!(verification_messages.iter().all(|(_, m)| m.data.len() == num_parties));

	// NOTE: ideally we wouldn't need to serialize the messages again here, but
	// we can't use T as key directly (in our case it holds third-party structs)
	// and delaying deserialization when we receive these over p2p would would make
	// our code more complicated than necessary.

	// Assume Some(x) are all the same (there is no inconsistency), do we have enough non-None
	// messages for all parties? yes: if we end up failing anyway, then it must be due to
	// inconsistency no: due to "too few messages"
	let insufficient_messages = participating_idxs.iter().any(|idx| {
		// Check if we have enough delivered messages for each idx to reach the threshold + 1
		verification_messages.iter().filter(|(_, m)| m.data[idx].is_some()).count() <= threshold
	});

	let mut agreed_on_values = BTreeMap::<AuthorityCount, T>::new();

	let mut reported_parties = BTreeSet::new();

	// Check that the values are agreed on by the threshold majority.
	// A party is reported if we can't agree on the value they broadcast
	// or if the agreed upon value is `None` (i.e. they didn't broadcast)
	for idx in &participating_idxs {
		let message_iter = verification_messages.values().map(|m| m.data[idx].clone());
		if let Some(Some(data)) = find_frequent_element(message_iter, threshold) {
			agreed_on_values.insert(*idx, data);
		} else {
			reported_parties.insert(*idx);
		}
	}

	if reported_parties.is_empty() {
		Ok(agreed_on_values)
	} else {
		Err((
			reported_parties,
			if insufficient_messages {
				BroadcastFailureReason::InsufficientMessages
			} else {
				// If the failure was not due to "InsufficientMessages",
				// then it must be caused by (or at least partially caused by) inconsistency.
				BroadcastFailureReason::Inconsistency
			},
		))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::collections::BTreeSet;

	/// Transforms the (more concise) test data into the expected "shape";
	fn to_broadcast_verification_messages(
		test_data: Vec<(AuthorityCount, Option<Vec<Option<i32>>>)>,
	) -> BTreeMap<AuthorityCount, Option<BroadcastVerificationMessage<i32>>> {
		test_data
			.into_iter()
			.map(|(idx, opt_values)| {
				let opt_data = opt_values.map(|values| {
					let data: BTreeMap<_, _> = values
						.iter()
						.enumerate()
						.map(|(i, d)| (i as AuthorityCount + 1, *d))
						.collect();

					BroadcastVerificationMessage { data }
				});

				(idx, opt_data)
			})
			.collect()
	}

	/// check that the result matches `expected` (transforming the reported idxs Vec into a Set
	/// to make it *NOT* sensitive to the order of elements)
	fn check_broadcast_verification(
		verification_messages: BTreeMap<AuthorityCount, Option<BroadcastVerificationMessage<i32>>>,
		expected: Result<
			Vec<(AuthorityCount, i32)>,
			(BTreeSet<AuthorityCount>, BroadcastFailureReason),
		>,
	) {
		let expected = expected.map(|values| values.into_iter().collect::<BTreeMap<_, _>>());

		assert_eq!(verify_broadcasts(verification_messages), expected);
	}

	#[test]
	fn check_correct_broadcast() {
		// There is a consensus on each of the values,
		// even though some parties disagree on some values

		let all_messages = to_broadcast_verification_messages(vec![
			(1_u32, Some(vec![Some(1), Some(1), Some(1), Some(1)])),
			(2, Some(vec![Some(1), None, Some(1), Some(1)])),
			(3, Some(vec![Some(2), Some(1), None, Some(1)])),
			(4, Some(vec![Some(1), Some(1), Some(1), Some(2)])),
		]);

		// Expect all to agree on the following values:
		check_broadcast_verification(all_messages, Ok(vec![(1, 1), (2, 1), (3, 1), (4, 1)]));
	}

	#[test]
	fn fail_from_inconsistent_broadcast() {
		// We can't achieve consensus on values from parties
		// 2 and 4 (indexes in inner vectors), which we assume
		// is due to them sending messages inconsistently

		let all_messages = to_broadcast_verification_messages(vec![
			(1_u32, Some(vec![Some(1), None, Some(1), Some(2)])),
			(2, Some(vec![Some(1), Some(2), Some(1), Some(1)])),
			(3, Some(vec![Some(2), Some(2), Some(2), Some(1)])),
			(4, Some(vec![Some(1), Some(1), Some(1), Some(2)])),
		]);

		// Expect parties 2 and 4 to be reported
		check_broadcast_verification(
			all_messages,
			Err((vec![2, 4].iter().copied().collect(), BroadcastFailureReason::Inconsistency)),
		);
	}

	#[test]
	fn fail_from_missing_messages() {
		// We can't achieve consensus on values from 2
		// because 4 is missing all messages and 3 is missing one message from 2

		let all_messages = to_broadcast_verification_messages(vec![
			(1_u32, Some(vec![Some(1), Some(1), Some(1), Some(1)])),
			(2, Some(vec![Some(1), Some(1), Some(1), Some(1)])),
			(3, Some(vec![Some(1), None, Some(1), Some(1)])),
			(4, None),
		]);

		// Expect party 2 to be reported
		check_broadcast_verification(
			all_messages,
			Err((vec![2].iter().copied().collect(), BroadcastFailureReason::InsufficientMessages)),
		);
	}

	#[test]
	fn fail_from_missing_messages_during_broadcast_verification() {
		// We are missing broadcast verification messages from 3 and 4.

		let all_messages = to_broadcast_verification_messages(vec![
			(1_u32, Some(vec![Some(1), Some(1), Some(1), Some(1)])),
			(2, Some(vec![Some(1), Some(1), Some(1), Some(1)])),
			(3, None),
			(4, None),
		]);

		// Expect no parties to be reported
		check_broadcast_verification(
			all_messages,
			Err((BTreeSet::new(), BroadcastFailureReason::InsufficientVerificationMessages)),
		);
	}

	#[test]
	fn can_recover_from_small_number_of_missing_messages() {
		// If a small number of parties timeout during a
		// broadcast verification stage, we should be able
		// to recover the missing messages (even if the
		// recovered message is `None`)

		// Note that party 3's message is missing
		let all_messages = to_broadcast_verification_messages(vec![
			(1_u32, Some(vec![Some(1), Some(1), Some(1), Some(1)])),
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
			(1_u32, Some(vec![Some(1), Some(1), Some(1), Some(1)])),
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
			(1_u32, Some(vec![Some(1), Some(1), Some(1), Some(1)])),
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
			(1_u32, Some(vec![Some(1), Some(1), Some(1), Some(1)])),
			(2, Some(vec![Some(1), Some(1), Some(1)])),
			(3, Some(vec![Some(1), Some(1), Some(1), Some(1)])),
			(4, Some(vec![Some(1), Some(1), Some(1), Some(1)])),
		]);

		// Insert a non-existent index 5 for party 2 (the number of messages is correct however)
		all_messages.get_mut(&2).unwrap().as_mut().unwrap().data.insert(5, None);

		// Expect all to agree on the following values:
		check_broadcast_verification(all_messages, Ok(vec![(1, 1), (2, 1), (3, 1), (4, 1)]));
	}
}
