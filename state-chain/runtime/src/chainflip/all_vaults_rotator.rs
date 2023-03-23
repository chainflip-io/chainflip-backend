//! Vault rotator to be used by the Validator pallet to control the rotation of multiple vaults

use core::marker::PhantomData;

use cf_primitives::EpochIndex;
use cf_traits::{AsyncResult, VaultRotator, VaultStatus};
use sp_std::{collections::btree_set::BTreeSet, vec::Vec};

pub struct AllVaultRotator<A, B, C> {
	_phantom: PhantomData<(A, B, C)>,
}

impl<A, B, C> VaultRotator for AllVaultRotator<A, B, C>
where
	A: VaultRotator,
	B: VaultRotator<ValidatorId = A::ValidatorId>,
	C: VaultRotator<ValidatorId = A::ValidatorId>,
{
	type ValidatorId = A::ValidatorId;

	/// Start all vault rotations with the provided `candidates`.
	fn keygen(candidates: BTreeSet<Self::ValidatorId>, epoch_index: EpochIndex) {
		A::keygen(candidates.clone(), epoch_index);
		B::keygen(candidates.clone(), epoch_index);
		C::keygen(candidates, epoch_index);
	}

	fn status() -> AsyncResult<VaultStatus<Self::ValidatorId>> {
		let async_results = [A::status(), B::status(), C::status()];

		// if any of the inner rotations are void, then the overall vault rotation result is void.
		if async_results.iter().any(|item| matches!(item, AsyncResult::Void)) {
			return AsyncResult::Void
		}

		// We must wait until all of these are ready before we do any action
		if async_results.iter().all(|item| matches!(item, AsyncResult::Ready(..))) {
			let statuses = async_results.into_iter().map(AsyncResult::unwrap).collect::<Vec<_>>();

			if statuses.iter().all(|x| matches!(x, VaultStatus::KeygenComplete)) {
				AsyncResult::Ready(VaultStatus::KeygenComplete)
			} else if statuses.iter().all(|x| matches!(x, VaultStatus::RotationComplete)) {
				AsyncResult::Ready(VaultStatus::RotationComplete)
			} else {
				// We currently treat an offence in one vault rotation as bad as in all rotations.
				// We may want to change it, but this is simplest for now.

				AsyncResult::Ready(VaultStatus::Failed(
					statuses
						.iter()
						.filter_map(|r| {
							if let VaultStatus::Failed(offenders) = r {
								Some(offenders)
							} else {
								None
							}
						})
						.fold(BTreeSet::default(), |acc, x| {
							acc.union(x).into_iter().cloned().collect::<BTreeSet<_>>()
						}),
				))
			}
		} else {
			AsyncResult::Pending
		}
	}

	fn activate() {
		A::activate();
		B::activate();
		C::activate();
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn set_status(outcome: AsyncResult<VaultStatus<Self::ValidatorId>>) {
		A::set_status(outcome.clone());
		B::set_status(outcome.clone());
		B::set_status(outcome);
	}
}

#[cfg(test)]
mod tests {
	use cf_traits::{
		mocks::vault_rotator::{MockVaultRotatorA, MockVaultRotatorB, MockVaultRotatorC},
		AsyncResult, VaultRotator,
	};

	use super::*;

	#[test]
	fn status_keygen_complete_when_all_complete() {
		frame_support::sp_io::TestExternalities::new_empty().execute_with(|| {
			MockVaultRotatorA::keygen_success();
			MockVaultRotatorB::keygen_success();
			MockVaultRotatorC::keygen_success();

			assert_eq!(
				AllVaultRotator::<MockVaultRotatorA, MockVaultRotatorB, MockVaultRotatorC>::status(
				),
				AsyncResult::Ready(VaultStatus::KeygenComplete)
			);
		});
	}

	#[test]
	fn status_rotation_complete_when_all_complete() {
		frame_support::sp_io::TestExternalities::new_empty().execute_with(|| {
			MockVaultRotatorA::keys_activated();
			MockVaultRotatorB::keys_activated();
			MockVaultRotatorC::keys_activated();

			assert_eq!(
				AllVaultRotator::<MockVaultRotatorA, MockVaultRotatorB, MockVaultRotatorC>::status(
				),
				AsyncResult::Ready(VaultStatus::RotationComplete)
			);
		});
	}

	// If one vault is at keygens complete and the other is at rotation complete, this is considered
	// failure. This should not be possible, since *all* vaults should move out of KeygenComplete at
	// the same time - since the validator pallet should do this.
	#[test]
	fn all_ready_but_diff_statuses_is_failure() {
		frame_support::sp_io::TestExternalities::new_empty().execute_with(|| {
			MockVaultRotatorA::keys_activated();
			MockVaultRotatorB::keygen_success();
			MockVaultRotatorC::keygen_success();

			assert_eq!(
				AllVaultRotator::<MockVaultRotatorA, MockVaultRotatorB, MockVaultRotatorC>::status(
				),
				AsyncResult::Ready(VaultStatus::Failed(BTreeSet::default()))
			);
		});
	}

	#[test]
	fn all_ready_one_failed_is_failed() {
		frame_support::sp_io::TestExternalities::new_empty().execute_with(|| {
			MockVaultRotatorA::failed([1, 2, 3, 4]);
			MockVaultRotatorB::keygen_success();
			MockVaultRotatorC::keygen_success();

			assert_eq!(
				AllVaultRotator::<MockVaultRotatorA, MockVaultRotatorB, MockVaultRotatorC>::status(
				),
				AsyncResult::Ready(VaultStatus::Failed(BTreeSet::from([1, 2, 3, 4])))
			);
		});
	}

	#[test]
	fn failed_statuses_combine_offenders() {
		frame_support::sp_io::TestExternalities::new_empty().execute_with(|| {
			MockVaultRotatorA::failed([1, 2, 3, 4]);
			MockVaultRotatorB::failed([2, 4, 5]);
			MockVaultRotatorC::failed([4, 5, 6]);

			assert_eq!(
				AllVaultRotator::<MockVaultRotatorA, MockVaultRotatorB, MockVaultRotatorC>::status(
				),
				AsyncResult::Ready(VaultStatus::Failed(BTreeSet::from([1, 2, 3, 4, 5, 6])))
			);
		});
	}

	#[test]
	fn all_pending_is_pending() {
		frame_support::sp_io::TestExternalities::new_empty().execute_with(|| {
			MockVaultRotatorA::pending();
			MockVaultRotatorB::pending();
			MockVaultRotatorC::pending();

			assert_eq!(
				AllVaultRotator::<MockVaultRotatorA, MockVaultRotatorB, MockVaultRotatorC>::status(
				),
				AsyncResult::Pending
			);
		});
	}

	#[test]
	fn one_pending_is_pending() {
		frame_support::sp_io::TestExternalities::new_empty().execute_with(|| {
			MockVaultRotatorA::keygen_success();
			MockVaultRotatorB::pending();
			MockVaultRotatorC::keygen_success();

			assert_eq!(
				AllVaultRotator::<MockVaultRotatorA, MockVaultRotatorB, MockVaultRotatorC>::status(
				),
				AsyncResult::Pending
			);
		});
	}

	// We want to wait for all results to be ready before failing. This is in case the other results
	// we are waiting on also fail, in which case we want to punish the offenders for those failures
	// too, before we continue.
	#[test]
	fn pending_if_one_pending_even_when_failure() {
		frame_support::sp_io::TestExternalities::new_empty().execute_with(|| {
			MockVaultRotatorA::failed([1, 2, 3]);
			MockVaultRotatorB::pending();
			MockVaultRotatorC::failed([4, 5, 6]);

			assert_eq!(
				AllVaultRotator::<MockVaultRotatorA, MockVaultRotatorB, MockVaultRotatorC>::status(
				),
				AsyncResult::Pending
			);
		});
	}
}
