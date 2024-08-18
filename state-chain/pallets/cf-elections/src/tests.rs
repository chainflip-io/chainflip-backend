#![cfg(test)]

use std::collections::BTreeMap;

use cf_traits::EpochInfo;
use electoral_system::{AuthorityVoteOf, ElectoralReadAccess};
use frame_system::RawOrigin;
use sp_std::collections::btree_set::BTreeSet;

use mock::{new_test_ext, MockEpochInfo, Test, INITIAL_UNSYNCED_STATE};

use super::*;

#[test]
fn happy_path_vote_and_consensus() {
	const NEW_DATA: u64 = 23;

	fn submit_vote_for(validator_id: u64) {
		let elections =
			Pallet::<Test, Instance1>::validator_election_data(&validator_id).unwrap();

		for election in elections {
			Pallet::<Test, Instance1>::vote(
				RawOrigin::Signed(validator_id).into(),
				BoundedBTreeMap::try_from(
					[(
						election.0,
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
