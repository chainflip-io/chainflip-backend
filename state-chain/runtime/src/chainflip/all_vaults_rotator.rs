//! Vault rotator to be used by the Validator pallet to control the rotation of multiple vaults

use core::marker::PhantomData;

use cf_traits::{AsyncResult, VaultRotator, VaultStatus};
use sp_std::collections::btree_set::BTreeSet;

pub struct AllVaultRotator<A> {
	_phantom: PhantomData<A>,
}

impl<A> VaultRotator for AllVaultRotator<A>
where
	A: VaultRotator,
{
	type ValidatorId = A::ValidatorId;

	/// Start all vault rotations with the provided `candidates`.
	fn start(candidates: BTreeSet<Self::ValidatorId>) {
		A::start(candidates)
	}

	fn status() -> AsyncResult<VaultStatus<Self::ValidatorId>> {
		let a_async_result = A::status();

		// if any of the inner rotations are void, then the overall vault rotation result is void.
		if matches!(a_async_result, AsyncResult::Void) {
			return AsyncResult::Void
		}

		let all_ready = a_async_result.is_ready();

		// We must wait until all of these are ready before we do any action
		if all_ready {
			let all_results = [a_async_result.unwrap()];
			if all_results.iter().all(|x| matches!(x, VaultStatus::KeygenComplete)) {
				AsyncResult::Ready(VaultStatus::KeygenComplete)
			} else if all_results.iter().all(|x| matches!(x, VaultStatus::RotationComplete)) {
				AsyncResult::Ready(VaultStatus::RotationComplete)
			} else {
				// We currently treat an offence in one vault rotation as bad as in all rotations.
				// We may want to change it, but this is simplest for now.

				AsyncResult::Ready(VaultStatus::Failed(
					all_results
						.into_iter()
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
		A::activate()
	}
}
