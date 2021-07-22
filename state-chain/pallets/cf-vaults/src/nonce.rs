use crate::NonceProvider;
use frame_support::traits::UnixTime;
use sp_runtime::traits::{Bounded, UniqueSaturatedInto};
use std::marker::PhantomData;

pub struct NonceUnixTime<N, T> {
	_marker1: PhantomData<N>,
	_marker2: PhantomData<T>,
}

impl<N, T> NonceProvider for NonceUnixTime<N, T>
where
	T: UnixTime,
	N: From<u64> + Bounded,
{
	type Nonce = N;

	/// Generate a nonce using a unix timestamp
	fn generate_nonce() -> Self::Nonce {
		// For now, we expect the nonce to be an u64 to stay compatible with the CFE
		let u64_nonce = T::now().as_nanos() as u64;
		u64_nonce.unique_saturated_into()
	}
}
