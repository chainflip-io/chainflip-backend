use crate::{mock::*, *};
use cf_traits::{
	mocks::epoch_info::Mock, offence_reporting::*, EpochInfo, Heartbeat, NetworkState,
};
use frame_support::{assert_noop, assert_ok, assert_storage_noop};
use sp_runtime::BuildStorage;
use sp_std::{cmp::max, ops::Neg};

fn last_event() -> mock::Event {
	frame_system::Pallet::<Test>::events().pop().expect("Event expected").event
}

fn reputation_points(who: <Test as frame_system::Config>::AccountId) -> ReputationPoints {
	ReputationPallet::reputation(who).reputation_points
}

// Cycle heartbeat interval sending the heartbeat extrinsic in each
fn run_heartbeat_intervals(
	validators: Vec<<Test as frame_system::Config>::AccountId>,
	intervals: u64,
) {
	for interval in 1..=intervals {
		for validator_id in &validators {
			<ReputationPallet as Heartbeat>::heartbeat_submitted(
				validator_id,
				interval * HEARTBEAT_BLOCK_INTERVAL,
			);
		}
	}
}

// We will need to send heartbeats for the next ACCRUAL_BLOCKS_PER_REPUTATION_POINT blocks
fn submit_heartbeats_for_accrual_blocks(
	validator: <Test as frame_system::Config>::AccountId,
	number_of_accruals: u64,
) {
	let intervals = ACCRUAL_BLOCKS * number_of_accruals / HeartbeatBlockInterval::get();
	run_heartbeat_intervals(vec![validator], intervals + 1 /* roundup */);
}

type MockNetworkState = NetworkState<<Test as frame_system::Config>::AccountId>;
fn dead_network() -> MockNetworkState {
	MockNetworkState { offline: Mock::current_validators(), ..MockNetworkState::default() }
}

fn live_network() -> MockNetworkState {
	MockNetworkState { online: Mock::current_validators(), ..MockNetworkState::default() }
}

// Move a heartbeat interval forward with no heartbeat sent
fn dead_network_for_intervals(intervals: u64) {
	for _ in 0..intervals {
		<ReputationPallet as Heartbeat>::on_heartbeat_interval(dead_network());
	}
}

#[test]
#[should_panic]
fn should_panic_if_accrual_rate_is_less_than_heartbeat_interval_at_genesis() {
	mock::GenesisConfig {
		system: Default::default(),
		reputation_pallet: ReputationPalletConfig {
			accrual_ratio: (1, HEARTBEAT_BLOCK_INTERVAL - 1),
		},
	}
	.build_storage()
	.unwrap();
}

#[test]
fn submitting_heartbeat_should_reward_reputation_points() {
	new_test_ext().execute_with(|| {
		let number_of_accruals = 10;
		submit_heartbeats_for_accrual_blocks(ALICE, number_of_accruals);
		// Alice should now have 10 points
		assert_eq!(reputation_points(ALICE), number_of_accruals as i32 * ACCRUAL_POINTS);
	});
}

#[test]
fn missing_heartbeats_should_see_loss_of_reputation_points() {
	new_test_ext().execute_with(|| {
		assert_eq!(reputation_points(ALICE), 0);
		// We will need to send heartbeats for the next ACCRUAL_BLOCKS_PER_REPUTATION_POINT
		// blocks
		submit_heartbeats_for_accrual_blocks(ALICE, 1);
		// Alice should now have 1 point
		let current_reputation = reputation_points(ALICE);
		assert_eq!(current_reputation, ACCRUAL_POINTS);
		let heartbeats = 100;
		dead_network_for_intervals(heartbeats);
		assert_eq!(
			reputation_points(ALICE),
			current_reputation -
				(heartbeats as u32 *
					POINTS_PER_BLOCK_PENALTY.points as u32 *
					HEARTBEAT_BLOCK_INTERVAL as u32 /
					POINTS_PER_BLOCK_PENALTY.blocks as u32) as ReputationPoints
		);
	});
}

#[test]
fn missing_heartbeats_should_see_slashing_when_we_hit_negative() {
	new_test_ext().execute_with(|| {
		assert_eq!(reputation_points(ALICE), 0);
		let expected_slashes = 10;
		dead_network_for_intervals(expected_slashes);
		assert_eq!(SLASH_COUNT.with(|count| *count.borrow()), expected_slashes);
	});
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
			Error::<Test>::InvalidAccrualReputationPoints
		);

		assert_noop!(
			ReputationPallet::update_accrual_ratio(Origin::root(), MAX_REPUTATION_POINT_ACCRUED, 0),
			Error::<Test>::InvalidAccrualOnlineCredits
		);

		assert_ok!(ReputationPallet::update_accrual_ratio(
			Origin::root(),
			ACCRUAL_POINTS,
			ACCRUAL_BLOCKS
		));

		assert_eq!(ReputationPallet::accrual_ratio(), (ACCRUAL_POINTS, ACCRUAL_BLOCKS));

		assert_eq!(
			last_event(),
			mock::Event::ReputationPallet(crate::Event::AccrualRateUpdated(
				ACCRUAL_POINTS,
				ACCRUAL_BLOCKS
			))
		);

		let number_of_accruals = 2;
		submit_heartbeats_for_accrual_blocks(ALICE, number_of_accruals);
		assert_eq!(reputation_points(ALICE), ACCRUAL_POINTS * number_of_accruals as i32);
	});
}

#[test]
fn missing_a_heartbeat_submission_should_penalise_reputation_points() {
	new_test_ext().execute_with(|| {
		let ReputationPenalty { points, blocks } = POINTS_PER_BLOCK_PENALTY;
		// We are starting out with zero points
		assert_eq!(reputation_points(ALICE), 0);
		// Interval 1 - with no heartbeat we will lose `points` per `block`
		<ReputationPallet as Heartbeat>::on_heartbeat_interval(dead_network());
		assert_eq!(
			reputation_points(ALICE),
			(HEARTBEAT_BLOCK_INTERVAL as i32 / blocks as i32 * points as i32).neg()
		);
		// Interval 2 - with no heartbeat this will continue
		<ReputationPallet as Heartbeat>::on_heartbeat_interval(dead_network());
		assert_eq!(
			reputation_points(ALICE),
			2 * (HEARTBEAT_BLOCK_INTERVAL as i32 / blocks as i32 * points as i32).neg()
		);
	});
}

#[test]
fn should_not_be_penalised_if_submitted_heartbeat() {
	new_test_ext().execute_with(|| {
		assert_eq!(reputation_points(ALICE), 0);
		<ReputationPallet as Heartbeat>::on_heartbeat_interval(live_network());
		assert_eq!(reputation_points(ALICE), 0);
	});
}

#[test]
fn reporting_any_offence_for_unknown_validator_should_produce_error() {
	new_test_ext().execute_with(|| {
		assert_storage_noop!(ReputationPallet::report(Offence::ParticipateSigningFailed, &BOB));
		assert_storage_noop!(ReputationPallet::report(Offence::ParticipateKeygenFailed, &BOB));
		assert_storage_noop!(ReputationPallet::report(
			Offence::TransactionFailedOnTransmission,
			&BOB
		));
		assert_storage_noop!(ReputationPallet::report(Offence::InvalidTransactionAuthored, &BOB));
	});
}

#[test]
fn reporting_any_offence_for_known_validator_without_reputation_recorded_should_produce_error() {
	new_test_ext().execute_with(|| {
		assert_storage_noop!(ReputationPallet::report(Offence::ParticipateSigningFailed, &ALICE));
		assert_storage_noop!(ReputationPallet::report(Offence::ParticipateKeygenFailed, &ALICE));
		assert_storage_noop!(ReputationPallet::report(
			Offence::TransactionFailedOnTransmission,
			&ALICE
		));
		assert_storage_noop!(ReputationPallet::report(Offence::InvalidTransactionAuthored, &ALICE));
	});
}

#[test]
fn reporting_any_offence_should_penalise_reputation_points() {
	new_test_ext().execute_with(|| {
		<ReputationPallet as Heartbeat>::on_heartbeat_interval(dead_network());
		let offline_test = |offence: Offence, who: <Test as frame_system::Config>::AccountId| {
			let (penalty, _) = MockOffencePenalty::penalty(&offence);
			let points_before = reputation_points(who);
			ReputationPallet::report(offence.clone(), &who);
			assert_eq!(
				reputation_points(who),
				max(points_before - penalty, ReputationPointFloorAndCeiling::get().0)
			);
			assert_eq!(
				last_event(),
				mock::Event::ReputationPallet(crate::Event::OffencePenalty(who, offence, penalty))
			);
		};
		<ReputationPallet as Heartbeat>::on_heartbeat_interval(dead_network());
		offline_test(Offence::ParticipateSigningFailed, ALICE);
		offline_test(Offence::ParticipateKeygenFailed, ALICE);
		offline_test(Offence::InvalidTransactionAuthored, ALICE);
		offline_test(Offence::TransactionFailedOnTransmission, ALICE);
	});
}

#[test]
fn reporting_participate_in_signing_offence_should_penalise_reputation_points() {
	new_test_ext().execute_with(|| {
		<ReputationPallet as Heartbeat>::heartbeat_submitted(&ALICE, 1);
		let points_before = reputation_points(ALICE);
		ReputationPallet::report(Offence::ParticipateSigningFailed, &ALICE);
		let (penalty, _) = MockOffencePenalty::penalty(&Offence::ParticipateSigningFailed);
		assert_eq!(reputation_points(ALICE), points_before - penalty);
		assert_eq!(
			last_event(),
			mock::Event::ReputationPallet(crate::Event::OffencePenalty(
				ALICE,
				Offence::ParticipateSigningFailed,
				penalty
			))
		);
	});
}

#[test]
fn reporting_participate_in_signing_offence_should_also_ban_validator() {
	new_test_ext().execute_with(|| {
		// Confirm a ban is called for this validator for this condition
		<ReputationPallet as Heartbeat>::heartbeat_submitted(&ALICE, 1);
		ReputationPallet::report(Offence::ParticipateSigningFailed, &ALICE);

		assert!(MockBanned::is_banned(&ALICE));
	});
}

#[test]
fn reporting_invalid_transaction_authored_offence_should_not_ban_validator() {
	new_test_ext().execute_with(|| {
		// We do not ban validators for authoring an invalid transaction
		<ReputationPallet as Heartbeat>::heartbeat_submitted(&ALICE, 1);
		ReputationPallet::report(Offence::InvalidTransactionAuthored, &ALICE);

		assert!(!MockBanned::is_banned(&ALICE));
	});
}
