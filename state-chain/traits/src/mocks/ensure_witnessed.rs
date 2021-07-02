#[macro_export] macro_rules! impl_mock_ensure_witnessed_for_origin {
	($origin:ty) => {
		use frame_support::traits::EnsureOrigin;
		use frame_system::{ensure_root, RawOrigin};

		pub struct MockEnsureWitnessed;

		impl EnsureOrigin<$origin> for MockEnsureWitnessed {
			type Success = ();

			fn try_origin(o: $origin) -> Result<Self::Success, $origin> {
				ensure_root(o).or(Err(RawOrigin::None.into()))
			}
		}
	};
}
