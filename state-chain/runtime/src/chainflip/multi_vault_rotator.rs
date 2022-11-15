//! Vault rotator to be used by the Validator pallet to control the rotation of multiple vaults

use core::marker::PhantomData;

use cf_traits::{AsyncResult, MultiVaultRotator, VaultRotator};
use sp_std::collections::btree_set::BTreeSet;

pub struct AllVaultRotator<A, B> {
	_phantom: PhantomData<(A, B)>,
}

// Do some type bounds here so the return
impl<A, B> MultiVaultRotator for AllVaultRotator<A, B>
where
	A: VaultRotator,
	B: VaultRotator<ValidatorId = A::ValidatorId>,
{
	type ValidatorId = A::ValidatorId;

	// Only if all keygen verifications are *successful* are we ready to rotate
	fn ready_to_commit_new_keys() -> AsyncResult<Result<(), BTreeSet<Self::ValidatorId>>> {
		let a_async_result = A::get_vault_rotation_outcome();
		let b_async_result = B::get_vault_rotation_outcome();

		// We must wait until all of these are ready before we do any action
		if a_async_result.is_ready() && b_async_result.is_ready() {
			let all_results = [a_async_result.unwrap(), b_async_result.unwrap()];

			if all_results.iter().all(Result::is_ok) {
				AsyncResult::Ready(Ok(()))
			} else {
				// We currently treat an offence in one vault rotation as bad as in all rotations.
				// We may want to change it, but this is simplest for now.

				AsyncResult::Ready(Err(all_results
					.into_iter()
					.filter_map(|r| if let Err(offenders) = r { Some(offenders) } else { None })
					.fold(BTreeSet::default(), |acc, x| {
						acc.union(&x).into_iter().cloned().collect::<BTreeSet<_>>()
					})))
			}
		} else {
			AsyncResult::Pending
		}
	}
}
