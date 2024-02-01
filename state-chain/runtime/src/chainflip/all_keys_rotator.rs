//! Key rotator to be used by the Validator pallet to control the rotation of multiple keys

use core::marker::PhantomData;

use cf_primitives::EpochIndex;
use cf_traits::{AsyncResult, KeyRotationStatusOuter, KeyRotator};
use sp_std::{collections::btree_set::BTreeSet, vec::Vec};

pub struct AllKeyRotator<A, B, C> {
	_phantom: PhantomData<(A, B, C)>,
}

impl<A, B, C> KeyRotator for AllKeyRotator<A, B, C>
where
	A: KeyRotator,
	B: KeyRotator<ValidatorId = A::ValidatorId>,
	C: KeyRotator<ValidatorId = A::ValidatorId>,
{
	type ValidatorId = A::ValidatorId;

	/// Start all key rotations with the provided `candidates`.
	fn keygen(candidates: BTreeSet<Self::ValidatorId>, next_epoch_index: EpochIndex) {
		A::keygen(candidates.clone(), next_epoch_index);
		B::keygen(candidates.clone(), next_epoch_index);
		C::keygen(candidates, next_epoch_index);
	}

	/// Start all the key handovers for the keys with the provided `candidates`.
	fn key_handover(
		sharing_participants: BTreeSet<Self::ValidatorId>,
		new_candidates: BTreeSet<Self::ValidatorId>,
		epoch_index: EpochIndex,
	) {
		A::key_handover(sharing_participants.clone(), new_candidates.clone(), epoch_index);
		B::key_handover(sharing_participants.clone(), new_candidates.clone(), epoch_index);
		C::key_handover(sharing_participants, new_candidates, epoch_index);
	}

	fn status() -> AsyncResult<KeyRotationStatusOuter<Self::ValidatorId>> {
		let async_results = [A::status(), B::status(), C::status()];

		// if any of the inner rotations are void, then the overall key rotation result is void.
		if async_results.iter().any(|item| matches!(item, AsyncResult::Void)) {
			return AsyncResult::Void
		}

		// We must wait until all of these are ready before we do any action
		if async_results.iter().all(|item| matches!(item, AsyncResult::Ready(..))) {
			let statuses = async_results.into_iter().map(AsyncResult::unwrap).collect::<Vec<_>>();

			if statuses.iter().all(|x| matches!(x, KeyRotationStatusOuter::KeygenComplete)) {
				AsyncResult::Ready(KeyRotationStatusOuter::KeygenComplete)
			} else if statuses
				.iter()
				.all(|x| matches!(x, KeyRotationStatusOuter::KeyHandoverComplete))
			{
				AsyncResult::Ready(KeyRotationStatusOuter::KeyHandoverComplete)
			} else if statuses.iter().all(|x| matches!(x, KeyRotationStatusOuter::RotationComplete))
			{
				AsyncResult::Ready(KeyRotationStatusOuter::RotationComplete)
			} else {
				// We currently treat an offence in one key rotation as bad as in all rotations.
				// We may want to change it, but this is simplest for now.

				AsyncResult::Ready(KeyRotationStatusOuter::Failed(
					statuses
						.into_iter()
						.filter_map(|r| {
							if let KeyRotationStatusOuter::Failed(offenders) = r {
								Some(offenders)
							} else {
								None
							}
						})
						.flatten()
						.collect(),
				))
			}
		} else {
			AsyncResult::Pending
		}
	}

	fn reset_key_rotation() {
		A::reset_key_rotation();
		B::reset_key_rotation();
		C::reset_key_rotation();
	}

	fn activate_vaults() {
		A::activate_vaults();
		B::activate_vaults();
		C::activate_vaults();
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn set_status(outcome: AsyncResult<KeyRotationStatusOuter<Self::ValidatorId>>) {
		A::set_status(outcome.clone());
		B::set_status(outcome.clone());
		C::set_status(outcome);
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
				AllKeyRotator::<MockKeyRotatorA, MockKeyRotatorB, MockKeyRotatorC>::status(),
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
				AllKeyRotator::<MockKeyRotatorA, MockKeyRotatorB, MockKeyRotatorC>::status(),
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
				AllKeyRotator::<MockKeyRotatorA, MockKeyRotatorB, MockKeyRotatorC>::status(),
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
				AllKeyRotator::<MockKeyRotatorA, MockKeyRotatorB, MockKeyRotatorC>::status(),
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
				AllKeyRotator::<MockKeyRotatorA, MockKeyRotatorB, MockKeyRotatorC>::status(),
				AsyncResult::Ready(KeyRotationStatusOuter::Failed(BTreeSet::from(OFFENDERS)))
			);
		});

		// Key handover
		sp_io::TestExternalities::new_empty().execute_with(|| {
			MockKeyRotatorA::failed(OFFENDERS);
			MockKeyRotatorB::key_handover_success();
			MockKeyRotatorC::key_handover_success();

			assert_eq!(
				AllKeyRotator::<MockKeyRotatorA, MockKeyRotatorB, MockKeyRotatorC>::status(),
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
				AllKeyRotator::<MockKeyRotatorA, MockKeyRotatorB, MockKeyRotatorC>::status(),
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
				AllKeyRotator::<MockKeyRotatorA, MockKeyRotatorB, MockKeyRotatorC>::status(),
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
				AllKeyRotator::<MockKeyRotatorA, MockKeyRotatorB, MockKeyRotatorC>::status(),
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
				AllKeyRotator::<MockKeyRotatorA, MockKeyRotatorB, MockKeyRotatorC>::status(),
				AsyncResult::Pending
			);
		});
	}
}
