#[macro_export]
macro_rules! impl_mock_never_failing_origin_check {
	($origin:ty) => {
		pub struct NeverFailingOriginCheck;

		impl frame_support::traits::EnsureOrigin<$origin> for NeverFailingOriginCheck {
			type Success = ();

			fn try_origin(_o: $origin) -> Result<Self::Success, $origin> {
				Ok(())
			}
		}
	};
}
