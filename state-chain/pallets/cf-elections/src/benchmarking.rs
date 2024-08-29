#![cfg(feature = "runtime-benchmarks")]

use crate::{
	electoral_system::{AuthorityVoteOf, ElectoralSystem},
	vote_storage::VoteStorage,
	Config, ContributingAuthorities, ElectionConsensusHistoryUpToDate, ElectoralSystemStatus,
	Pallet, SharedData, SharedDataHash, Status,
};
use cf_chains::benchmarking_value::BenchmarkValue;
use cf_primitives::AccountRole;
use cf_traits::{AccountRoleRegistry, EpochInfo};
use frame_benchmarking::v2::*;
use frame_support::{assert_ok, storage::bounded_btree_map::BoundedBTreeMap};
use frame_system::RawOrigin;
use sp_std::collections::btree_map::BTreeMap;

use crate::Call;

#[instance_benchmarks(
	where
	<<<T as Config<I>>::ElectoralSystem as ElectoralSystem>::Vote as VoteStorage>::Vote: BenchmarkValue,
	<<<T as Config<I>>::ElectoralSystem as ElectoralSystem>::Vote as VoteStorage>::SharedData: BenchmarkValue,
)]
mod benchmarks {
	use core::iter;
	use frame_support::traits::OnFinalize;
	use sp_std::vec;

	use super::*;

	fn ready_validator_for_vote<T: crate::pallet::Config<I>, I: 'static>() -> T::AccountId {
		let caller =
			T::AccountRoleRegistry::whitelisted_caller_with_role(AccountRole::Validator).unwrap();
		let validator_id: T::ValidatorId = caller.clone().into();

		let epoch = T::EpochInfo::epoch_index();
		T::EpochInfo::add_authority_info_for_epoch(epoch, vec![validator_id.clone()]);

		// kick off an election
		Pallet::<T, I>::on_finalize(frame_system::Pallet::<T>::block_number());

		assert_ok!(Pallet::<T, I>::ignore_my_votes(RawOrigin::Signed(caller.clone()).into()));

		assert_ok!(Pallet::<T, I>::stop_ignoring_my_votes(
			RawOrigin::Signed(caller.clone()).into()
		));

		caller
	}

	#[benchmark]
	fn vote(n: Linear<1, 10>) {
		let validator_id: T::ValidatorId = ready_validator_for_vote::<T, I>().into();

		let elections = Pallet::<T, I>::electoral_data(&validator_id).unwrap().current_elections;
		let next_election = elections.into_iter().next().unwrap();

		#[extrinsic_call]
		vote(
			RawOrigin::Signed(validator_id.into()),
			BoundedBTreeMap::try_from(
				iter::repeat((
					next_election.0,
					AuthorityVoteOf::<T::ElectoralSystem>::Vote(BenchmarkValue::benchmark_value()),
				))
				.take(n as usize)
				.collect::<BTreeMap<_, _>>(),
			)
			.unwrap(),
		);
	}

	#[benchmark]
	fn stop_ignoring_my_votes() {
		let caller =
			T::AccountRoleRegistry::whitelisted_caller_with_role(AccountRole::Validator).unwrap();
		let validator_id: T::ValidatorId = caller.clone().into();
		let epoch = T::EpochInfo::epoch_index();

		T::EpochInfo::add_authority_info_for_epoch(epoch, vec![validator_id.clone()]);

		Status::<T, I>::put(ElectoralSystemStatus::Running);

		#[extrinsic_call]
		stop_ignoring_my_votes(RawOrigin::Signed(caller));

		assert!(ContributingAuthorities::<T, I>::contains_key(validator_id.clone()));
	}

	#[benchmark]
	fn ignore_my_votes() {
		let caller =
			T::AccountRoleRegistry::whitelisted_caller_with_role(AccountRole::Validator).unwrap();
		let validator_id: T::ValidatorId = caller.clone().into();
		let epoch = T::EpochInfo::epoch_index();

		T::EpochInfo::add_authority_info_for_epoch(epoch, vec![validator_id.clone()]);

		Status::<T, I>::put(ElectoralSystemStatus::Running);

		assert!(
			!ContributingAuthorities::<T, I>::contains_key(validator_id.clone()),
			"ContributingAuthorities is expected to be empty for this benchmark!"
		);

		#[extrinsic_call]
		ignore_my_votes(RawOrigin::Signed(caller));
	}

	#[benchmark]
	fn recheck_contributed_to_consensuses() {
		let caller = ready_validator_for_vote::<T, I>();
		let validator_id: T::ValidatorId = caller.clone().into();
		let epoch = T::EpochInfo::epoch_index();

		let elections = Pallet::<T, I>::electoral_data(&validator_id).unwrap().current_elections;
		let next_election = elections.into_iter().next().unwrap();

		Pallet::<T, I>::vote(
			RawOrigin::Signed(caller).into(),
			BoundedBTreeMap::try_from(
				[(
					next_election.0,
					AuthorityVoteOf::<T::ElectoralSystem>::Vote(BenchmarkValue::benchmark_value()),
				)]
				.into_iter()
				.collect::<BTreeMap<_, _>>(),
			)
			.unwrap(),
		)
		.unwrap();

		ElectionConsensusHistoryUpToDate::<T, I>::insert(next_election.0.unique_monotonic(), epoch);

		#[block]
		{
			let _ = Pallet::<T, I>::recheck_contributed_to_consensuses(epoch, &validator_id, epoch);
		}

		assert!(
			ElectionConsensusHistoryUpToDate::<T, I>::iter().count() == 0,
			"Expected ElectionConsensusHistoryUpToDate to be empty! Benchmark requirement are not met!"
		);
	}

	#[benchmark]
	fn delete_vote() {
		let caller = ready_validator_for_vote::<T, I>();
		let validator_id: T::ValidatorId = caller.clone().into();
		let epoch = T::EpochInfo::epoch_index();

		let elections = Pallet::<T, I>::electoral_data(&validator_id).unwrap().current_elections;
		let next_election = elections.into_iter().next().unwrap();

		Pallet::<T, I>::vote(
			RawOrigin::Signed(caller).into(),
			BoundedBTreeMap::try_from(
				[(
					next_election.0,
					AuthorityVoteOf::<T::ElectoralSystem>::Vote(BenchmarkValue::benchmark_value()),
				)]
				.into_iter()
				.collect::<BTreeMap<_, _>>(),
			)
			.unwrap(),
		)
		.unwrap();

		ElectionConsensusHistoryUpToDate::<T, I>::insert(next_election.0.unique_monotonic(), epoch);

		#[extrinsic_call]
		delete_vote(RawOrigin::Signed(validator_id.clone().into()), next_election.0);

		assert!(
            ElectionConsensusHistoryUpToDate::<T, I>::iter().count() == 0,
            "Expected ElectionConsensusHistoryUpToDate to be empty! Benchmark requirement are not met!"
        );

		assert!(SharedData::<T, I>::iter().count() == 0, "Expected SharedData to be deleted!");
	}

	#[benchmark]
	fn provide_shared_data() {
		let validator_id = ready_validator_for_vote::<T, I>();

		let (election_identifier, ..) =
			Pallet::<T, I>::electoral_data(&validator_id.clone().into())
				.unwrap()
				.current_elections
				.into_iter()
				.next()
				.unwrap();

		assert_ok!(Pallet::<T, I>::vote(
			RawOrigin::Signed(validator_id.clone()).into(),
			BoundedBTreeMap::try_from(
				[(
					election_identifier,
					AuthorityVoteOf::<T::ElectoralSystem>::Vote(BenchmarkValue::benchmark_value()),
				)]
				.into_iter()
				.collect::<BTreeMap<_, _>>(),
			)
			.unwrap(),
		));

		#[extrinsic_call]
		provide_shared_data(RawOrigin::Signed(validator_id), BenchmarkValue::benchmark_value());

		assert_eq!(
			SharedData::<T, I>::get(SharedDataHash::of::<
				<<<T as Config<I>>::ElectoralSystem as ElectoralSystem>::Vote as VoteStorage>::Vote,
			>(&BenchmarkValue::benchmark_value())),
			Some(BenchmarkValue::benchmark_value())
		);
	}

	#[cfg(test)]
	mod tests {
		use super::*;

		use crate::{mock::*, tests::ElectoralSystemTestExt, Instance1};

		macro_rules! benchmark_tests {
			( $( $test_name:ident: $test_fn:ident ( $( $arg:expr ),* ) ),+ $(,)? ) => {
				$(
					#[test]
					fn $test_name() {
						election_test_ext(Default::default())
							.new_election()
							.then_execute_with(|_| {
								$test_fn::<Test, Instance1>( $( $arg, )* true );
							});
					}
				)+
			};
		}

		benchmark_tests! {
			test_vote: _vote(10),
			test_stop_ignoring_my_votes: _stop_ignoring_my_votes(),
			test_ignore_my_votes: _ignore_my_votes(),
			test_recheck_contributed_to_consensuses: _recheck_contributed_to_consensuses(),
			test_delete_vote: _delete_vote(),
			test_provide_shared_data: _provide_shared_data(),
		}
	}
}
