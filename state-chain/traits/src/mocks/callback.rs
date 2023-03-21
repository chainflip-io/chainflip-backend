#[macro_export]
macro_rules! impl_mock_callback {
	($runtime_origin:ty) => {
		pub struct MockCallback;

		impl UnfilteredDispatchable for MockCallback {
			type RuntimeOrigin = $runtime_origin;

			fn dispatch_bypass_filter(
				self,
				_origin: Self::RuntimeOrigin,
			) -> frame_support::dispatch::DispatchResultWithPostInfo {
				Ok(().into())
			}
		}
	};
}
