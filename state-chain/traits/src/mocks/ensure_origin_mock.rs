use std::marker::PhantomData;

pub struct NeverFailingOriginCheck<T>(PhantomData<T>);

impl<T: frame_system::Config> frame_support::traits::EnsureOrigin<T::Origin>
	for NeverFailingOriginCheck<T>
{
	type Success = ();

	fn try_origin(_o: T::Origin) -> Result<Self::Success, T::Origin> {
		Ok(())
	}
}
