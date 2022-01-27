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
			) -> ($crate::offline_conditions::ReputationPoints, bool) {
				match condition {
					$crate::offline_conditions::OfflineCondition::ParticipateSigningFailed => (15, true),
					$crate::offline_conditions::OfflineCondition::ParticipateKeygenFailed => (15, true),
					$crate::offline_conditions::OfflineCondition::InvalidTransactionAuthored => (15, false),
					$crate::offline_conditions::OfflineCondition::TransactionFailedOnTransmission => (15, false),
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
			) -> frame_support::dispatch::Weight {
				REPORTED.with(|cell| cell.borrow_mut().push(validator_id.clone()));
				0
			}
		}
	};
}
