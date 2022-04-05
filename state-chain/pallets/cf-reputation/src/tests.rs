use crate::{mock::*, *};
use cf_traits::{offence_reporting::*, Heartbeat, NetworkState};
use frame_support::{assert_noop, assert_ok};

fn reputation_points(who: &<Test as frame_system::Config>::AccountId) -> ReputationPoints {
	ReputationPallet::reputation(who).reputation_points
}

#[test]
fn submitting_heartbeat_should_reward_reputation_points() {
	new_test_ext().execute_with(|| {
		let ignored = 0;
		<ReputationPallet as Heartbeat>::heartbeat_submitted(&ALICE, ignored);
		assert_eq!(reputation_points(&ALICE), REPUTATION_PER_HEARTBEAT,);
	});
}

#[test]
fn missing_a_heartbeat_deducts_penalty_points() {
	new_test_ext().execute_with(|| {
		<ReputationPallet as Heartbeat>::on_heartbeat_interval(NetworkState {
			offline: vec![ALICE],
			..Default::default()
		});

		assert_eq!(reputation_points(&ALICE), -MISSED_HEARTBEAT_PENALTY_POINTS);
	});
}

#[test]
fn offline_nodes_get_slashed_if_reputation_is_negative() {
	new_test_ext().execute_with(|| {
		assert_eq!(reputation_points(&ALICE), 0);
		<ReputationPallet as Heartbeat>::on_heartbeat_interval(NetworkState {
			offline: vec![ALICE],
			..Default::default()
		});
		assert_eq!(SLASH_COUNT.with(|count| *count.borrow()), 1);
	});
}

#[test]
fn updating_accrual_rate_should_affect_reputation_points() {
	new_test_ext().execute_with(|| {
		let ignored = 0;
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

		<ReputationPallet as Heartbeat>::heartbeat_submitted(&ALICE, ignored);
		assert_eq!(reputation_points(&ALICE), REPUTATION_PER_HEARTBEAT);

		// Double the accrual rate.
		assert_ok!(ReputationPallet::update_accrual_ratio(
			Origin::root(),
			ACCRUAL_RATE.0 * 2,
			ACCRUAL_RATE.1,
		));

		<ReputationPallet as Heartbeat>::heartbeat_submitted(&ALICE, ignored);
		assert_eq!(reputation_points(&ALICE), REPUTATION_PER_HEARTBEAT * 3);

		// Halve the divisor, equivalent to double the initial rate.
		assert_ok!(ReputationPallet::update_accrual_ratio(
			Origin::root(),
			ACCRUAL_RATE.0,
			ACCRUAL_RATE.1 / 2,
		));

		<ReputationPallet as Heartbeat>::heartbeat_submitted(&ALICE, ignored);
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
			let points_before = who.iter().map(|id| reputation_points(id)).collect::<Vec<_>>();
			<ReputationPallet as OffenceReporter>::report_many(offence, who);
			for (id, points) in who.iter().zip(points_before) {
				assert_eq!(reputation_points(id), points - penalty.reputation,);
			}
			assert_eq!(
				ReputationPallet::validators_suspended_for::<BTreeSet<_>>(&[offence]),
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

		assert_eq!(
			ReputationPallet::validators_suspended_for::<BTreeSet<_>>(&[
				AllOffences::MissedHeartbeat,
				AllOffences::ForgettingYourYubiKey,
				AllOffences::NotLockingYourComputer
			]),
			[ALICE, BOB].iter().cloned().collect(),
		);
	});
}

#[test]
fn suspensions() {
	new_test_ext().execute_with(|| {
		ReputationPallet::suspend_all(&[1, 2, 3], &AllOffences::ForgettingYourYubiKey, 10);
		assert_eq!(
			ReputationPallet::validators_suspended_for::<BTreeSet<_>>(&[
				AllOffences::ForgettingYourYubiKey,
			]),
			[1, 2, 3].iter().cloned().collect(),
		);
	});
}

// #[test]
// fn reporting_participate_in_signing_offence_should_also_ban_validator() {
// 	new_test_ext().execute_with(|| {
// 		// Confirm a ban is called for this validator for this condition
// 		<ReputationPallet as Heartbeat>::heartbeat_submitted(&ALICE, 1);
// 		ReputationPallet::report(Offence::ParticipateSigningFailed, &ALICE);

// 		assert!(ReputationPallet::is_suspended_at(System::current_block_number(), &ALICE));
// 	});
// }

// #[test]
// fn reporting_invalid_transaction_authored_offence_should_not_ban_validator() {
// 	new_test_ext().execute_with(|| {
// 		// We do not ban validators for authoring an invalid transaction
// 		<ReputationPallet as Heartbeat>::heartbeat_submitted(&ALICE, 1);
// 		ReputationPallet::report(Offence::InvalidTransactionAuthored, &ALICE);

// 		assert!(!ReputationPallet::is_suspended_at(System::current_block_number(), &ALICE));
// 	});
// }
