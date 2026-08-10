// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

use std::{
	collections::{BTreeMap, BTreeSet},
	marker::PhantomData,
};

use cf_primitives::AuthorityCount;
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::client::utils::{find_frequent_element, threshold_for_broadcast_verification};

use super::BroadcastFailureReason;

/// A wrapper around a multisig message that can be
/// used as part of larger Serialize payloads, but prevents
/// the inner message from being deserialized until explicitly
/// requested. (This is particularly useful in broadcast
/// verification, where we don't need to do expensive calls
/// to deserialize a large number of EC point in order to verify
/// that the broadcast is successful.)
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(transparent)]
pub struct DelayDeserialization<T> {
	pub payload: Vec<u8>,
	_phantom: PhantomData<T>,
}

impl<T: serde::de::DeserializeOwned> DelayDeserialization<T> {
	pub fn new<M: Serialize>(message: &M) -> Self {
		DelayDeserialization {
			payload: bincode::serialize(message).expect("serialization can't fail"),
			_phantom: PhantomData,
		}
	}

	pub fn deserialize(self) -> anyhow::Result<T> {
		use anyhow::Context;
		bincode::deserialize(&self.payload).context("deserialisation failure")
	}
}

/// Data received by a single party for a given
/// stage from all parties (includes our own for
/// simplicity). Used for broadcast verification.
/// `None` indicates that the data hasn't been received.
#[derive(Serialize, Deserialize, Debug, Clone, PartialOrd, Ord, PartialEq, Eq)]
pub struct BroadcastVerificationMessage<T: Clone> {
	pub data: BTreeMap<AuthorityCount, Option<T>>,
}

impl<T: Clone> BroadcastVerificationMessage<DelayDeserialization<T>> {
	/// Checks that there is the correct number of payloads and all payloads are smaller than the
	/// given max size (without deserializing them)
	pub fn is_data_size_valid(
		&self,
		number_of_parties: usize,
		max_payload_size_bytes: usize,
	) -> bool {
		self.data.len() == number_of_parties &&
			self.data
				.values()
				.filter_map(|x| x.as_ref())
				.all(|d| d.payload.len() <= max_payload_size_bytes)
	}
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

/// Decide, for each party, what they are deemed to have broadcast, using a 2/3 quorum over
/// what the *reporters* claim (see `threshold_for_broadcast_verification`).
///
/// The distinction that matters is between the two ways a party can fail to reach quorum:
///
///   - a quorum agrees the party sent *nothing* - attributable, and reported;
///   - no quorum forms either way - the stage fails, but nobody is reported.
///
/// The second case must never produce blame. Claims of non-receipt are unfalsifiable, so a
/// colluding minority large enough to deny quorum could otherwise manufacture agreement that
/// honest parties failed to broadcast, and blame is what gets nodes banned from the retried
/// ceremony.
fn verify_broadcasts<T>(
	verification_messages: BTreeMap<AuthorityCount, Option<BroadcastVerificationMessage<T>>>,
) -> Result<BTreeMap<AuthorityCount, T>, (BTreeSet<AuthorityCount>, BroadcastFailureReason)>
where
	T: Clone + std::fmt::Debug + Ord,
{
	let num_parties = verification_messages.len();
	let threshold = threshold_for_broadcast_verification(num_parties);

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

	let mut agreed_on_values = BTreeMap::<AuthorityCount, T>::new();

	let mut reported_parties = BTreeSet::new();

	// Set if some party's value reached no quorum either way. The stage still fails, but
	// unlike `reported_parties` this is not attributable to anyone.
	let mut unresolved = false;

	for idx in &participating_idxs {
		let message_iter = verification_messages.values().map(|m| m.data[idx].clone());
		match find_frequent_element(message_iter, threshold) {
			Some(Some(data)) => {
				agreed_on_values.insert(*idx, data);
			},
			Some(None) => {
				reported_parties.insert(*idx);
			},
			None => {
				unresolved = true;
			},
		}
	}

	if !reported_parties.is_empty() {
		// A quorum agrees these parties broadcast nothing, so the failure is attributable
		// to them. Any unresolved values are reported through this same error; naming only
		// the attributable parties is what keeps the blame set defensible.
		Err((reported_parties, BroadcastFailureReason::InsufficientMessages))
	} else if unresolved {
		// Deliberately reporting nobody: we can see the ceremony cannot proceed, but not
		// who is at fault, and an honest node must never emit blame it cannot substantiate.
		Err((BTreeSet::new(), BroadcastFailureReason::Inconsistency))
	} else {
		Ok(agreed_on_values)
	}
}

pub async fn verify_broadcasts_non_blocking<T>(
	verification_messages: BTreeMap<AuthorityCount, Option<BroadcastVerificationMessage<T>>>,
) -> Result<BTreeMap<AuthorityCount, T>, (BTreeSet<AuthorityCount>, BroadcastFailureReason)>
where
	T: Clone + std::fmt::Debug + Ord + Send + 'static,
{
	cf_utilities::task_scope::without_blocking(move || verify_broadcasts(verification_messages))
		.await
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

	/// Agreeing on a value requires a 2/3 quorum. 1/2 is not sufficient (requires 5 participants
	/// to differentiate the two cases).
	#[test]
	fn bare_majority_cannot_dictate_values() {
		// Reporters 1-3 are a coalition claiming everyone broadcast 2; reporters 4 and 5
		// honestly report what was really sent.
		let all_messages = to_broadcast_verification_messages(vec![
			(1_u32, Some(vec![Some(2), Some(2), Some(2), Some(2), Some(2)])),
			(2, Some(vec![Some(2), Some(2), Some(2), Some(2), Some(2)])),
			(3, Some(vec![Some(2), Some(2), Some(2), Some(2), Some(2)])),
			(4, Some(vec![Some(1), Some(1), Some(1), Some(1), Some(1)])),
			(5, Some(vec![Some(1), Some(1), Some(1), Some(1), Some(1)])),
		]);

		// Neither value reaches the quorum, so nothing is substituted - and the coalition
		// cannot convert the failure into evictions, because nobody is reported.
		check_broadcast_verification(
			all_messages,
			Err((BTreeSet::new(), BroadcastFailureReason::Inconsistency)),
		);
	}

	/// A quorum agreeing a party broadcast *nothing* is attributable, and is the only
	/// way a party gets reported by this function.
	#[test]
	fn quorum_on_non_receipt_is_attributable() {
		// Nobody received anything from party 2, and all four reporters say so.
		let all_messages = to_broadcast_verification_messages(vec![
			(1_u32, Some(vec![Some(1), None, Some(1), Some(1)])),
			(2, Some(vec![Some(1), None, Some(1), Some(1)])),
			(3, Some(vec![Some(1), None, Some(1), Some(1)])),
			(4, Some(vec![Some(1), None, Some(1), Some(1)])),
		]);

		check_broadcast_verification(
			all_messages,
			Err(([2].into_iter().collect(), BroadcastFailureReason::InsufficientMessages)),
		);
	}

	/// A colluding minority claiming non-receipt must not be able to get an honest party
	/// blamed. Denying quorum is within their power; *affirming* non-receipt is not.
	#[test]
	fn minority_claiming_non_receipt_blames_nobody() {
		// Parties 3 and 4 falsely claim they received nothing from party 1. That is enough
		// to deny party 1's value a quorum, but not enough to affirm non-receipt.
		let all_messages = to_broadcast_verification_messages(vec![
			(1_u32, Some(vec![Some(1), Some(1), Some(1), Some(1)])),
			(2, Some(vec![Some(1), Some(1), Some(1), Some(1)])),
			(3, Some(vec![None, Some(1), Some(1), Some(1)])),
			(4, Some(vec![None, Some(1), Some(1), Some(1)])),
		]);

		// The ceremony cannot proceed, but nobody is punished for it.
		check_broadcast_verification(
			all_messages,
			Err((BTreeSet::new(), BroadcastFailureReason::Inconsistency)),
		);
	}

	#[test]
	fn fail_from_inconsistent_broadcast() {
		// We can't achieve consensus on values from parties 2 and 4 (indexes in inner
		// vectors), which we assume is due to them sending messages inconsistently.
		// Equivocation is not attributable without authenticated messages, so the ceremony
		// fails reporting nobody.

		let all_messages = to_broadcast_verification_messages(vec![
			(1_u32, Some(vec![Some(1), None, Some(1), Some(2)])),
			(2, Some(vec![Some(1), Some(2), Some(1), Some(1)])),
			(3, Some(vec![Some(2), Some(2), Some(2), Some(1)])),
			(4, Some(vec![Some(1), Some(1), Some(1), Some(2)])),
		]);

		// The stage fails, but no parties are reported.
		check_broadcast_verification(
			all_messages,
			Err((BTreeSet::new(), BroadcastFailureReason::Inconsistency)),
		);
	}

	#[test]
	fn fail_from_missing_messages() {
		// We can't achieve consensus on values from 2 because 4 is missing all messages
		// and 3 is missing one message from 2. Two of four parties are faulty here, which
		// is beyond what a 2/3 quorum tolerates (f <= (n-1)/3, i.e. 1 at n = 4), so the
		// non-receipt cannot be affirmed either and nobody is reported.

		let all_messages = to_broadcast_verification_messages(vec![
			(1_u32, Some(vec![Some(1), Some(1), Some(1), Some(1)])),
			(2, Some(vec![Some(1), Some(1), Some(1), Some(1)])),
			(3, Some(vec![Some(1), None, Some(1), Some(1)])),
			(4, None),
		]);

		check_broadcast_verification(
			all_messages,
			Err((BTreeSet::new(), BroadcastFailureReason::Inconsistency)),
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
