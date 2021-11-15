#[macro_export]
macro_rules! impl_mock_restriction_handler {
	($account_id:ty, $call:ty) => {
		pub struct RestrictionHandlerMock;

		impl GovernanceRestriction for RestrictionHandlerMock {
			type AccountId = $account_id;
			type Call = $call;

			fn is_whitelisted(_call: &Self::Call, _account_id: &Self::AccountId) -> bool {
				false
			}
		}
	};
}
