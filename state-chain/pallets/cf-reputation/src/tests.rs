use frame_support::{assert_noop, assert_ok, traits::OnInitialize};

use cf_traits::{
	mocks::account_role_registry::MockAccountRoleRegistry, offence_reporting::*,
	AccountRoleRegistry, EpochInfo, QualifyNode, SafeMode, SetSafeMode,
};

use crate::{mock::*, *};

fn reputation_points(who: &<Test as frame_system::Config>::AccountId) -> ReputationPoints {
	ReputationPallet::reputation(who).reputation_points
}

pub fn advance_by_heartbeat_intervals(n: u64) {
	for _ in 0..n * HeartbeatBlockInterval::get() {
		advance_by_block();
	}
}

fn advance_by_block() {
	let next_block = System::block_number() + 1;
	System::set_block_number(next_block);
	AllPalletsWithoutSystem::on_initialize(next_block);
}

// Move forward one heartbeat interval sending the heartbeat extrinsic for nodes
fn move_forward_heartbeat_interval_and_submit_heartbeat(
	node: <Test as frame_system::Config>::AccountId,
) {
	advance_by_heartbeat_intervals(1);
	assert_ok!(ReputationPallet::heartbeat(RuntimeOrigin::signed(node)));
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

macro_rules! assert_reputation {
	( $id:expr, $rep:expr ) => {
		assert_eq!(
			reputation_points(&$id),
			$rep,
			"Expected reputation of {}, got {:?}",
			$rep,
			ReputationPallet::reputation(&$id)
		);
	};
}

#[test]
fn number_of_submissions_doesnt_affect_reputation_increase() {
	new_test_ext().execute_with(|| {
		// Disable reporting to prevent reputation points from being deducted.
		<MockRuntimeSafeMode as SetSafeMode<PalletSafeMode>>::set_code_red();
		assert_reputation!(ALICE, 0);

		// Submit twice per block.
		for _ in 0..HEARTBEAT_BLOCK_INTERVAL {
			advance_by_block();
			ReputationPallet::heartbeat(RuntimeOrigin::signed(ALICE)).unwrap();
			ReputationPallet::heartbeat(RuntimeOrigin::signed(ALICE)).unwrap();
		}
		assert_reputation!(ALICE, REPUTATION_PER_HEARTBEAT);

		// Submit every other block.
		Pallet::<Test>::reset_reputation(&ALICE);
		for i in 0..HEARTBEAT_BLOCK_INTERVAL {
			advance_by_block();
			if i % 2 == 0 {
				ReputationPallet::heartbeat(RuntimeOrigin::signed(ALICE)).unwrap();
			}
		}
		ReputationPallet::heartbeat(RuntimeOrigin::signed(ALICE)).unwrap();
		assert_reputation!(ALICE, REPUTATION_PER_HEARTBEAT);

		// Submit after the heartbeat interval has elapsed.
		Pallet::<Test>::reset_reputation(&ALICE);
		for _ in 0..(HEARTBEAT_BLOCK_INTERVAL * 2) {
			advance_by_block();
		}
		ReputationPallet::heartbeat(RuntimeOrigin::signed(ALICE)).unwrap();
		assert_reputation!(ALICE, REPUTATION_PER_HEARTBEAT);
	});
}

#[test]
fn update_last_heartbeat_each_submission() {
	new_test_ext().execute_with(|| {
		ReputationPallet::heartbeat(RuntimeOrigin::signed(ALICE)).unwrap();
		assert_eq!(ReputationPallet::last_heartbeat(ALICE).unwrap(), 1);
		advance_by_block();
		ReputationPallet::heartbeat(RuntimeOrigin::signed(ALICE)).unwrap();
		assert_eq!(ReputationPallet::last_heartbeat(ALICE).unwrap(), 2);
	});
}

#[test]
fn updating_accrual_rate_should_affect_reputation_points() {
	new_test_ext().execute_with(|| {
		// Disable reporting to prevent reputation points from being deducted.
		<MockRuntimeSafeMode as SetSafeMode<PalletSafeMode>>::set_code_red();
		// Fails due to too high a reputation points
		assert_noop!(
			ReputationPallet::update_accrual_ratio(
				RuntimeOrigin::root(),
				MAX_ACCRUABLE_REPUTATION + 1,
				20
			),
			Error::<Test>::InvalidAccrualRatio,
		);

		// Fails due to online points not being > 0
		assert_noop!(
			ReputationPallet::update_accrual_ratio(
				RuntimeOrigin::root(),
				MAX_ACCRUABLE_REPUTATION,
				0
			),
			Error::<Test>::InvalidAccrualRatio,
		);

		assert_ok!(ReputationPallet::update_accrual_ratio(
			RuntimeOrigin::root(),
			ACCRUAL_RATIO.0,
			ACCRUAL_RATIO.1,
		));

		assert_eq!(ReputationPallet::accrual_ratio(), ACCRUAL_RATIO);

		move_forward_heartbeat_interval_and_submit_heartbeat(ALICE);
		assert_reputation!(ALICE, REPUTATION_PER_HEARTBEAT);

		// Double the accrual rate.
		assert_ok!(ReputationPallet::update_accrual_ratio(
			RuntimeOrigin::root(),
			ACCRUAL_RATIO.0 * 2,
			ACCRUAL_RATIO.1,
		));

		move_forward_heartbeat_interval_and_submit_heartbeat(ALICE);
		assert_reputation!(ALICE, REPUTATION_PER_HEARTBEAT * 3);

		// Halve the divisor, equivalent to double the initial rate.
		assert_ok!(ReputationPallet::update_accrual_ratio(
			RuntimeOrigin::root(),
			ACCRUAL_RATIO.0,
			ACCRUAL_RATIO.1 / 2,
		));

		move_forward_heartbeat_interval_and_submit_heartbeat(ALICE);
		assert_reputation!(ALICE, REPUTATION_PER_HEARTBEAT * 5);
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
		let offline_test = |offence: AllOffences, who: Vec<u64>| {
			let penalty = ReputationPallet::resolve_penalty_for(offence);
			let points_before = who.clone().iter().map(reputation_points).collect::<Vec<_>>();
			<ReputationPallet as OffenceReporter>::report_many(offence, who.clone());
			for (id, points) in who.clone().into_iter().zip(points_before) {
				assert_reputation!(id, points - penalty.reputation);
			}
			assert_eq!(
				ReputationPallet::validators_suspended_for(&[offence]),
				if !penalty.suspension.is_zero() {
					who.into_iter().collect::<BTreeSet<_>>()
				} else {
					BTreeSet::default()
				}
			);
		};
		offline_test(AllOffences::MissedHeartbeat, vec![ALICE]);
		offline_test(AllOffences::ForgettingYourYubiKey, vec![ALICE, BOB]);
		offline_test(AllOffences::NotLockingYourComputer, vec![BOB]);

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
		const SUSPENSION_DURATION: u64 = 10;
		let first_suspend = [1, 2, 3];
		ReputationPallet::suspend_all(
			first_suspend,
			&AllOffences::ForgettingYourYubiKey,
			SUSPENSION_DURATION,
		);
		assert_eq!(
			ReputationPallet::validators_suspended_for(&[AllOffences::ForgettingYourYubiKey,]),
			first_suspend.into_iter().collect(),
		);

		advance_by_block();

		// overlapping suspensions, 1 not included
		let second_suspend = [2, 3, 4, 6];
		ReputationPallet::suspend_all(
			second_suspend,
			&AllOffences::ForgettingYourYubiKey,
			SUSPENSION_DURATION,
		);
		assert_eq!(
			ReputationPallet::validators_suspended_for(&[AllOffences::ForgettingYourYubiKey,]),
			[1, 2, 3, 4, 6].into_iter().collect(),
		);

		for _ in 0..SUSPENSION_DURATION {
			advance_by_block();
		}

		// 1 was not re-suspended, so they have served their time. Only the second suspended remain.
		assert_eq!(
			ReputationPallet::validators_suspended_for(&[AllOffences::ForgettingYourYubiKey,]),
			second_suspend.into_iter().collect(),
		);

		advance_by_block();

		assert!(ReputationPallet::validators_suspended_for(&[AllOffences::ForgettingYourYubiKey,])
			.is_empty());
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
		ReputationPallet::suspend_all([1, 2, 3], &AllOffences::ForgettingYourYubiKey, 10);
		ReputationPallet::suspend_all([1, 2], &AllOffences::NotLockingYourComputer, u64::MAX);
		ReputationPallet::suspend_all([1], &AllOffences::MissedHeartbeat, 15);
		assert_eq!(
			Pallet::<Test>::validators_suspended_for(AllOffences::OFFENCES),
			[1, 2, 3].into_iter().collect(),
		);
		<ReputationPallet as OffenceReporter>::forgive_all(AllOffences::ForgettingYourYubiKey);
		assert_eq!(
			Pallet::<Test>::validators_suspended_for(AllOffences::OFFENCES),
			[1, 2].into_iter().collect(),
		);
		<ReputationPallet as OffenceReporter>::forgive_all(AllOffences::NotLockingYourComputer);
		assert_eq!(
			Pallet::<Test>::validators_suspended_for(AllOffences::OFFENCES),
			[1].into_iter().collect(),
		);
		<ReputationPallet as OffenceReporter>::forgive_all(PalletOffence::MissedHeartbeat);
		assert_eq!(
			Pallet::<Test>::validators_suspended_for(AllOffences::OFFENCES),
			[].into_iter().collect(),
		);
	});
}

#[test]
fn dont_report_in_safe_mode() {
	new_test_ext().execute_with(|| {
		let marcello = 1;
		MockRuntimeSafeMode::set_safe_mode(MockRuntimeSafeMode {
			reputation: crate::PalletSafeMode { reporting_enabled: false },
		});
		ReputationPallet::report(AllOffences::NotLockingYourComputer, marcello);
		assert_reputation!(marcello, 0);
		MockRuntimeSafeMode::set_safe_mode(MockRuntimeSafeMode {
			reputation: crate::PalletSafeMode { reporting_enabled: true },
		});
		ReputationPallet::report(AllOffences::NotLockingYourComputer, marcello);
		assert!(ReputationPallet::reputation(marcello).reputation_points < 0);
	});
}

#[test]
fn heartbeats_emitted_in_safe_mode() {
	new_test_ext().execute_with(|| {
		assert_eq!(MockHeartbeat::heartbeats(), 0);

		// enable safe mode (disable reporting)
		MockRuntimeSafeMode::set_safe_mode(MockRuntimeSafeMode {
			reputation: crate::PalletSafeMode::CODE_RED,
		});
		advance_by_heartbeat_intervals(1);

		assert_eq!(MockHeartbeat::heartbeats(), 1);
	});
}

#[cfg(test)]
mod reporting_adapter_test {
	use frame_support::assert_err;
	use pallet_grandpa::{
		EquivocationOffence as GrandpaEquivocationOffence, TimeSlot as GrandpaTimeSlot,
	};
	use sp_staking::offence::ReportOffence;

	use super::*;

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
			assert!(Pallet::<Test>::validators_suspended_for(&[AllOffences::UpsettingGrandpa])
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
				Pallet::<Test>::validators_suspended_for(&[AllOffences::UpsettingGrandpa]),
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
		assert_ok!(ReputationPallet::heartbeat(RuntimeOrigin::signed(ALICE)));
		assert!(HeartbeatQualification::<Test>::is_qualified(&ALICE), "Alice should be online");
		assert_ok!(ReputationPallet::heartbeat(RuntimeOrigin::signed(ALICE)));
		assert!(HeartbeatQualification::<Test>::is_qualified(&ALICE), "Alice should be online");
		advance_by_heartbeat_intervals(1);
		assert_ok!(ReputationPallet::heartbeat(RuntimeOrigin::signed(ALICE)));
		assert!(HeartbeatQualification::<Test>::is_qualified(&ALICE), "Alice should be online");
	});
}

#[test]
fn we_should_see_missing_nodes_when_not_having_submitted_one_interval() {
	new_test_ext().execute_with(|| {
		assert_ok!(ReputationPallet::heartbeat(RuntimeOrigin::signed(ALICE)));
		assert!(HeartbeatQualification::<Test>::is_qualified(&ALICE), "Alice should be online");
		advance_by_heartbeat_intervals(1);
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
		assert_ok!(ReputationPallet::heartbeat(RuntimeOrigin::signed(ALICE)));
		assert_eq!(
			ReputationPallet::current_network_state().online,
			vec![ALICE],
			"Alice should be online after submitting a heartbeat"
		);
	});
}

#[test]
fn only_authorities_should_appear_in_network_state() {
	new_test_ext().execute_with(|| {
		assert_ok!(ReputationPallet::heartbeat(RuntimeOrigin::signed(BOB)));
		assert_ok!(ReputationPallet::heartbeat(RuntimeOrigin::signed(ALICE)));

		let current_epoch = MockEpochInfo::epoch_index();
		println!("The current epoch is: {current_epoch}");
		assert!(
			MockEpochInfo::authority_index(current_epoch, &BOB).is_none(),
			"Bob should not be an authority"
		);

		assert!(
			MockEpochInfo::authority_index(current_epoch, &ALICE).is_some(),
			"Alice should be an authority"
		);

		assert!(HeartbeatQualification::<Test>::is_qualified(&BOB), "Bob should be online");

		assert!(HeartbeatQualification::<Test>::is_qualified(&ALICE), "Alice should be online");

		advance_by_heartbeat_intervals(3);

		assert!(!HeartbeatQualification::<Test>::is_qualified(&BOB), "Bob should be offline");

		assert!(!HeartbeatQualification::<Test>::is_qualified(&ALICE), "Alice should be offline");

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

#[test]
fn in_safe_mode_you_dont_lose_reputation_for_being_offline() {
	new_test_ext().execute_with(|| {
		assert_ok!(ReputationPallet::heartbeat(RuntimeOrigin::signed(BOB)));
		assert!(HeartbeatQualification::<Test>::is_qualified(&BOB));
		let reputation = reputation_points(&BOB);
		MockRuntimeSafeMode::set_safe_mode(MockRuntimeSafeMode {
			reputation: PalletSafeMode { reporting_enabled: false },
		});
		advance_by_heartbeat_intervals(3);
		assert!(!HeartbeatQualification::<Test>::is_qualified(&BOB));
		assert_eq!(reputation, reputation_points(&BOB));
	});
}

#[test]
fn should_properly_check_if_validator_is_qualified() {
	new_test_ext().execute_with(|| {
		const EVE: u64 = 101;
		assert_ok!(<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_validator(
			&EVE
		));

		let test_set = BTreeSet::from_iter([ALICE, BOB, EVE]);

		// single validator is always qualified
		ReputationPallet::heartbeat(RuntimeOrigin::signed(ALICE)).unwrap();
		assert!(ReputationPointsQualification::<Test>::is_qualified(&ALICE));
		assert_eq!(
			ReputationPointsQualification::<Test>::filter_qualified(iter::once(ALICE).collect()),
			iter::once(ALICE).collect()
		);

		// test when network has 3 validators
		for id in &test_set {
			ReputationPallet::heartbeat(RuntimeOrigin::signed(*id)).unwrap();
			assert!(ReputationPointsQualification::<Test>::is_qualified(id));
		}
		assert_eq!(
			ReputationPointsQualification::<Test>::filter_qualified(test_set.clone()),
			test_set
		);

		// If there are 3 validators and 33rd percentile validator has  lower reputation than the
		// other (so 1 validator has worse reputation than the other 2, they should not be
		// considered qualified)
		ReputationPallet::penalise_offline_authorities(vec![BOB]);
		assert!(reputation_points(&BOB) < 0);
		assert!(!ReputationPointsQualification::<Test>::is_qualified(&BOB));

		// If reputation of other validators is reduced further, make sure that
		// we don't unnecessarily disqualify validators
		ReputationPallet::penalise_offline_authorities(vec![EVE]);
		assert!(reputation_points(&EVE) < 0);
		assert!(reputation_points(&ALICE) >= 0);
		for id in &test_set {
			assert!(ReputationPointsQualification::<Test>::is_qualified(id));
		}
		assert_eq!(
			ReputationPointsQualification::<Test>::filter_qualified(test_set.clone()),
			test_set
		);

		// Check that updating reputations properly calculates qualifications
		move_forward_heartbeat_interval_and_submit_heartbeat(ALICE);
		move_forward_heartbeat_interval_and_submit_heartbeat(BOB);

		assert!(!ReputationPointsQualification::<Test>::is_qualified(&EVE));
		assert_eq!(
			ReputationPointsQualification::<Test>::filter_qualified(test_set.clone()),
			test_set.into_iter().filter(|id| *id != EVE).collect()
		);

		// Test with a bunch of validators
		let mut test_set =
			(300..500u64)
				.inspect(|id| {
					assert_ok!(
						<MockAccountRoleRegistry as AccountRoleRegistry<Test>>::register_as_validator(id)
					);
					ReputationPallet::heartbeat(RuntimeOrigin::signed(*id)).unwrap();
					assert!(ReputationPointsQualification::<Test>::is_qualified(id));
				})
				.collect::<Vec<_>>();

		// just so that ALICE, BOB and EVE are at the end to make testing bit easier
		test_set.extend([ALICE, BOB, EVE]);

		let (first_third, rest) = test_set.split_at(test_set.len() / 3);

		// make one third of validators misbehave
		ReputationPallet::penalise_offline_authorities(first_third.to_vec());
		// Even though validators misbehaved because previously they sent heartbeat and received
		// reputation points they are still above 0, so they are all qualified
		let current_reputation_of_third_of_validators =
			REPUTATION_PER_HEARTBEAT - MISSED_HEARTBEAT_PENALTY_POINTS;

		for id in first_third {
			assert!(ReputationPointsQualification::<Test>::is_qualified(id));
			assert_eq!(reputation_points(id), current_reputation_of_third_of_validators);
		}

		// Except poor Eve, they are still disqualified
		assert!(reputation_points(&EVE) < 0);
		assert!(!ReputationPointsQualification::<Test>::is_qualified(&EVE));
		assert_eq!(
			ReputationPointsQualification::<Test>::filter_qualified(
				test_set.iter().copied().collect()
			),
			test_set.iter().copied().filter(|id| *id != EVE).collect()
		);

		// Set reputation to negative for 1/3 of validators
		let bad_rep = Reputations::<Test>::get(EVE).reputation_points - 1;
		for id in first_third {
			Reputations::<Test>::mutate(id, |reputation| {
				reputation.reputation_points = bad_rep;
			});
		}

		// Now the first third of validators will not be qualified anymore but the rest are,
		// including EVE.
		for id in first_third {
			assert!(!ReputationPointsQualification::<Test>::is_qualified(id));
		}
		for id in rest {
			assert!(ReputationPointsQualification::<Test>::is_qualified(id));
		}
		assert_eq!(
			ReputationPointsQualification::<Test>::filter_qualified(
				test_set.iter().copied().collect()
			),
			rest.iter().copied().collect()
		);
	});
}

#[test]
fn reputation_cutoff_threshold() {
	assert_eq!(
		ReputationPointsQualification::<Test>::reputation_qualification_cutoff(vec![
			-1, -1, -1, 0, 1, 1,
		]),
		-1
	);
	assert_eq!(
		ReputationPointsQualification::<Test>::reputation_qualification_cutoff(vec![
			-1, -1, 0, 1, 1,
		]),
		-1
	);
	assert_eq!(
		ReputationPointsQualification::<Test>::reputation_qualification_cutoff(vec![
			-1, -1, 0, 1, 1, 1
		]),
		0
	);
	assert_eq!(
		ReputationPointsQualification::<Test>::reputation_qualification_cutoff(vec![
			-1, -1, 0, 1, 1, 1, 1
		]),
		0
	);
	assert_eq!(
		ReputationPointsQualification::<Test>::reputation_qualification_cutoff(vec![
			-1, -1, 0, 1, 1, 1, 1, 1
		]),
		0
	);
	assert_eq!(
		ReputationPointsQualification::<Test>::reputation_qualification_cutoff(vec![
			-1, -1, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1
		]),
		0
	);
}

#[test]
fn ensure_governance_origin_checks() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			ReputationPallet::update_accrual_ratio(RuntimeOrigin::signed(ALICE), 1, 1),
			sp_runtime::traits::BadOrigin,
		);
		assert_noop!(
			ReputationPallet::set_penalty(
				RuntimeOrigin::signed(ALICE),
				AllOffences::MissedHeartbeat,
				Penalty::<Test> { reputation: 1, suspension: 1 }
			),
			sp_runtime::traits::BadOrigin,
		);
		assert_noop!(
			ReputationPallet::update_missed_heartbeat_penalty(RuntimeOrigin::signed(ALICE), 1),
			sp_runtime::traits::BadOrigin,
		);
	});
}
