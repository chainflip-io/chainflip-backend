#[macro_export]
macro_rules! impl_mock_waived_fees {
	($account_id:ty, $call:ty) => {
		pub struct WaivedFeesMock;

		impl WaivedFees for WaivedFeesMock {
			type AccountId = $account_id;
			type RuntimeCall = $call;
			fn should_waive_fees(call: &Self::RuntimeCall, caller: &Self::AccountId) -> bool {
				false
			}
		}
	};
}
