#![cfg(feature = "runtime-benchmarks")]
use super::*;

use crate::{Config, Pallet};
use cf_primitives::AccountRole;
use cf_traits::{Chainflip, EpochInfo};
use frame_benchmarking::v2::*;

use crate::electoral_system::AuthorityVoteOf;
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

		// let vote = AuthorityVoteOf::<<T as Config<I>>::ElectoralSystem>::Vote(
		// 	BenchmarkValue::benchmark_value(),
		// );
		T::EpochInfo::add_authority_info_for_epoch(1, vec![validator_id.clone()]);

		Pallet::<T, I>::on_finalize(frame_system::Pallet::<T>::block_number());

		// 1. Run on_finalize

		// Pallet::<T, I>::inner_provide_shared_data(vote);

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

		assert!(
			ElectionConsensusHistoryUpToDate::<T, I>::iter().count() == 0,
			"Storage didn't got removed!"
		);
	}

	#[benchmark]
	fn stop_ignoring_my_votes(n: Linear<1, 10>) {
		let caller =
			T::AccountRoleRegistry::whitelisted_caller_with_role(AccountRole::Validator).unwrap();
		let validator_id: T::ValidatorId = caller.clone().into();
		T::EpochInfo::add_authority_info_for_epoch(1, vec![validator_id.clone()]);
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

		assert!(
			ElectionConsensusHistoryUpToDate::<T, I>::iter().count() == 0,
			"Storage didn't got removed!"
		);
	}

	#[cfg(test)]
	use crate::mock::*;

	#[test]
	fn benchmark_works() {
		new_test_ext().execute_with(|| {
			_ignore_my_votes::<Test, Instance1>(50, true);
		});
		new_test_ext().execute_with(|| {
			_stop_ignoring_my_votes::<Test, Instance1>(50, true);
		});
	}
}
