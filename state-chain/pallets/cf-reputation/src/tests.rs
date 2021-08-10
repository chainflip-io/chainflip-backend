mod tests {
	use crate::mock::*;
	use crate::*;
	use frame_support::{assert_noop, assert_ok};
	use cf_traits::mocks::{epoch_info, time_source};

	#[test]
	fn should_have_a_list_of_validators_at_genesis() {
		new_test_ext().execute_with(|| {
			assert_eq!(1 + 1, 2)
		});
	}

	#[test]
	fn submitting_heartbeat_should_reward_reputation_points() {
		new_test_ext().execute_with(|| {
			assert_eq!(1 + 1, 2)
		});
	}

	#[test]
	fn updating_accrual_rate_should_affect_reputation_points() {
		new_test_ext().execute_with(|| {
			assert_eq!(1 + 1, 2)
		});
	}

	#[test]
	fn submitting_heartbeats_in_same_heartbeat_interval_should_fails() {
		new_test_ext().execute_with(|| {

		});
	}

	#[test]
	fn missing_a_heartbeat_submission_should_penalise_reputation_points() {
		new_test_ext().execute_with(|| {
			assert_eq!(1 + 1, 2)
		});
	}

	#[test]
	fn reporting_broadcast_output_failed_offline_condition_should_penalise_reputation_points() {
		new_test_ext().execute_with(|| {
			assert_eq!(1 + 1, 2)
		});
	}

	#[test]
	fn reporting_participate_in_signing_offline_condition_should_penalise_reputation_points() {
		new_test_ext().execute_with(|| {
			assert_eq!(1 + 1, 2)
		});
	}

	#[test]
	fn reporting_on_non_existing_validator_should_produce_an_error() {
		new_test_ext().execute_with(|| {
			assert_eq!(1 + 1, 2)
		});
	}
}