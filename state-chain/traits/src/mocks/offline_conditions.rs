#[macro_export]
macro_rules! impl_mock_offline_conditions {
	($account_id:ty) => {
		thread_local! {
			pub static REPORTED: std::cell::RefCell<Vec<$account_id>> = Default::default();
			pub static BANNED_VALIDATORS: std::cell::RefCell<std::collections::HashMap<$account_id, ()>> = Default::default();
		}

		pub struct MockOfflineReporter;

		impl MockOfflineReporter {
			pub fn get_reported() -> Vec<$account_id> {
				REPORTED.with(|cell| cell.borrow().clone())
			}
		}

		pub struct MockOfflinePenalty;
		impl $crate::offline_conditions::OfflinePenalty for MockOfflinePenalty {
			fn penalty(
				condition: &$crate::offline_conditions::OfflineCondition,
			) -> $crate::offline_conditions::ReputationPoints {
				match condition {
					$crate::offline_conditions::OfflineCondition::BroadcastOutputFailed => 10,
					$crate::offline_conditions::OfflineCondition::ParticipateSigningFailed => 100,
					$crate::offline_conditions::OfflineCondition::NotEnoughPerformanceCredits => 1000,
				}
			}
		}

		pub struct MockBanned;
		impl $crate::offline_conditions::Banned for MockBanned {
			type ValidatorId = $account_id;
			fn ban(validator_id: &Self::ValidatorId) {
				BANNED_VALIDATORS.with(|cell| {
					(*(cell.borrow_mut())).insert(validator_id.clone(), ());
				});
			}
		}

		impl MockBanned {
			pub fn is_banned(validator_id: &$account_id) -> bool {
				BANNED_VALIDATORS.with(|cell| {
					(*(cell.borrow())).contains_key(validator_id)
				})
			}
		}

		impl $crate::offline_conditions::OfflineReporter for MockOfflineReporter {
			type ValidatorId = $account_id;
			type Penalty = MockOfflinePenalty;

			fn report(
				_condition: $crate::offline_conditions::OfflineCondition,
				validator_id: &Self::ValidatorId,
			) -> Result<frame_support::dispatch::Weight, $crate::offline_conditions::ReportError> {
				REPORTED.with(|cell| cell.borrow_mut().push(validator_id.clone()));
				Ok(0)
			}
		}
	};
}
