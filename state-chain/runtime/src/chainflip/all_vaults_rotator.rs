//! Vault rotator to be used by the Validator pallet to control the rotation of multiple vaults

use core::marker::PhantomData;

use cf_traits::{AsyncResult, VaultRotator, VaultStatus};
use sp_std::collections::btree_set::BTreeSet;

pub struct AllVaultRotator<A, B> {
	_phantom: PhantomData<(A, B)>,
}

impl<A, B> VaultRotator for AllVaultRotator<A, B>
where
	A: VaultRotator,
	B: VaultRotator<ValidatorId = A::ValidatorId>,
{
	type ValidatorId = A::ValidatorId;

	/// Start all vault rotations with the provided `candidates`.
	fn start(candidates: BTreeSet<Self::ValidatorId>) {
		A::start(candidates.clone());
		B::start(candidates);
	}

	fn status() -> AsyncResult<VaultStatus<Self::ValidatorId>> {
		let async_results = [A::status(), B::status()];

		// if any of the inner rotations are void, then the overall vault rotation result is void.
		if async_results.iter().any(|item| matches!(item, AsyncResult::Void)) {
			return AsyncResult::Void
		}

		// We must wait until all of these are ready before we do any action
		if async_results.iter().all(|item| matches!(item, AsyncResult::Ready(..))) {
			let mut statuses = async_results.into_iter().map(AsyncResult::unwrap);

			if statuses.all(|x| matches!(x, VaultStatus::KeygenComplete)) {
				AsyncResult::Ready(VaultStatus::KeygenComplete)
			} else if statuses.all(|x| matches!(x, VaultStatus::RotationComplete)) {
				AsyncResult::Ready(VaultStatus::RotationComplete)
			} else {
				// We currently treat an offence in one vault rotation as bad as in all rotations.
				// We may want to change it, but this is simplest for now.

				AsyncResult::Ready(VaultStatus::Failed(
					statuses
						.filter_map(|r| {
							if let VaultStatus::Failed(offenders) = r {
								Some(offenders)
							} else {
								None
							}
						})
						.fold(BTreeSet::default(), |acc, x| {
							acc.union(&x).into_iter().cloned().collect::<BTreeSet<_>>()
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
	}
}
