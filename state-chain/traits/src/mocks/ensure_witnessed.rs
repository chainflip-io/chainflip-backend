#[macro_export]
macro_rules! impl_mock_ensure_witnessed_for_origin {
	($origin:ty) => {
		pub struct MockEnsureWitnessed;

		impl frame_support::traits::EnsureOrigin<$origin> for MockEnsureWitnessed {
			type Success = ();

					fn try_origin(o: $origin) -> Result<Self::Success, $origin> {
				frame_system::ensure_root(o).or(Err(frame_system::RawOrigin::None.into()))
			}
		}
	};
}
