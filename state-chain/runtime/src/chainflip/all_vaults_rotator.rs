//! Vault rotator to be used by the Validator pallet to control the rotation of multiple vaults

use core::marker::PhantomData;

use cf_primitives::EpochIndex;
use cf_traits::{AsyncResult, VaultRotationStatusOuter, VaultRotator};
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
	fn keygen(candidates: BTreeSet<Self::ValidatorId>, next_epoch_index: EpochIndex) {
		A::keygen(candidates.clone(), next_epoch_index);
		B::keygen(candidates.clone(), next_epoch_index);
		C::keygen(candidates, next_epoch_index);
	}

	/// Start all the key handovers for the vaults with the provided `candidates`.
	fn key_handover(
		sharing_participants: BTreeSet<Self::ValidatorId>,
		new_candidates: BTreeSet<Self::ValidatorId>,
		epoch_index: EpochIndex,
	) {
		A::key_handover(sharing_participants.clone(), new_candidates.clone(), epoch_index);
		B::key_handover(sharing_participants.clone(), new_candidates.clone(), epoch_index);
		C::key_handover(sharing_participants, new_candidates, epoch_index);
	}

	fn status() -> AsyncResult<VaultRotationStatusOuter<Self::ValidatorId>> {
		let async_results = [A::status(), B::status(), C::status()];

		// if any of the inner rotations are void, then the overall vault rotation result is void.
		if async_results.iter().any(|item| matches!(item, AsyncResult::Void)) {
			return AsyncResult::Void
		}

		// We must wait until all of these are ready before we do any action
		if async_results.iter().all(|item| matches!(item, AsyncResult::Ready(..))) {
			let statuses = async_results.into_iter().map(AsyncResult::unwrap).collect::<Vec<_>>();

			if statuses.iter().all(|x| matches!(x, VaultRotationStatusOuter::KeygenComplete)) {
				AsyncResult::Ready(VaultRotationStatusOuter::KeygenComplete)
			} else if statuses
				.iter()
				.all(|x| matches!(x, VaultRotationStatusOuter::KeyHandoverComplete))
			{
				AsyncResult::Ready(VaultRotationStatusOuter::KeyHandoverComplete)
			} else {
				// We currently treat an offence in one vault rotation as bad as in all rotations.
				// We may want to change it, but this is simplest for now.

				AsyncResult::Ready(VaultRotationStatusOuter::Failed(
					statuses
						.into_iter()
						.filter_map(|r| {
							if let VaultRotationStatusOuter::Failed(offenders) = r {
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

	fn reset_vault_rotation() {
		A::reset_vault_rotation();
		B::reset_vault_rotation();
		C::reset_vault_rotation();
	}

	fn activate_vaults() {
		A::activate_vaults();
		B::activate_vaults();
		C::activate_vaults();
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn set_status(outcome: AsyncResult<VaultRotationStatusOuter<Self::ValidatorId>>) {
		A::set_status(outcome.clone());
		B::set_status(outcome.clone());
		C::set_status(outcome);
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
				AsyncResult::Ready(VaultRotationStatusOuter::KeygenComplete)
			);
		});
	}

	#[test]
	fn status_key_handover_complete_when_all_complete() {
		frame_support::sp_io::TestExternalities::new_empty().execute_with(|| {
			MockVaultRotatorA::key_handover_success();
			MockVaultRotatorB::key_handover_success();
			MockVaultRotatorC::key_handover_success();

			assert_eq!(
				AllVaultRotator::<MockVaultRotatorA, MockVaultRotatorB, MockVaultRotatorC>::status(
				),
				AsyncResult::Ready(VaultRotationStatusOuter::KeyHandoverComplete)
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
				AsyncResult::Ready(VaultRotationStatusOuter::RotationComplete)
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
				AsyncResult::Ready(VaultRotationStatusOuter::Failed(BTreeSet::default()))
			);
		});
	}

	#[test]
	fn all_ready_one_failed_is_failed() {
		const OFFENDERS: [u64; 4] = [1u64, 2, 3, 4];
		// Keygen
		frame_support::sp_io::TestExternalities::new_empty().execute_with(|| {
			MockVaultRotatorA::failed(OFFENDERS);
			MockVaultRotatorB::keygen_success();
			MockVaultRotatorC::keygen_success();

			assert_eq!(
				AllVaultRotator::<MockVaultRotatorA, MockVaultRotatorB, MockVaultRotatorC>::status(
				),
				AsyncResult::Ready(VaultRotationStatusOuter::Failed(BTreeSet::from(OFFENDERS)))
			);
		});

		// Key handover
		frame_support::sp_io::TestExternalities::new_empty().execute_with(|| {
			MockVaultRotatorA::failed(OFFENDERS);
			MockVaultRotatorB::key_handover_success();
			MockVaultRotatorC::key_handover_success();

			assert_eq!(
				AllVaultRotator::<MockVaultRotatorA, MockVaultRotatorB, MockVaultRotatorC>::status(
				),
				AsyncResult::Ready(VaultRotationStatusOuter::Failed(BTreeSet::from(OFFENDERS)))
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
				AsyncResult::Ready(VaultRotationStatusOuter::Failed(BTreeSet::from([
					1, 2, 3, 4, 5, 6
				])))
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
