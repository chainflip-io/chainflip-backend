use crate::{mock::*, *};
use cf_traits::{mocks::epoch_info::MockEpochInfo, offence_reporting::*, EpochInfo, QualifyNode};
use frame_support::{assert_noop, assert_ok, traits::OnInitialize};

fn reputation_points(who: &<Test as frame_system::Config>::AccountId) -> ReputationPoints {
	ReputationPallet::reputation(who).reputation_points
}

pub fn advance_by_hearbeat_intervals(n: u64) {
	for _ in 0..n * HeartbeatBlockInterval::get() {
		let next_block = System::block_number() + 1;
		System::set_block_number(next_block);
		AllPalletsWithoutSystem::on_initialize(next_block);
	}
}

// Move forward one heartbeat interval sending the heartbeat extrinsic for nodes
fn submit_heartbeat_and_move_forward_heartbeat_interval(
	node: <Test as frame_system::Config>::AccountId,
) {
	assert_ok!(ReputationPallet::heartbeat(Origin::signed(node)));
	advance_by_hearbeat_intervals(1);
}

#[test]
fn submitting_heartbeat_for_heartbeat_block_interval_should_reward_reputation_points() {
	new_test_ext().execute_with(|| {
		ReputationPallet::heartbeat(Origin::signed(ALICE)).unwrap();
		assert_eq!(reputation_points(&ALICE), REPUTATION_PER_HEARTBEAT,);
	});
}

#[test]
fn missing_a_heartbeat_deducts_penalty_points() {
	new_test_ext().execute_with(|| {
		ReputationPallet::penalise_offline_authorities(vec![ALICE]);
		assert_eq!(reputation_points(&ALICE), -MISSED_HEARTBEAT_PENALTY_POINTS);
	});
}

#[test]
fn offline_nodes_get_slashed_if_reputation_is_negative() {
	new_test_ext().execute_with(|| {
		assert_eq!(reputation_points(&ALICE), 0);
		ReputationPallet::penalise_offline_authorities(vec![ALICE]);
		assert_eq!(MockSlasher::slash_count(ALICE), 1);
	});
}

#[test]
fn only_one_heartbeat_per_interval_earns_reputation() {
	new_test_ext().execute_with(|| {
		ReputationPallet::heartbeat(Origin::signed(ALICE)).unwrap();
		assert_eq!(reputation_points(&ALICE), REPUTATION_PER_HEARTBEAT,);
		// submit again, then move forward
		submit_heartbeat_and_move_forward_heartbeat_interval(ALICE);
		// no change in reputation, because we were on the same block, therefore we were in the same
		// heartbeat block interval
		assert_eq!(reputation_points(&ALICE), REPUTATION_PER_HEARTBEAT,);
		// we've moved forward a block interval, so now we should have the extra rep
		ReputationPallet::heartbeat(Origin::signed(ALICE)).unwrap();
		assert_eq!(reputation_points(&ALICE), REPUTATION_PER_HEARTBEAT * 2,);
	})
}

#[test]
fn updating_accrual_rate_should_affect_reputation_points() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			ReputationPallet::update_accrual_ratio(
				Origin::root(),
				MAX_REPUTATION_POINT_ACCRUED + 1,
				0
			),
			Error::<Test>::InvalidAccrualRatio,
		);

		assert_noop!(
			ReputationPallet::update_accrual_ratio(Origin::root(), MAX_REPUTATION_POINT_ACCRUED, 0),
			Error::<Test>::InvalidAccrualRatio,
		);

		assert_ok!(ReputationPallet::update_accrual_ratio(
			Origin::root(),
			ACCRUAL_RATE.0,
			ACCRUAL_RATE.1,
		));

		assert_eq!(ReputationPallet::accrual_ratio(), ACCRUAL_RATE);

		submit_heartbeat_and_move_forward_heartbeat_interval(ALICE);
		assert_eq!(reputation_points(&ALICE), REPUTATION_PER_HEARTBEAT);

		// Double the accrual rate.
		assert_ok!(ReputationPallet::update_accrual_ratio(
			Origin::root(),
			ACCRUAL_RATE.0 * 2,
			ACCRUAL_RATE.1,
		));

		submit_heartbeat_and_move_forward_heartbeat_interval(ALICE);
		assert_eq!(reputation_points(&ALICE), REPUTATION_PER_HEARTBEAT * 3);

		// Halve the divisor, equivalent to double the initial rate.
		assert_ok!(ReputationPallet::update_accrual_ratio(
			Origin::root(),
			ACCRUAL_RATE.0,
			ACCRUAL_RATE.1 / 2,
		));

		submit_heartbeat_and_move_forward_heartbeat_interval(ALICE);
		assert_eq!(reputation_points(&ALICE), REPUTATION_PER_HEARTBEAT * 5);
	});
}

frame_support::parameter_types! {
	pub const MissedHeartbeat: AllOffences = AllOffences::MissedHeartbeat;
	pub const ForgettingYourYubiKey: AllOffences = AllOffences::ForgettingYourYubiKey;
	pub const NotLockingYourComputer: AllOffences = AllOffences::NotLockingYourComputer;
}

#[test]
fn reporting_any_offence_should_penalise_reputation_points_and_suspend() {
	new_test_ext().execute_with(|| {
		let offline_test = |offence: AllOffences, who: &[u64]| {
			let penalty = ReputationPallet::resolve_penalty_for(offence);
			let points_before = who.iter().map(reputation_points).collect::<Vec<_>>();
			<ReputationPallet as OffenceReporter>::report_many(offence, who);
			for (id, points) in who.iter().zip(points_before) {
				assert_eq!(reputation_points(id), points - penalty.reputation,);
			}
			assert_eq!(
				ReputationPallet::validators_suspended_for(&[offence]),
				if !penalty.suspension.is_zero() {
					who.iter().cloned().collect::<BTreeSet<_>>()
				} else {
					BTreeSet::default()
				}
			);
		};
		offline_test(AllOffences::MissedHeartbeat, &[ALICE]);
		offline_test(AllOffences::ForgettingYourYubiKey, &[ALICE, BOB]);
		offline_test(AllOffences::NotLockingYourComputer, &[BOB]);

		// Heartbeats have no explicit suspension.
		assert_eq!(
			ReputationPallet::validators_suspended_for(&[AllOffences::MissedHeartbeat,]),
			[].iter().cloned().collect(),
		);
		assert_eq!(
			ReputationPallet::validators_suspended_for(&[
				AllOffences::MissedHeartbeat,
				AllOffences::ForgettingYourYubiKey,
				AllOffences::NotLockingYourComputer
			]),
			[ALICE, BOB].into_iter().collect(),
		);
	});
}

#[test]
fn suspensions() {
	new_test_ext().execute_with(|| {
		ReputationPallet::suspend_all(&[1, 2, 3], &AllOffences::ForgettingYourYubiKey, 10);
		assert_eq!(
			ReputationPallet::validators_suspended_for(&[AllOffences::ForgettingYourYubiKey,]),
			[1, 2, 3].into_iter().collect(),
		);
	});
}

#[test]
fn forgiveness() {
	impl OffenceList<Test> for AllOffences {
		const OFFENCES: &'static [Self] = &[
			AllOffences::ForgettingYourYubiKey,
			AllOffences::NotLockingYourComputer,
			AllOffences::MissedHeartbeat,
		];
	}

	new_test_ext().execute_with(|| {
		ReputationPallet::suspend_all(&[1, 2, 3], &AllOffences::ForgettingYourYubiKey, 10);
		ReputationPallet::suspend_all(&[1, 2], &AllOffences::NotLockingYourComputer, u64::MAX);
		ReputationPallet::suspend_all(&[1], &AllOffences::MissedHeartbeat, 15);
		assert_eq!(
			GetValidatorsExcludedFor::<Test, AllOffences>::get(),
			[1, 2, 3].into_iter().collect(),
		);
		<ReputationPallet as OffenceReporter>::forgive_all(AllOffences::ForgettingYourYubiKey);
		assert_eq!(
			GetValidatorsExcludedFor::<Test, AllOffences>::get(),
			[1, 2].into_iter().collect(),
		);
		<ReputationPallet as OffenceReporter>::forgive_all(AllOffences::NotLockingYourComputer);
		assert_eq!(GetValidatorsExcludedFor::<Test, AllOffences>::get(), [1].into_iter().collect(),);
		<ReputationPallet as OffenceReporter>::forgive_all(PalletOffence::MissedHeartbeat);
		assert_eq!(GetValidatorsExcludedFor::<Test, AllOffences>::get(), [].into_iter().collect(),);
	});
}

#[cfg(test)]
mod reporting_adapter_test {
	use super::*;
	use frame_support::assert_err;
	use pallet_grandpa::{GrandpaEquivocationOffence, GrandpaTimeSlot};
	use sp_staking::offence::ReportOffence;

	type IdentificationTuple = (u64, ());

	type GrandpaOffenceReporter =
		ChainflipOffenceReportingAdapter<Test, GrandpaEquivocationOffence<IdentificationTuple>, ()>;

	impl From<GrandpaEquivocationOffence<IdentificationTuple>> for AllOffences {
		fn from(_: GrandpaEquivocationOffence<IdentificationTuple>) -> Self {
			Self::UpsettingGrandpa
		}
	}

	impl OffenceList<Test> for GrandpaEquivocationOffence<IdentificationTuple> {
		const OFFENCES: &'static [AllOffences] = &[AllOffences::UpsettingGrandpa];
	}

	#[test]
	fn test_with_grandpa_equivocation_offence() {
		new_test_ext().execute_with(|| {
			const OFFENDER: IdentificationTuple = (42, ());
			const OFFENCE_TIME_SLOT: GrandpaTimeSlot = GrandpaTimeSlot { set_id: 0, round: 0 };
			const OFFENCE: GrandpaEquivocationOffence<IdentificationTuple> =
				GrandpaEquivocationOffence {
					time_slot: OFFENCE_TIME_SLOT,
					session_index: 0,
					validator_set_count: 1,
					offender: OFFENDER,
				};

			// Offence for this time slot is not known, nobody has been reported yet.
			assert!(!GrandpaOffenceReporter::is_known_offence(&[OFFENDER], &OFFENCE_TIME_SLOT));
			assert!(GetValidatorsExcludedFor::<
				Test,
				GrandpaEquivocationOffence<IdentificationTuple>,
			>::get()
			.is_empty());
			assert_eq!(MockSlasher::slash_count(OFFENDER.0), 0);

			// Report the offence. It should now be known, and a duplicate report should not be
			// possible.
			assert_ok!(GrandpaOffenceReporter::report_offence(Default::default(), OFFENCE,));
			assert!(GrandpaOffenceReporter::is_known_offence(&[OFFENDER], &OFFENCE_TIME_SLOT));
			assert_err!(
				GrandpaOffenceReporter::report_offence(Default::default(), OFFENCE,),
				sp_staking::offence::OffenceError::DuplicateReport
			);

			// The offender is suspended and reputation reduced.
			assert_eq!(
				GetValidatorsExcludedFor::<Test, GrandpaEquivocationOffence<IdentificationTuple>>::get(),
				[OFFENDER.0].into_iter().collect()
			);
			assert_eq!(
				ReputationPallet::reputation(OFFENDER.0).reputation_points,
				-GRANDPA_EQUIVOCATION_PENALTY_POINTS
			);
			assert_eq!(MockSlasher::slash_count(OFFENDER.0), 1);

			// Once an offence has been reported, it's not possible to report an offence for a
			// previous time slot.
			const NEXT_TIME_SLOT: GrandpaTimeSlot =
				GrandpaTimeSlot { set_id: OFFENCE_TIME_SLOT.set_id + 1, round: 0 };
			const FUTURE_TIME_SLOT: GrandpaTimeSlot =
				GrandpaTimeSlot { set_id: OFFENCE_TIME_SLOT.set_id + 2, round: 0 };
			const FUTURE_OFFENCE: GrandpaEquivocationOffence<IdentificationTuple> =
				GrandpaEquivocationOffence {
					time_slot: FUTURE_TIME_SLOT,
					session_index: 10,
					validator_set_count: 1,
					offender: OFFENDER,
				};
			assert!(!GrandpaOffenceReporter::is_known_offence(&[OFFENDER], &NEXT_TIME_SLOT));
			assert!(!GrandpaOffenceReporter::is_known_offence(&[OFFENDER], &FUTURE_TIME_SLOT));
			assert_ok!(GrandpaOffenceReporter::report_offence(Default::default(), FUTURE_OFFENCE,));
			assert!(GrandpaOffenceReporter::is_known_offence(&[OFFENDER], &NEXT_TIME_SLOT));
			assert!(GrandpaOffenceReporter::is_known_offence(&[OFFENDER], &FUTURE_TIME_SLOT));
			assert_eq!(MockSlasher::slash_count(OFFENDER.0), 2);
		});
	}
}

#[test]
fn submitting_heartbeat_more_than_once_in_an_interval() {
	new_test_ext().execute_with(|| {
		assert_ok!(ReputationPallet::heartbeat(Origin::signed(ALICE)));
		assert!(ReputationPallet::is_qualified(&ALICE), "Alice should be online");
		assert_ok!(ReputationPallet::heartbeat(Origin::signed(ALICE)));
		assert!(ReputationPallet::is_qualified(&ALICE), "Alice should be online");
		advance_by_hearbeat_intervals(1);
		assert_ok!(ReputationPallet::heartbeat(Origin::signed(ALICE)));
		assert!(ReputationPallet::is_qualified(&ALICE), "Alice should be online");
	});
}

#[test]
fn we_should_see_missing_nodes_when_not_having_submitted_one_interval() {
	new_test_ext().execute_with(|| {
		assert_ok!(ReputationPallet::heartbeat(Origin::signed(ALICE)));
		assert!(ReputationPallet::is_qualified(&ALICE), "Alice should be online");
		advance_by_hearbeat_intervals(1);
		assert_eq!(
			ReputationPallet::current_network_state().offline,
			vec![ALICE],
			"Alice should be offline after missing one heartbeat"
		);
		assert_eq!(
			ReputationPallet::current_network_state().number_of_nodes(),
			1,
			"We should have one node"
		);
		assert_ok!(ReputationPallet::heartbeat(Origin::signed(ALICE)));
		assert_eq!(
			ReputationPallet::current_network_state().online,
			vec![ALICE],
			"Alice should be offline after missing one heartbeat"
		);
	});
}

#[test]
fn non_validators_should_not_appear_in_network_state() {
	new_test_ext().execute_with(|| {
		assert_ok!(ReputationPallet::heartbeat(Origin::signed(BOB)));
		assert_ok!(ReputationPallet::heartbeat(Origin::signed(ALICE)));

		let current_epoch = MockEpochInfo::epoch_index();
		println!("The current epoch is: {}", current_epoch);
		assert!(
			!MockEpochInfo::authority_index(current_epoch, &BOB).is_some(),
			"Bob should not be an authority"
		);

		assert!(
			MockEpochInfo::authority_index(current_epoch, &ALICE).is_some(),
			"Alice should be an authority"
		);

		assert!(ReputationPallet::is_qualified(&BOB), "Bob should be online");

		assert!(ReputationPallet::is_qualified(&ALICE), "Alice should be online");

		advance_by_hearbeat_intervals(3);

		assert!(!ReputationPallet::is_qualified(&BOB), "Bob should be offline");

		assert!(!ReputationPallet::is_qualified(&ALICE), "Alice should be offline");

		assert!(
			ReputationPallet::current_network_state().online.is_empty(),
			"Alice is now not online"
		);

		assert_eq!(
			ReputationPallet::current_network_state().number_of_nodes(),
			1,
			"We should have one node"
		);
	});
}
