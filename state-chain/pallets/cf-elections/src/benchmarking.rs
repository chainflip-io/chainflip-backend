#![cfg(feature = "runtime-benchmarks")]
use super::*;

use core::iter;

use crate::{electoral_system::ElectoralSystem, Config, Pallet};
use cf_primitives::AccountRole;
use cf_traits::{AccountRoleRegistry, EpochInfo};

use frame_benchmarking::v2::*;
use frame_support::storage::bounded_btree_map::BoundedBTreeMap;
use frame_system::RawOrigin;
use sp_std::collections::btree_map::BTreeMap;

use crate::Call;

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
		let umi = epoch - 1;
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
			UniqueMonotonicIdentifier::from_u64(umi as u64),
			epoch,
		);
	}

	#[benchmark]
	fn vote(n: Linear<1, 10>) {
		let caller =
			T::AccountRoleRegistry::whitelisted_caller_with_role(AccountRole::Validator).unwrap();
		let validator_id: T::ValidatorId = caller.clone().into();

		T::EpochInfo::add_authority_info_for_epoch(1, vec![validator_id.clone()]);

		// kick off an election
		Pallet::<T, I>::on_finalize(frame_system::Pallet::<T>::block_number());

		// Set the sync barrier to 0
		let zero_sync_barrier = VoteSynchronisationBarrier::from_u32(0);
		Pallet::<T, I>::ignore_my_votes(
			RawOrigin::Signed(caller.clone()).into(),
			zero_sync_barrier.clone(),
		)
		.unwrap();

		Pallet::<T, I>::stop_ignoring_my_votes(
			RawOrigin::Signed(caller.clone()).into(),
			zero_sync_barrier.clone(),
		)
		.unwrap();

		let (elections, sync_barrier) =
			Pallet::<T, I>::validator_election_data(&validator_id).unwrap();

		let next_election = elections.into_iter().next().unwrap();

		#[extrinsic_call]
		vote(
			RawOrigin::Signed(caller),
			BoundedBTreeMap::try_from(
				iter::repeat((next_election.0, T::ElectoralSystem::benchmark_authority_vote()))
					.take(n as usize)
					.collect::<BTreeMap<_, _>>(),
			)
			.unwrap(),
			sync_barrier.unwrap(),
		);
	}

	#[benchmark]
	fn ignore_my_votes() {
		let caller =
			T::AccountRoleRegistry::whitelisted_caller_with_role(AccountRole::Validator).unwrap();
		let validator_id: T::ValidatorId = caller.clone().into();

		T::EpochInfo::add_authority_info_for_epoch(2, vec![validator_id.clone()]);

		let zero_sync_barrier = VoteSynchronisationBarrier::from_u32(0);

		Status::<T, I>::put(ElectoralSystemStatus::Running);

		#[extrinsic_call]
		ignore_my_votes(RawOrigin::Signed(caller), zero_sync_barrier);

		assert!(AuthorityVoteSynchronisationBarriers::<T, I>::contains_key(validator_id.clone()));
	}

	#[benchmark]
	fn stop_ignoring_my_votes() {
		let caller =
			T::AccountRoleRegistry::whitelisted_caller_with_role(AccountRole::Validator).unwrap();
		let validator_id: T::ValidatorId = caller.clone().into();

		T::EpochInfo::add_authority_info_for_epoch(3, vec![validator_id.clone()]);

		Status::<T, I>::put(ElectoralSystemStatus::Running);

		AuthorityVoteSynchronisationBarriers::<T, I>::insert(
			validator_id.clone(),
			VoteSynchronisationBarrier::from_u32(0),
		);

		#[extrinsic_call]
		stop_ignoring_my_votes(RawOrigin::Signed(caller), VoteSynchronisationBarrier::from_u32(0));

		assert!(ContributingAuthorities::<T, I>::contains_key(validator_id.clone()));
	}

	#[benchmark]
	fn recheck_contributed_to_consensuses() {
		let caller =
			T::AccountRoleRegistry::whitelisted_caller_with_role(AccountRole::Validator).unwrap();
		let validator_id: T::ValidatorId = caller.clone().into();

		vote_in_epoch::<T, I>(caller.clone(), 1);

		#[block]
		{
			let _ = Pallet::<T, I>::recheck_contributed_to_consensuses(1, &validator_id, 1);
		}

		assert!(
			ElectionConsensusHistoryUpToDate::<T, I>::iter().count() == 0,
			"History not cleared"
		);
	}

	#[cfg(test)]
	use crate::mock::*;

	#[cfg(test)]
	use crate::Instance1;
	use crate::VoteSynchronisationBarrier;

	#[test]
	fn benchmark_works() {
		new_test_ext().execute_with(|| {
			_vote::<Test, Instance1>(10, true);
		});
		new_test_ext().execute_with(|| {
			_ignore_my_votes::<Test, Instance1>(true);
		});
		new_test_ext().execute_with(|| {
			_stop_ignoring_my_votes::<Test, Instance1>(true);
		});
		// new_test_ext().execute_with(|| {
		// 	_recheck_contributed_to_consensuses::<Test, Instance1>(true);
		// });
	}
}
