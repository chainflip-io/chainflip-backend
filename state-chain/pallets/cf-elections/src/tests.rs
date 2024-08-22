#![cfg(test)]

use super::*;
use sp_std::collections::btree_set::BTreeSet;
use std::collections::BTreeMap;

use cf_traits::EpochInfo;

use electoral_system::{AuthorityVoteOf, ElectoralReadAccess, ElectoralSystem};
use frame_support::assert_ok;
use frame_system::RawOrigin;
use mock::{new_test_ext, Elections, MockEpochInfo, Test, INITIAL_UNSYNCED_STATE};
use vote_storage::VoteStorage;

#[test]
fn happy_path_vote_and_consensus() {
	const NEW_DATA: u64 = 23;

	fn submit_vote_for(validator_id: u64) {
		let electoral_data = Pallet::<Test, Instance1>::electoral_data(&validator_id).unwrap();

		for (election_identifier, _election_data) in electoral_data.current_elections {
			Pallet::<Test, Instance1>::vote(
				RawOrigin::Signed(validator_id).into(),
				BoundedBTreeMap::try_from(
					[(
						election_identifier,
						AuthorityVoteOf::<<Test as Config<Instance1>>::ElectoralSystem>::Vote(
							NEW_DATA,
						),
					)]
					.into_iter()
					.collect::<BTreeMap<_, _>>(),
				)
				.unwrap(),
			)
			.unwrap();
		}
	}

	new_test_ext()
		// Run one block, which on_finalise will create the election for the median
		.then_execute_at_next_block(|()| {
			Pallet::<Test, Instance1>::with_electoral_access(|electoral_access| {
				assert_eq!(
					electoral_access.unsynchronised_state().unwrap(),
					INITIAL_UNSYNCED_STATE
				);
				Ok(())
			})
			.unwrap();
		})
		.then_execute_at_next_block(|()| {
			assert_eq!(Status::<Test, Instance1>::get(), Some(ElectoralSystemStatus::Running));

			let current_authorities = MockEpochInfo::current_authorities();
			for validator_id in current_authorities.clone() {
				Pallet::<Test, Instance1>::stop_ignoring_my_votes(
					RawOrigin::Signed(validator_id).into(),
				)
				.unwrap()
			}

			assert_ne!(NEW_DATA, INITIAL_UNSYNCED_STATE);

			let mut super_maj =
				current_authorities.into_iter().take(2).collect::<BTreeSet<u64>>().into_iter();

			submit_vote_for(super_maj.next().unwrap());
			super_maj.next().unwrap()
		})
		// Only one vote means we have not reached consensus, so the state should not change
		.then_execute_at_next_block(|next_validator| {
			Pallet::<Test, Instance1>::with_electoral_access(|electoral_access| {
				assert_eq!(
					electoral_access.unsynchronised_state().unwrap(),
					INITIAL_UNSYNCED_STATE
				);
				Ok(())
			})
			.unwrap();

			submit_vote_for(next_validator);
		})
		// 2 votes is now consensus, so the state should be updated
		.then_execute_at_next_block(|()| {
			Pallet::<Test, Instance1>::with_electoral_access(|electoral_access| {
				assert_eq!(electoral_access.unsynchronised_state().unwrap(), NEW_DATA);
				Ok(())
			})
			.unwrap();
		});
}

#[test]
fn can_provide_shared_data() {
	const NEW_DATA: u64 = 23;

	new_test_ext().execute_with(|| {
		Elections::on_finalize(1);

		let validator_id = MockEpochInfo::current_authorities()[0];
		let elections = Pallet::<Test, Instance1>::electoral_data(&validator_id)
			.unwrap()
			.current_elections;

		let election_id = elections.into_iter().next().unwrap().0;

		assert_ok!(Pallet::<Test, Instance1>::ignore_my_votes(
			RawOrigin::Signed(validator_id).into(),
		));
		assert_ok!(Pallet::<Test, Instance1>::stop_ignoring_my_votes(
			RawOrigin::Signed(validator_id).into(),
		));

		assert_ok!(Pallet::<Test, Instance1>::vote(
			RawOrigin::Signed(validator_id).into(),
			BoundedBTreeMap::try_from(
				[(
					election_id,
					AuthorityVoteOf::<<Test as Config<Instance1>>::ElectoralSystem>::Vote(NEW_DATA,),
				)]
				.into_iter()
				.collect::<BTreeMap<_, _>>(),
			)
			.unwrap(),
		));

		assert_ok!(Elections::provide_shared_data(
			RawOrigin::Signed(validator_id).into(),
			NEW_DATA
		));

		assert_eq!(
			SharedData::<Test, Instance1>::get(
				SharedDataHash::of::<<<<Test as Config<Instance1>>::ElectoralSystem as ElectoralSystem>::Vote as VoteStorage>::Vote>(&NEW_DATA)
			),
			Some(NEW_DATA)
		);
	});
}
