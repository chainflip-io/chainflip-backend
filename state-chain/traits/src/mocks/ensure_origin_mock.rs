use std::marker::PhantomData;
pub struct NeverFailingOriginCheck<T>(PhantomData<T>);

impl<T: frame_system::Config> frame_support::traits::EnsureOrigin<T::RuntimeOrigin>
	for NeverFailingOriginCheck<T>
{
	type Success = ();

	fn try_origin(_o: T::RuntimeOrigin) -> Result<Self::Success, T::RuntimeOrigin> {
		Ok(())
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn successful_origin() -> T::RuntimeOrigin {
		frame_system::RawOrigin::Root.into()
	}
}
