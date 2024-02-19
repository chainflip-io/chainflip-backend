use cf_primitives::EpochIndex;
use cf_traits::{AsyncResult, KeyRotationStatusOuter, KeyRotator};
use sp_std::collections::btree_set::BTreeSet;

#[macro_export]
macro_rules! cons_key_rotator {
    ($last: ty) => {
        $last
    };

    ($head: ty, $($tail:ty),+) => {
        $crate::chainflip::cons_key_rotator::ConsKeyRotator<$head, cons_key_rotator!($($tail),+)>
    }
}

pub struct ConsKeyRotator<H, T>(H, T);

impl<H, T> KeyRotator for ConsKeyRotator<H, T>
where
	H: KeyRotator,
	T: KeyRotator<ValidatorId = H::ValidatorId>,
{
	type ValidatorId = H::ValidatorId;

	fn keygen(candidates: BTreeSet<Self::ValidatorId>, new_epoch_index: EpochIndex) {
		H::keygen(candidates.clone(), new_epoch_index);
		T::keygen(candidates, new_epoch_index);
	}

	fn key_handover(
		// Authorities of the last epoch selected to share their key in the key handover
		sharing_participants: BTreeSet<Self::ValidatorId>,
		// These are any authorities for the new epoch who are not sharing participants
		receiving_participants: BTreeSet<Self::ValidatorId>,
		epoch_index: EpochIndex,
	) {
		H::key_handover(sharing_participants.clone(), receiving_participants.clone(), epoch_index);
		T::key_handover(sharing_participants, receiving_participants, epoch_index);
	}

	fn status() -> AsyncResult<KeyRotationStatusOuter<Self::ValidatorId>> {
		use KeyRotationStatusOuter::*;
		match (H::status(), T::status()) {
			(AsyncResult::Void, _) => AsyncResult::Void,
			(_, AsyncResult::Void) => AsyncResult::Void,

			(AsyncResult::Ready(head_status), AsyncResult::Ready(tail_status)) =>
				AsyncResult::Ready(match (head_status, tail_status) {
					(KeygenComplete, KeygenComplete) => KeygenComplete,
					(KeyHandoverComplete, KeyHandoverComplete) => KeyHandoverComplete,
					(RotationComplete, RotationComplete) => RotationComplete,
					(head_maybe_failed, tail_maybe_failed) => Failed(
						extract_offenders(head_maybe_failed)
							.chain(extract_offenders(tail_maybe_failed))
							.collect(),
					),
				}),

			_ => AsyncResult::Pending,
		}
	}

	fn reset_key_rotation() {
		H::reset_key_rotation();
		T::reset_key_rotation();
	}

	fn activate_keys() {
		H::activate_keys();
		T::activate_keys();
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn set_status(outcome: AsyncResult<KeyRotationStatusOuter<Self::ValidatorId>>) {
		H::set_status(outcome.clone());
		T::set_status(outcome);
	}
}

fn extract_offenders<ValidatorId>(
	status: KeyRotationStatusOuter<ValidatorId>,
) -> impl Iterator<Item = ValidatorId> {
	if let KeyRotationStatusOuter::Failed(offenders) = status {
		Some(offenders.into_iter()).into_iter().flatten()
	} else {
		None.into_iter().flatten()
	}
}

#[cfg(test)]
mod tests {
	use cf_traits::{
		mocks::key_rotator::{MockKeyRotatorA, MockKeyRotatorB, MockKeyRotatorC},
		AsyncResult, KeyRotator,
	};

	use super::*;

	#[test]
	fn status_keygen_complete_when_all_complete() {
		sp_io::TestExternalities::new_empty().execute_with(|| {
			MockKeyRotatorA::keygen_success();
			MockKeyRotatorB::keygen_success();
			MockKeyRotatorC::keygen_success();

			assert_eq!(
				<cons_key_rotator!(MockKeyRotatorA, MockKeyRotatorB, MockKeyRotatorC)>::status(),
				AsyncResult::Ready(KeyRotationStatusOuter::KeygenComplete)
			);
		});
	}

	#[test]
	fn status_key_handover_complete_when_all_complete() {
		sp_io::TestExternalities::new_empty().execute_with(|| {
			MockKeyRotatorA::key_handover_success();
			MockKeyRotatorB::key_handover_success();
			MockKeyRotatorC::key_handover_success();

			assert_eq!(
				<cons_key_rotator!(MockKeyRotatorA, MockKeyRotatorB, MockKeyRotatorC)>::status(),
				AsyncResult::Ready(KeyRotationStatusOuter::KeyHandoverComplete)
			);
		});
	}

	#[test]
	fn status_rotation_complete_when_all_complete() {
		sp_io::TestExternalities::new_empty().execute_with(|| {
			MockKeyRotatorA::keys_activated();
			MockKeyRotatorB::keys_activated();
			MockKeyRotatorC::keys_activated();

			assert_eq!(
				<cons_key_rotator!(MockKeyRotatorA, MockKeyRotatorB, MockKeyRotatorC)>::status(),
				AsyncResult::Ready(KeyRotationStatusOuter::RotationComplete)
			);
		});
	}

	// If one vault is at keygens complete and the other is at rotation complete, this is considered
	// failure. This should not be possible, since *all* vaults should move out of KeygenComplete at
	// the same time - since the validator pallet should do this.
	#[test]
	fn all_ready_but_diff_statuses_is_failure() {
		sp_io::TestExternalities::new_empty().execute_with(|| {
			MockKeyRotatorA::keys_activated();
			MockKeyRotatorB::keygen_success();
			MockKeyRotatorC::keygen_success();

			assert_eq!(
				<cons_key_rotator!(MockKeyRotatorA, MockKeyRotatorB, MockKeyRotatorC)>::status(),
				AsyncResult::Ready(KeyRotationStatusOuter::Failed(BTreeSet::default()))
			);
		});
	}

	#[test]
	fn all_ready_one_failed_is_failed() {
		const OFFENDERS: [u64; 4] = [1u64, 2, 3, 4];
		// Keygen
		sp_io::TestExternalities::new_empty().execute_with(|| {
			MockKeyRotatorA::failed(OFFENDERS);
			MockKeyRotatorB::keygen_success();
			MockKeyRotatorC::keygen_success();

			assert_eq!(
				<cons_key_rotator!(MockKeyRotatorA, MockKeyRotatorB, MockKeyRotatorC)>::status(),
				AsyncResult::Ready(KeyRotationStatusOuter::Failed(BTreeSet::from(OFFENDERS)))
			);
		});

		// Key handover
		sp_io::TestExternalities::new_empty().execute_with(|| {
			MockKeyRotatorA::failed(OFFENDERS);
			MockKeyRotatorB::key_handover_success();
			MockKeyRotatorC::key_handover_success();

			assert_eq!(
				<cons_key_rotator!(MockKeyRotatorA, MockKeyRotatorB, MockKeyRotatorC)>::status(),
				AsyncResult::Ready(KeyRotationStatusOuter::Failed(BTreeSet::from(OFFENDERS)))
			);
		});
	}

	#[test]
	fn failed_statuses_combine_offenders() {
		sp_io::TestExternalities::new_empty().execute_with(|| {
			MockKeyRotatorA::failed([1, 2, 3, 4]);
			MockKeyRotatorB::failed([2, 4, 5]);
			MockKeyRotatorC::failed([4, 5, 6]);

			assert_eq!(
				<cons_key_rotator!(MockKeyRotatorA, MockKeyRotatorB, MockKeyRotatorC)>::status(),
				AsyncResult::Ready(KeyRotationStatusOuter::Failed(BTreeSet::from([
					1, 2, 3, 4, 5, 6
				])))
			);
		});
	}

	#[test]
	fn all_pending_is_pending() {
		sp_io::TestExternalities::new_empty().execute_with(|| {
			MockKeyRotatorA::pending();
			MockKeyRotatorB::pending();
			MockKeyRotatorC::pending();

			assert_eq!(
				<cons_key_rotator!(MockKeyRotatorA, MockKeyRotatorB, MockKeyRotatorC)>::status(),
				AsyncResult::Pending
			);
		});
	}

	#[test]
	fn one_pending_is_pending() {
		sp_io::TestExternalities::new_empty().execute_with(|| {
			MockKeyRotatorA::keygen_success();
			MockKeyRotatorB::pending();
			MockKeyRotatorC::keygen_success();

			assert_eq!(
				<cons_key_rotator!(MockKeyRotatorA, MockKeyRotatorB, MockKeyRotatorC)>::status(),
				AsyncResult::Pending
			);
		});
	}

	// We want to wait for all results to be ready before failing. This is in case the other results
	// we are waiting on also fail, in which case we want to punish the offenders for those failures
	// too, before we continue.
	#[test]
	fn pending_if_one_pending_even_when_failure() {
		sp_io::TestExternalities::new_empty().execute_with(|| {
			MockKeyRotatorA::failed([1, 2, 3]);
			MockKeyRotatorB::pending();
			MockKeyRotatorC::failed([4, 5, 6]);

			assert_eq!(
				<cons_key_rotator!(MockKeyRotatorA, MockKeyRotatorB, MockKeyRotatorC)>::status(),
				AsyncResult::Pending
			);
		});
	}
}
