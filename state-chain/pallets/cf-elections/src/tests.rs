#![cfg(test)]

use std::collections::BTreeMap;

use cf_traits::EpochInfo;
use electoral_system::{AuthorityVoteOf, ElectoralReadAccess};
use frame_support::{assert_noop, assert_ok};
use frame_system::RawOrigin;
use sp_std::collections::btree_set::BTreeSet;

use mock::{new_test_ext, MockEpochInfo, Test, INITIAL_UNSYNCED_STATE, NEW_DATA};
use storage::IterableStorageMap;

use super::*;

fn submit_vote_for(validator_id: u64, data: u64) {
	let electoral_data = Pallet::<Test, Instance1>::electoral_data(&validator_id).unwrap();

	for (election_identifier, _election_data) in electoral_data.current_elections {
		Pallet::<Test, Instance1>::vote(
			RawOrigin::Signed(validator_id).into(),
			BoundedBTreeMap::try_from(
				[(
					election_identifier,
					AuthorityVoteOf::<<Test as Config<Instance1>>::ElectoralSystem>::Vote(data),
				)]
				.into_iter()
				.collect::<BTreeMap<_, _>>(),
			)
			.unwrap(),
		)
		.unwrap();
	}
}

#[test]
fn happy_path_vote_and_consensus() {
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

			submit_vote_for(super_maj.next().unwrap(), NEW_DATA);
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

			submit_vote_for(next_validator, NEW_DATA);
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
fn provide_shared_data() {
	new_test_ext()
		// Run one block, which on_finalise will create the election for the median
		.then_execute_at_next_block(|()| {})
		// Do voting
		.then_execute_at_next_block(|()| {
			let current_authorities = MockEpochInfo::current_authorities();
			let validator_id = current_authorities
				.into_iter()
				.take(1)
				.collect::<BTreeSet<u64>>()
				.into_iter()
				.next()
				.unwrap();
			Pallet::<Test, Instance1>::stop_ignoring_my_votes(
				RawOrigin::Signed(validator_id).into(),
			)
			.unwrap();
			submit_vote_for(validator_id, NEW_DATA);
			validator_id
		})
		// Provide shared data for an election.
		.then_execute_at_next_block(|validator_id| {
			assert_eq!(
				SharedData::<Test, Instance1>::iter().count(),
				1,
				"No shared data found in storage!"
			);
			let referenced_shared_data = SharedData::<Test, Instance1>::iter().next().unwrap().1;
			let unreferenced_shared_data = referenced_shared_data + 1;
			// Provide unreferenced shared data
			assert_noop!(
				Pallet::<Test, Instance1>::provide_shared_data(
					RawOrigin::Signed(validator_id).into(),
					unreferenced_shared_data
				),
				Error::<Test, Instance1>::UnreferencedSharedData
			);
			// Provide referenced shared data
			assert_ok!(Pallet::<Test, Instance1>::provide_shared_data(
				RawOrigin::Signed(validator_id).into(),
				referenced_shared_data
			));
		});
}
