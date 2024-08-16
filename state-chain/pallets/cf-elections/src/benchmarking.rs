#![cfg(feature = "runtime-benchmarks")]
use super::*;

use crate::{Config, Pallet};
use cf_primitives::AccountRole;
use cf_traits::{Chainflip, EpochInfo};
use frame_benchmarking::v2::*;

use cf_traits::AccountRoleRegistry;
use frame_system::RawOrigin;

// Keep this to avoid CI warnings about no benchmarks in the crate.
#[instance_benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn ignore_my_votes(n: Linear<1, 10>) {
		let caller =
			T::AccountRoleRegistry::whitelisted_caller_with_role(AccountRole::Validator).unwrap();
		let validator_id: T::ValidatorId = caller.clone().into();

		for i in 0..n {
			ElectionConsensusHistoryUpToDate::<T, I>::insert(
				UniqueMonotonicIdentifier::from_u64(i as u64),
				i,
			);
		}

		ContributingAuthorities::<T, I>::insert(validator_id, ());

		let zero_sync_barrier = VoteSynchronisationBarrier::from_u32(0);

		#[extrinsic_call]
		ignore_my_votes(RawOrigin::Signed(caller), zero_sync_barrier);
	}

	#[benchmark]
	fn stop_ignoring_my_votes(n: Linear<1, 10>) {
		let caller =
			T::AccountRoleRegistry::whitelisted_caller_with_role(AccountRole::Validator).unwrap();
		let validator_id: T::ValidatorId = caller.clone().into();
		let zero_sync_barrier = VoteSynchronisationBarrier::from_u32(0);

		AuthorityVoteSynchronisationBarriers::<T, I>::insert(validator_id, zero_sync_barrier);

		for i in 0..n {
			ElectionConsensusHistoryUpToDate::<T, I>::insert(
				UniqueMonotonicIdentifier::from_u64(i as u64),
				i,
			);
		}

		#[extrinsic_call]
		stop_ignoring_my_votes(
			RawOrigin::Signed(caller),
			VoteSynchronisationBarrier::from_u32(0u32),
		);
	}
}
