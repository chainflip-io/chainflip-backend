#[macro_export]
macro_rules! impl_mock_offline_conditions {
	($account_id:ty) => {
		thread_local! {
			pub static REPORTED: std::cell::RefCell<Vec<$account_id>> = Default::default()
		}

		pub struct MockOfflineConditions;

		impl MockOfflineConditions {
			pub fn get_reported() -> Vec<$account_id> {
				REPORTED.with(|cell| cell.borrow().clone())
			}
		}

		impl $crate::offline_conditions::OfflineConditions for MockOfflineConditions {
			type ValidatorId = $account_id;

			fn report(
				_condition: $crate::offline_conditions::OfflineCondition,
				_penalty: $crate::offline_conditions::ReputationPoints,
				validator_id: &Self::ValidatorId,
			) -> Result<frame_support::dispatch::Weight, $crate::offline_conditions::ReportError> {
				REPORTED.with(|cell| cell.borrow_mut().push(validator_id.clone()));
				Ok(0)
			}
		}
	};
}
