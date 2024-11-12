use crate::{
	bitmap_components::ElectionBitmapComponents,
	electoral_system_runner::{
		CompositeAuthorityVoteOf, CompositeIndividualComponentOf, CompositeVotePropertiesOf,
		ElectoralSystemRunner,
	},
	vote_storage::VoteStorage,
	*,
};
use cf_chains::benchmarking_value::BenchmarkValue;
use cf_primitives::AccountRole;
use cf_traits::{AccountRoleRegistry, EpochInfo};
use core::iter;
use frame_benchmarking::v2::*;
use frame_support::{
	assert_ok,
	storage::bounded_btree_map::BoundedBTreeMap,
	traits::{EnsureOrigin, Hooks, UnfilteredDispatchable},
};
use frame_system::RawOrigin;
use sp_std::{collections::btree_map::BTreeMap, vec, vec::Vec};

use crate::Call;

#[allow(clippy::multiple_bound_locations)]
#[instance_benchmarks(
	where
	<<<T as Config<I>>::ElectoralSystemRunner as ElectoralSystemRunner>::Vote as VoteStorage>::Vote: BenchmarkValue,
	<<<T as Config<I>>::ElectoralSystemRunner as ElectoralSystemRunner>::Vote as VoteStorage>::SharedData: BenchmarkValue,
	<<<T as Config<I>>::ElectoralSystemRunner as ElectoralSystemRunner>::Vote as VoteStorage>::Properties: BenchmarkValue,
	<<<T as Config<I>>::ElectoralSystemRunner as ElectoralSystemRunner>::Vote as VoteStorage>::IndividualComponent: BenchmarkValue,
	InitialStateOf<T, I>: BenchmarkValue,
	<T::ElectoralSystemRunner as ElectoralSystemRunner>::ElectoralUnsynchronisedSettings: BenchmarkValue,
	<T::ElectoralSystemRunner as ElectoralSystemRunner>::ElectoralSettings: BenchmarkValue,
)]
mod benchmarks {
	use super::*;

	fn ready_validator_for_vote<T: crate::pallet::Config<I>, I: 'static>(
		validator_counts: u32,
	) -> Vec<T::AccountId> {
		let validators = T::AccountRoleRegistry::generate_whitelisted_callers_with_role(
			AccountRole::Validator,
			validator_counts,
		)
		.unwrap();

		let epoch = T::EpochInfo::epoch_index();
		T::EpochInfo::add_authority_info_for_epoch(
			epoch,
			validators.clone().into_iter().map(|v| v.into()).collect(),
		);

		// kick off an election
		Pallet::<T, I>::on_finalize(frame_system::Pallet::<T>::block_number());

		validators.iter().for_each(|v| {
			assert_ok!(Pallet::<T, I>::ignore_my_votes(RawOrigin::Signed(v.clone()).into()));
			assert_ok!(Pallet::<T, I>::stop_ignoring_my_votes(RawOrigin::Signed(v.clone()).into()));
		});

		validators
	}

	fn setup_validators_and_vote<T: crate::pallet::Config<I>, I: 'static>(
		validator_counts: u32,
		vote_value: <<<T as Config<I>>::ElectoralSystemRunner as ElectoralSystemRunner>::Vote as VoteStorage>::Vote,
	) -> CompositeElectionIdentifierOf<T::ElectoralSystemRunner> {
		// Setup a validator set of 150 as in the case of Mainnet.
		let validators = ready_validator_for_vote::<T, I>(validator_counts);
		let caller = validators[0].clone();
		let (election_identifier, ..) = Pallet::<T, I>::electoral_data(&caller.clone().into())
			.unwrap()
			.current_elections
			.into_iter()
			.next()
			.unwrap();

		validators.iter().for_each(|v| {
			assert_ok!(Pallet::<T, I>::vote(
				RawOrigin::Signed(v.clone()).into(),
				BoundedBTreeMap::try_from(
					[(
						election_identifier,
						CompositeAuthorityVoteOf::<T::ElectoralSystemRunner>::Vote(
							vote_value.clone()
						),
					)]
					.into_iter()
					.collect::<BTreeMap<_, _>>(),
				)
				.unwrap(),
			));
		});
		election_identifier
	}

	#[benchmark]
	fn vote(n: Linear<1, 10>) {
		let validator_id: T::ValidatorId = ready_validator_for_vote::<T, I>(1)[0].clone().into();

		let elections = Pallet::<T, I>::electoral_data(&validator_id).unwrap().current_elections;
		let next_election = elections.into_iter().next().unwrap();

		#[extrinsic_call]
		vote(
			RawOrigin::Signed(validator_id.into()),
			BoundedBTreeMap::try_from(
				iter::repeat((
					next_election.0,
					CompositeAuthorityVoteOf::<T::ElectoralSystemRunner>::Vote(
						BenchmarkValue::benchmark_value(),
					),
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

		Status::<T, I>::put(ElectionPalletStatus::Running);

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

		Status::<T, I>::put(ElectionPalletStatus::Running);

		assert!(
			!ContributingAuthorities::<T, I>::contains_key(validator_id.clone()),
			"ContributingAuthorities is expected to be empty for this benchmark!"
		);

		#[extrinsic_call]
		ignore_my_votes(RawOrigin::Signed(caller));
	}

	#[benchmark]
	fn recheck_contributed_to_consensuses() {
		let caller = ready_validator_for_vote::<T, I>(1)[0].clone();
		let validator_id: T::ValidatorId = caller.clone().into();
		let epoch = T::EpochInfo::epoch_index();

		let elections = Pallet::<T, I>::electoral_data(&validator_id).unwrap().current_elections;
		let next_election = elections.into_iter().next().unwrap();

		assert_ok!(Pallet::<T, I>::vote(
			RawOrigin::Signed(caller).into(),
			BoundedBTreeMap::try_from(
				[(
					next_election.0,
					CompositeAuthorityVoteOf::<T::ElectoralSystemRunner>::Vote(
						BenchmarkValue::benchmark_value()
					),
				)]
				.into_iter()
				.collect::<BTreeMap<_, _>>(),
			)
			.unwrap(),
		));

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
		let caller = ready_validator_for_vote::<T, I>(1)[0].clone();
		let validator_id: T::ValidatorId = caller.clone().into();
		let epoch = T::EpochInfo::epoch_index();

		let elections = Pallet::<T, I>::electoral_data(&validator_id).unwrap().current_elections;
		let next_election = elections.into_iter().next().unwrap();

		assert_ok!(Pallet::<T, I>::vote(
			RawOrigin::Signed(caller).into(),
			BoundedBTreeMap::try_from(
				[(
					next_election.0,
					CompositeAuthorityVoteOf::<T::ElectoralSystemRunner>::Vote(
						BenchmarkValue::benchmark_value()
					),
				)]
				.into_iter()
				.collect::<BTreeMap<_, _>>(),
			)
			.unwrap(),
		));

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
		let validator_id = ready_validator_for_vote::<T, I>(1)[0].clone();

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
					CompositeAuthorityVoteOf::<T::ElectoralSystemRunner>::Vote(
						BenchmarkValue::benchmark_value()
					),
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
				<<<T as Config<I>>::ElectoralSystemRunner as ElectoralSystemRunner>::Vote as VoteStorage>::Vote,
			>(&BenchmarkValue::benchmark_value())),
			Some(BenchmarkValue::benchmark_value())
		);
	}

	#[benchmark]
	fn initialize() {
		Status::<T, I>::set(None);
		let call = Call::<T, I>::initialize { initial_state: BenchmarkValue::benchmark_value() };

		#[block]
		{
			assert_ok!(
				call.dispatch_bypass_filter(T::EnsureGovernance::try_successful_origin().unwrap())
			);
		}

		// Ensure elections are initialised
		assert!(ElectoralUnsynchronisedState::<T, I>::get().is_some());
		assert!(ElectoralUnsynchronisedSettings::<T, I>::get().is_some());
		assert!(ElectoralSettings::<T, I>::get(NextElectionIdentifier::<T, I>::get()).is_some());
		assert_eq!(Status::<T, I>::get(), Some(ElectionPalletStatus::Running));
	}

	#[benchmark]
	fn update_settings() {
		// Initialize the elections
		Status::<T, I>::set(None);
		assert_ok!(Call::<T, I>::initialize { initial_state: BenchmarkValue::benchmark_value() }
			.dispatch_bypass_filter(T::EnsureGovernance::try_successful_origin().unwrap()));
		let next_election = NextElectionIdentifier::<T, I>::get();

		// Clear the storage so it can be "re-set".
		ElectoralUnsynchronisedSettings::<T, I>::set(None);
		ElectoralSettings::<T, I>::remove(next_election);

		let call = Call::<T, I>::update_settings {
			unsynchronised_settings: Some(BenchmarkValue::benchmark_value()),
			settings: Some(BenchmarkValue::benchmark_value()),
			ignore_corrupt_storage: CorruptStorageAdherance::Heed,
		};

		#[block]
		{
			assert_ok!(
				call.dispatch_bypass_filter(T::EnsureGovernance::try_successful_origin().unwrap())
			);
		}

		// Settings are updated.
		assert!(ElectoralUnsynchronisedSettings::<T, I>::get().is_some());
		assert!(ElectoralSettings::<T, I>::get(next_election).is_some());
	}

	#[benchmark]
	fn set_shared_data_reference_lifetime() {
		// Initialize the elections
		Status::<T, I>::set(None);
		assert_ok!(Call::<T, I>::initialize { initial_state: BenchmarkValue::benchmark_value() }
			.dispatch_bypass_filter(T::EnsureGovernance::try_successful_origin().unwrap()));

		assert_eq!(SharedDataReferenceLifetime::<T, I>::get(), Default::default());
		let lifetime = BlockNumberFor::<T>::from(100u32);
		let call = Call::<T, I>::set_shared_data_reference_lifetime {
			blocks: lifetime,
			ignore_corrupt_storage: CorruptStorageAdherance::Heed,
		};

		#[block]
		{
			assert_ok!(
				call.dispatch_bypass_filter(T::EnsureGovernance::try_successful_origin().unwrap())
			);
		}

		assert_eq!(SharedDataReferenceLifetime::<T, I>::get(), lifetime);
	}

	#[benchmark]
	fn clear_election_votes() {
		// Setup a validator set of 150 as in the case of Mainnet.
		let election_identifier =
			setup_validators_and_vote::<T, I>(150, BenchmarkValue::benchmark_value());

		let call = Call::<T, I>::clear_election_votes {
			election_identifier,
			ignore_corrupt_storage: CorruptStorageAdherance::Heed,
			check_election_exists: true,
		};

		#[block]
		{
			assert_ok!(
				call.dispatch_bypass_filter(T::EnsureGovernance::try_successful_origin().unwrap())
			);
		}

		assert!(!ElectionConsensusHistoryUpToDate::<T, I>::contains_key(
			election_identifier.unique_monotonic()
		));
	}

	#[benchmark]
	fn invalidate_election_consensus_cache() {
		// Setup a validator set of 150 and reach consensus
		let election_identifier =
			setup_validators_and_vote::<T, I>(150, BenchmarkValue::benchmark_value());

		let call = Call::<T, I>::invalidate_election_consensus_cache {
			election_identifier,
			ignore_corrupt_storage: CorruptStorageAdherance::Heed,
			check_election_exists: true,
		};

		let epoch = T::EpochInfo::epoch_index();
		let monotonic_identifier = election_identifier.unique_monotonic();

		Pallet::<T, I>::on_finalize(frame_system::Pallet::<T>::block_number());
		assert_eq!(
			ElectionConsensusHistoryUpToDate::<T, I>::get(monotonic_identifier),
			Some(epoch),
		);

		#[block]
		{
			assert_ok!(
				call.dispatch_bypass_filter(T::EnsureGovernance::try_successful_origin().unwrap())
			);
		}

		assert!(!ElectionConsensusHistoryUpToDate::<T, I>::contains_key(monotonic_identifier));
	}

	#[benchmark]
	fn pause_elections() {
		let _validator_id = ready_validator_for_vote::<T, I>(1);
		let call = Call::<T, I>::pause_elections {};

		#[block]
		{
			assert_ok!(
				call.dispatch_bypass_filter(T::EnsureGovernance::try_successful_origin().unwrap())
			);
		}

		// Ensure elections are paused
		assert_eq!(
			Status::<T, I>::get(),
			Some(ElectionPalletStatus::Paused { detected_corrupt_storage: false })
		);
	}

	#[benchmark]
	fn unpause_elections() {
		let _validator_id = ready_validator_for_vote::<T, I>(1);

		// Pause the elections
		assert_ok!(Call::<T, I>::pause_elections {}
			.dispatch_bypass_filter(T::EnsureGovernance::try_successful_origin().unwrap()));
		assert_eq!(
			Status::<T, I>::get(),
			Some(ElectionPalletStatus::Paused { detected_corrupt_storage: false })
		);
		assert_ok!(Call::<T, I>::clear_all_votes {
			limit: 100u32,
			ignore_corrupt_storage: CorruptStorageAdherance::Ignore,
		}
		.dispatch_bypass_filter(T::EnsureGovernance::try_successful_origin().unwrap()));

		let call = Call::<T, I>::unpause_elections { require_votes_cleared: true };

		#[block]
		{
			assert_ok!(
				call.dispatch_bypass_filter(T::EnsureGovernance::try_successful_origin().unwrap())
			);
		}

		// Ensure elections are unpaused
		assert_eq!(Status::<T, I>::get(), Some(ElectionPalletStatus::Running));
	}

	#[benchmark]
	fn validate_storage() {
		let _validator_id = ready_validator_for_vote::<T, I>(1);

		// Pause the election, and set corrupt storage to `true`
		assert_ok!(Call::<T, I>::pause_elections {}
			.dispatch_bypass_filter(T::EnsureGovernance::try_successful_origin().unwrap()));
		Status::<T, I>::put(ElectionPalletStatus::Paused { detected_corrupt_storage: true });

		let call = Call::<T, I>::validate_storage {};

		#[block]
		{
			assert_ok!(
				call.dispatch_bypass_filter(T::EnsureGovernance::try_successful_origin().unwrap())
			);
		}

		assert_eq!(
			Status::<T, I>::get(),
			Some(ElectionPalletStatus::Paused { detected_corrupt_storage: false })
		);
	}

	#[benchmark]
	fn clear_all_votes(
		a: Linear<1, 10>,
		b: Linear<1, 10>,
		c: Linear<1, 10>,
		d: Linear<1, 10>,
		e: Linear<1, 10>,
	) {
		let validators = ready_validator_for_vote::<T, I>(10);
		let epoch = T::EpochInfo::epoch_index();

		let reference_details = ReferenceDetails::<BlockNumberFor<T>> {
			count: 1u32,
			created: BlockNumberFor::<T>::from(1u32),
			expires: BlockNumberFor::<T>::from(10u32),
		};

		(0..a).for_each(|i| {
			SharedDataReferenceCount::<T, I>::insert(
				SharedDataHash::of(&i),
				UniqueMonotonicIdentifier::from_u64(i as u64),
				reference_details.clone(),
			);
		});

		(0..b).for_each(|i| {
			SharedData::<T, I>::insert(
				SharedDataHash::of(&i),
				<<T::ElectoralSystemRunner as ElectoralSystemRunner>::Vote as VoteStorage>::SharedData::benchmark_value());
		});

		(0..c).for_each(|i| {
			ElectionBitmapComponents::<T, I>::with(
				epoch,
				UniqueMonotonicIdentifier::from_u64(i as u64),
				|_a| Ok(i),
			)
			.unwrap();
		});

		(0..d).for_each(|i| {
			IndividualComponents::<T, I>::insert(
				UniqueMonotonicIdentifier::from_u64(i as u64),
				T::ValidatorId::from(validators[i as usize].clone()),
				(
					CompositeVotePropertiesOf::<T::ElectoralSystemRunner>::benchmark_value(),
					CompositeIndividualComponentOf::<T::ElectoralSystemRunner>::benchmark_value(),
				),
			);
		});

		(0..e).for_each(|i| {
			ElectionConsensusHistoryUpToDate::<T, I>::insert(
				UniqueMonotonicIdentifier::from_u64(i as u64),
				epoch,
			);
		});

		let call = Call::<T, I>::clear_all_votes {
			limit: 1_000u32,
			ignore_corrupt_storage: CorruptStorageAdherance::Heed,
		};

		#[block]
		{
			assert_ok!(
				call.dispatch_bypass_filter(T::EnsureGovernance::try_successful_origin().unwrap())
			);
		}

		assert_eq!(ElectionConsensusHistoryUpToDate::<T, I>::iter_keys().count(), 0);
	}

	#[cfg(test)]
	mod tests {
		use super::*;

		use crate::{mock::*, tests::ElectoralSystemRunnerTestExt, Instance1};

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
			test_initialize: _initialize(),
			test_update_settings: _update_settings(),
			test_set_shared_data_reference_lifetime: _set_shared_data_reference_lifetime(),
			test_clear_election_votes: _clear_election_votes(),
			test_invalidate_election_consensus_cache: _invalidate_election_consensus_cache(),
			test_pause_elections: _pause_elections(),
			test_unpause_elections: _unpause_elections(),
			test_validate_storage: _validate_storage(),
			test_clear_all_votes: _clear_all_votes(10, 10, 10, 10, 10),
		}
	}
}
