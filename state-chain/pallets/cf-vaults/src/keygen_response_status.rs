use super::*;

/// Tracks the current state of the keygen ceremony.
#[derive(PartialEq, Eq, Clone, Encode, Decode, TypeInfo, RuntimeDebug)]
#[scale_info(skip_type_params(T, I))]
pub struct KeygenResponseStatus<T: Config<I>, I: 'static = ()> {
	/// The total number of candidates participating in the keygen ceremony.
	candidate_count: AuthorityCount,
	/// The candidates that have yet to reply.
	remaining_candidates: BTreeSet<T::ValidatorId>,
	/// A map of new keys with the number of votes for each key.
	success_votes: BTreeMap<AggKeyFor<T, I>, AuthorityCount>,
	/// A map of the number of blame votes that each keygen participant has received.
	blame_votes: BTreeMap<T::ValidatorId, AuthorityCount>,
}

impl<T: Config<I>, I: 'static> KeygenResponseStatus<T, I> {
	pub fn new(candidates: BTreeSet<T::ValidatorId>) -> Self {
		Self {
			candidate_count: candidates.len() as AuthorityCount,
			remaining_candidates: candidates,
			success_votes: Default::default(),
			blame_votes: Default::default(),
		}
	}

	pub fn candidate_count(&self) -> AuthorityCount {
		self.candidate_count
	}

	pub fn remaining_candidates(&self) -> &BTreeSet<T::ValidatorId> {
		&self.remaining_candidates
	}

	pub fn success_votes(&self) -> &BTreeMap<AggKeyFor<T, I>, AuthorityCount> {
		&self.success_votes
	}

	fn super_majority_threshold(&self) -> AuthorityCount {
		utilities::success_threshold_from_share_count(self.candidate_count)
	}

	pub fn add_success_vote(&mut self, voter: &T::ValidatorId, key: AggKeyFor<T, I>) {
		assert!(self.remaining_candidates.remove(voter));
		*self.success_votes.entry(key).or_default() += 1;

		KeygenSuccessVoters::<T, I>::append(key, voter);
	}

	pub fn add_failure_vote(&mut self, voter: &T::ValidatorId, blamed: BTreeSet<T::ValidatorId>) {
		assert!(self.remaining_candidates.remove(voter));
		for id in blamed {
			*self.blame_votes.entry(id).or_default() += 1
		}

		KeygenFailureVoters::<T, I>::append(voter);
	}

	/// How many candidates are we still awaiting a response from?
	pub fn remaining_candidate_count(&self) -> AuthorityCount {
		self.remaining_candidates.len() as AuthorityCount
	}

	/// Resolves the keygen outcome as follows:
	///
	/// If and only if *all* candidates agree on the same key, return Success.
	///
	/// Otherwise, determine unresponsive, dissenting and blamed nodes and return
	/// `Failure(unresponsive | dissenting | blamed)`
	pub fn resolve_keygen_outcome(self) -> KeygenOutcomeFor<T, I> {
		// If and only if *all* candidates agree on the same key, return success.
		if let Some((key, votes)) = self.success_votes.iter().next() {
			if *votes == self.candidate_count {
				// This *should* be safe since it's bounded by the number of candidates.
				// We may want to revise.
				// See https://github.com/paritytech/substrate/pull/11490
				let _ignored = KeygenSuccessVoters::<T, I>::clear(u32::MAX, None);
				return Ok(*key)
			}
		}

		let super_majority_threshold = self.super_majority_threshold() as usize;

		// We remove who we don't want to punish, and then punish the rest
		if let Some(key) = KeygenSuccessVoters::<T, I>::iter_keys().find(|key| {
			KeygenSuccessVoters::<T, I>::decode_len(key).unwrap_or_default() >=
				super_majority_threshold
		}) {
			KeygenSuccessVoters::<T, I>::remove(key);
		} else if KeygenFailureVoters::<T, I>::decode_len().unwrap_or_default() >=
			super_majority_threshold
		{
			KeygenFailureVoters::<T, I>::kill();
		} else {
			let _empty = KeygenSuccessVoters::<T, I>::clear(u32::MAX, None);
			KeygenFailureVoters::<T, I>::kill();
			log::warn!("Unable to determine a consensus outcome for keygen.");
		}

		Err(KeygenSuccessVoters::<T, I>::drain()
			.flat_map(|(_k, dissenters)| dissenters)
			.chain(KeygenFailureVoters::<T, I>::take())
			.chain(self.blame_votes.into_iter().filter_map(|(id, vote_count)| {
				if vote_count >= super_majority_threshold as u32 {
					Some(id)
				} else {
					None
				}
			}))
			.chain(self.remaining_candidates)
			.collect())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		mock::{new_test_ext, MockRuntime, NEW_AGG_PUB_KEY},
		AggKeyFor, KeygenOutcomeFor, KeygenResponseStatus,
	};
	use cf_chains::mocks::MockAggKey;
	use frame_support::assert_ok;
	use sp_std::collections::btree_set::BTreeSet;

	macro_rules! assert_failure_outcome {
		($ex:expr) => {
			let outcome: KeygenOutcomeFor<MockRuntime> = $ex;
			assert!(matches!(outcome, Err(_)), "Expected failure, got: {:?}", outcome);
		};
	}

	#[test]
	fn test_threshold() {
		// The success threshold is the smallest number of participants that *can* reach consensus.
		assert_eq!(
			KeygenResponseStatus::<MockRuntime, _>::new(BTreeSet::from_iter(0..144))
				.super_majority_threshold(),
			96
		);
		assert_eq!(
			KeygenResponseStatus::<MockRuntime, _>::new(BTreeSet::from_iter(0..145))
				.super_majority_threshold(),
			97
		);
		assert_eq!(
			KeygenResponseStatus::<MockRuntime, _>::new(BTreeSet::from_iter(0..146))
				.super_majority_threshold(),
			98
		);
		assert_eq!(
			KeygenResponseStatus::<MockRuntime, _>::new(BTreeSet::from_iter(0..147))
				.super_majority_threshold(),
			98
		);
		assert_eq!(
			KeygenResponseStatus::<MockRuntime, _>::new(BTreeSet::from_iter(0..148))
				.super_majority_threshold(),
			99
		);
		assert_eq!(
			KeygenResponseStatus::<MockRuntime, _>::new(BTreeSet::from_iter(0..149))
				.super_majority_threshold(),
			100
		);
		assert_eq!(
			KeygenResponseStatus::<MockRuntime, _>::new(BTreeSet::from_iter(0..150))
				.super_majority_threshold(),
			100
		);
		assert_eq!(
			KeygenResponseStatus::<MockRuntime, _>::new(BTreeSet::from_iter(0..151))
				.super_majority_threshold(),
			101
		);
	}

	// Takes an IntoIterator of tuples where the usize represents the number of times
	// we want to repeat the T
	fn n_times<T: Copy>(things: impl IntoIterator<Item = (usize, T)>) -> Vec<T> {
		things
			.into_iter()
			.flat_map(|(n, thing)| std::iter::repeat(thing).take(n).collect::<Vec<_>>())
			.collect()
	}

	fn unanimous(num_candidates: usize, outcome: ReportedOutcome) -> KeygenOutcomeFor<MockRuntime> {
		get_outcome(&n_times([(num_candidates, outcome)]), |_| [])
	}

	fn unanimous_success(num_candidates: usize) -> KeygenOutcomeFor<MockRuntime> {
		unanimous(num_candidates, ReportedOutcome::Success)
	}

	fn unanimous_failure(num_candidates: usize) -> KeygenOutcomeFor<MockRuntime> {
		unanimous(num_candidates, ReportedOutcome::Failure)
	}

	fn get_outcome_simple<F: Fn(u64) -> I, I: IntoIterator<Item = u64>>(
		num_successes: usize,
		num_failures: usize,
		num_bad_keys: usize,
		num_timeouts: usize,
		report_gen: F,
	) -> KeygenOutcomeFor<MockRuntime> {
		get_outcome(
			n_times([
				(num_successes, ReportedOutcome::Success),
				(num_failures, ReportedOutcome::Failure),
				(num_bad_keys, ReportedOutcome::BadKey),
				(num_timeouts, ReportedOutcome::Timeout),
			])
			.as_slice(),
			report_gen,
		)
	}

	#[derive(Copy, Clone, Debug, PartialEq, Eq)]
	enum ReportedOutcome {
		Success,
		/// When a node considers the keygen a success, but votes for a key that is actually not
		/// the correct key (according to the majority of nodes)
		BadKey,
		Failure,
		Timeout,
	}

	fn reported_outcomes(outcomes: &[u8]) -> Vec<ReportedOutcome> {
		outcomes
			.iter()
			.map(|o| match *o as char {
				's' => ReportedOutcome::Success,
				'b' => ReportedOutcome::BadKey,
				'f' => ReportedOutcome::Failure,
				't' => ReportedOutcome::Timeout,
				invalid => panic!("Invalid char {invalid:?} in outcomes."),
			})
			.collect()
	}

	fn get_outcome<F: Fn(u64) -> I, I: IntoIterator<Item = u64>>(
		outcomes: &[ReportedOutcome],
		report_gen: F,
	) -> Result<AggKeyFor<MockRuntime>, BTreeSet<u64>> {
		let mut status = KeygenResponseStatus::<MockRuntime, _>::new(BTreeSet::from_iter(
			1..=outcomes.len() as u64,
		));

		for (index, outcome) in outcomes.iter().enumerate() {
			let id = 1 + index as u64;
			match outcome {
				ReportedOutcome::Success => {
					status.add_success_vote(&id, NEW_AGG_PUB_KEY);
				},
				ReportedOutcome::BadKey => {
					status.add_success_vote(&id, MockAggKey(*b"bad!"));
				},
				ReportedOutcome::Failure => {
					status.add_failure_vote(&id, BTreeSet::from_iter(report_gen(id)));
				},
				ReportedOutcome::Timeout => {},
			}
		}

		let outcome = status.resolve_keygen_outcome();
		assert_eq!(KeygenSuccessVoters::<MockRuntime, _>::iter_keys().next(), None);
		assert!(!KeygenFailureVoters::<MockRuntime, _>::exists());
		outcome
	}

	/// Keygen can *only* succeed if *all* participants are in agreement.
	#[test]
	fn test_success_consensus() {
		new_test_ext().execute_with(|| {
			for n in 3..200 {
				// Full agreement.
				assert_ok!(unanimous_success(n));
				// Any dissenters cause failure.
				assert_failure_outcome!(get_outcome_simple(n - 1, 1, 0, 0, |_| []));
				assert_failure_outcome!(get_outcome_simple(5, 0, 1, 0, |_| []));
				assert_failure_outcome!(get_outcome_simple(5, 0, 0, 1, |_| []));
			}
		});
	}

	#[test]
	fn test_success_dissent() {
		new_test_ext().execute_with(|| {
			for n in 3..200 {
				for dissent in
					[ReportedOutcome::BadKey, ReportedOutcome::Failure, ReportedOutcome::Timeout]
				{
					// a single node is reporting incorrectly
					let outcome = get_outcome(
						&n_times([(n - 1, ReportedOutcome::Success), (1, dissent)]),
						|_| [],
					);
					assert!(
						matches!(
							outcome.clone(),
							Err(blamed) if blamed == BTreeSet::from_iter([n as u64])
						),
						"Expected Failure([{n:?}]), got: {outcome:?}."
					);
				}
			}
		});
	}

	#[test]
	fn test_failure_consensus() {
		new_test_ext().execute_with(|| {
			for n in 3..200 {
				// Full agreement.
				assert_failure_outcome!(unanimous_failure(n));
				// Minority dissent has no effect.
				assert_failure_outcome!(get_outcome_simple(0, n - 1, 1, 0, |_| []));
				assert_failure_outcome!(get_outcome_simple(1, n - 1, 0, 0, |_| []));
				assert_failure_outcome!(get_outcome_simple(0, n - 1, 0, 1, |_| []));
			}
		});
	}

	#[test]
	fn test_failure_dissent() {
		new_test_ext().execute_with(|| {
			// A keygen where no consensus is reached. Half think we failed, half think we suceeded.
			let outcome = get_outcome(
				&n_times([(3, ReportedOutcome::Failure), (3, ReportedOutcome::Success)]),
				|_| [4, 5, 6],
			);
			assert!(
				matches!(
					outcome.clone(),
					Err(blamed) if blamed.is_empty(),
				),
				"Got outcome: {outcome:?}",
			);

			// A keygen where more than `threshold` nodes have reported failure, but there is no
			// final agreement on the guilty parties. Only unresponsive nodes will be reported.
			assert!(matches!(
				get_outcome(
					&n_times([(17, ReportedOutcome::Failure), (7, ReportedOutcome::Timeout)]),
					|id| if id < 16 { [17] } else { [16] }
				),
				Err(blamed) if blamed == BTreeSet::from_iter(18..=24)
			));

			// As above, but some nodes have reported the wrong outcome.
			assert!(matches!(
				get_outcome(
					&n_times([
						(17, ReportedOutcome::Failure),
						(3, ReportedOutcome::BadKey),
						(2, ReportedOutcome::Success),
						(2, ReportedOutcome::Timeout)
					]),
					|id| if id < 16 { [17] } else { [16] }
				),
				Err(blamed) if blamed == BTreeSet::from_iter(18..=24)
			));

			// As above, but some nodes have additionally been voted on.
			assert!(matches!(
				get_outcome(
					&n_times([
						(18, ReportedOutcome::Failure),
						(2, ReportedOutcome::BadKey),
						(2, ReportedOutcome::Success),
						(2, ReportedOutcome::Timeout)
					]),
					|id| if id > 16 { [1, 2] } else { [17, 18] }
				),
				Err(blamed) if blamed == BTreeSet::from_iter(17..=24)
			));
		});
	}

	#[test]
	fn test_blaming_aggregation() {
		new_test_ext().execute_with(|| {
			// First five candidates all report candidate 6, candidate 6 unresponsive.
			let outcome = get_outcome(&reported_outcomes(b"ffffft"), |_| [6]);
			assert!(
				matches!(
					outcome.clone(),
					Err(blamed) if blamed == BTreeSet::from_iter([6])
				),
				"Got outcome: {outcome:?}",
			);

			// First five candidates all report candidate 6, candidate 6 reports 1.
			assert!(matches!(
				get_outcome(&reported_outcomes(b"ffffft"), |id| if id == 6 { [1] } else { [6] }),
				Err(blamed) if blamed == BTreeSet::from_iter([6])
			));

			// First five candidates all report nobody, candidate 6 unresponsive.
			assert!(matches!(
				get_outcome(&reported_outcomes(b"ffffft"), |_| []),
				Err(blamed) if blamed == BTreeSet::from_iter([6])
			));

			// Candidates 3 and 6 unresponsive.
			assert!(matches!(
				get_outcome(&reported_outcomes(b"fftfft"), |_| []),
				Err(blamed) if blamed == BTreeSet::from_iter([3, 6])
			));
			// One candidate unresponsive, one blamed by majority.
			assert!(matches!(
				get_outcome(&reported_outcomes(b"ffffftf"), |id| if id != 3 { [3] } else { [4] }),
				Err(blamed) if blamed == BTreeSet::from_iter([3, 6])
			));

			// One candidate unresponsive, one rogue blames everyone else.
			assert!(matches!(
				get_outcome(&reported_outcomes(b"ffffftf"), |id| {
					if id != 3 {
						vec![3, 6]
					} else {
						vec![1, 2, 4, 5, 6, 7]
					}
				}),
				Err(blamed) if blamed == BTreeSet::from_iter([3, 6])
			));

			let failures = |n| n_times([(n, ReportedOutcome::Failure)]);

			// Candidates don't agree.
			assert!(matches!(
				get_outcome(&failures(6), |id| [(id + 1) % 6]),
				Err(blamed) if blamed.is_empty()
			));

			// Candidate agreement is below reporting threshold.
			assert!(matches!(
				get_outcome(&failures(6), |id| if id < 4 { [6] } else { [2] }),
				Err(blamed) if blamed.is_empty()
			));

			// Candidates agreement just above threshold.
			assert!(matches!(
				get_outcome(&failures(6), |id| if id == 6 { [1] } else { [6] }),
				Err(blamed) if blamed == BTreeSet::from_iter([6])
			));

			// Candidates agree on multiple offenders.
			assert!(matches!(
				get_outcome(&failures(12), |id| if id < 9 { [11, 12] } else { [1, 2] }),
				Err(blamed) if blamed == BTreeSet::from_iter([11, 12])
			));

			// Overlapping agreement - no agreement on the entire set but in aggregate we can
			// determine offenders.
			assert!(matches!(
				get_outcome(&failures(12), |id| {
					if id < 5 {
						[11, 12]
					} else if id < 9 {
						[1, 11]
					} else {
						[1, 2]
					}
				}),
				Err(blamed) if blamed == BTreeSet::from_iter([1, 11])
			));

			// Unresponsive and dissenting nodes are reported.
			assert!(matches!(
				get_outcome(&reported_outcomes(b"tfffsfffbffft"), |_| []),
				Err(blamed) if blamed == BTreeSet::from_iter([1, 5, 9, 13])
			));
		});
	}
}
