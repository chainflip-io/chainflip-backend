#![cfg(feature = "runtime-benchmarks")]
use super::*;

use crate::{Config, Pallet};
use cf_primitives::AccountRole;
use cf_traits::EpochInfo;
use core::iter;
use frame_benchmarking::v2::*;

use crate::electoral_system::ElectoralSystem;
use cf_traits::AccountRoleRegistry;
use frame_system::RawOrigin;
use sp_std::collections::btree_map::BTreeMap;

// Keep this to avoid CI warnings about no benchmarks in the crate.
#[instance_benchmarks]
mod benchmarks {
	use super::*;

	/// Provides a valid vote in a valid  election and ensures that the system is in the right state
	/// for benchmarking other extrinsics.
	fn vote_in_epoch<T: crate::pallet::Config<I>, I: 'static>(
		account_id: T::AccountId,
		epoch: u32,
	) {
		let validator_id: T::ValidatorId = account_id.clone().into();
		T::EpochInfo::add_authority_info_for_epoch(epoch, vec![validator_id.clone()]);

		let block = BlockNumberFor::<T>::from(epoch);
		let zero_sync_barrier = VoteSynchronisationBarrier::from_u32(0);

		Pallet::<T, I>::on_finalize(block);

		let _ = Pallet::<T, I>::ignore_my_votes(
			RawOrigin::Signed(account_id.clone()).into(),
			zero_sync_barrier.clone(),
		)
		.unwrap();

		let _ = Pallet::<T, I>::stop_ignoring_my_votes(
			RawOrigin::Signed(account_id.clone()).into(),
			zero_sync_barrier.clone(),
		)
		.unwrap();

		let (elections, sync_barrier) =
			Pallet::<T, I>::validator_election_data(&validator_id).unwrap();

		let next_election = elections.into_iter().next().unwrap();

		let _ = Pallet::<T, I>::vote(
			RawOrigin::Signed(account_id.clone()).into(),
			BoundedBTreeMap::try_from(
				iter::repeat((next_election.0, T::ElectoralSystem::benchmark_authority_vote()))
					.take(epoch as usize)
					.collect::<BTreeMap<_, _>>(),
			)
			.unwrap(),
			sync_barrier.clone().unwrap(),
		)
		.unwrap();

		ElectionConsensusHistoryUpToDate::<T, I>::insert(
			UniqueMonotonicIdentifier::from_u64((epoch as u64) - 1),
			epoch - 1,
		);
	}

	#[benchmark]
	fn ignore_my_votes() {
		let caller =
			T::AccountRoleRegistry::whitelisted_caller_with_role(AccountRole::Validator).unwrap();
		let validator_id: T::ValidatorId = caller.clone().into();

		vote_in_epoch::<T, I>(caller.clone(), 1);

		let zero_sync_barrier = VoteSynchronisationBarrier::from_u32(0);

		assert!(SharedData::<T, I>::iter().count() == 1, "Shared data not present in storage!");

		#[extrinsic_call]
		ignore_my_votes(RawOrigin::Signed(caller), zero_sync_barrier);

		assert!(
			ElectionConsensusHistoryUpToDate::<T, I>::iter().count() == 0,
			"ElectionConsensusHistoryUpToDate not removed from storage!"
		);

		assert!(
			AuthorityVoteSynchronisationBarriers::<T, I>::contains_key(validator_id),
			"AuthorityVoteSynchronisationBarriers not present in storage!"
		);
	}

	#[benchmark]
	fn stop_ignoring_my_votes() {
		let caller =
			T::AccountRoleRegistry::whitelisted_caller_with_role(AccountRole::Validator).unwrap();
		let validator_id: T::ValidatorId = caller.clone().into();

		vote_in_epoch::<T, I>(caller.clone(), 1);

		let zero_sync_barrier = VoteSynchronisationBarrier::from_u32(0);

		assert!(SharedData::<T, I>::iter().count() == 1, "Shared data not present in storage!");

		ContributingAuthorities::<T, I>::remove(&validator_id);

		#[extrinsic_call]
		stop_ignoring_my_votes(RawOrigin::Signed(caller), zero_sync_barrier);

		assert!(
			ElectionConsensusHistoryUpToDate::<T, I>::iter().count() == 0,
			"ElectionConsensusHistoryUpToDate not removed from storage!"
		);
	}

	// #[cfg(test)]
	// use crate::mock::*;

	// #[test]
	// fn benchmark_works() {
	// 	new_test_ext().execute_with(|| {
	// 		_ignore_my_votes::<Test, Instance1>(true);
	// 	});
	// 	new_test_ext().execute_with(|| {
	// 		_stop_ignoring_my_votes::<Test, Instance1>(true);
	// 	});
	// }
}
