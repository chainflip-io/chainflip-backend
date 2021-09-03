#[macro_export]
macro_rules! impl_mock_ensure_governance_for_origin {
	($origin:ty) => {
		pub struct MockEnsureGovernance;

		impl frame_support::traits::EnsureOrigin<$origin> for MockEnsureGovernance {
			type Success = ();

			fn try_origin(o: $origin) -> Result<Self::Success, $origin> {
				frame_system::ensure_root(o).or(Err(frame_system::RawOrigin::None.into()))
			}
		}
	};
}
