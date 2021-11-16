#[macro_export]
macro_rules! impl_mock_restriction_handler {
	($account_id:ty, $call:ty) => {
		pub struct RestrictionHandlerMock;

		impl GovernanceRestriction for RestrictionHandlerMock {
			type AccountId = $account_id;
			type Call = $call;
			fn is_member(account_id: &Self::AccountId) -> bool {
				false
			}
			fn is_gov_call(call: &Self::Call) -> bool {
				false
			}
		}
	};
}
