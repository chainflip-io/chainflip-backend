use sp_runtime::traits::{UniqueSaturatedInto, Bounded};
use crate::NonceProvider;
use frame_support::traits::UnixTime;
use std::marker::PhantomData;

pub struct NonceUnixTime<N, T> {_marker1: PhantomData<N>, _marker2: PhantomData<T>}

impl<N, T> NonceProvider for NonceUnixTime<N, T> where
	T : UnixTime,
	N: From<u64> + Bounded,{
	type Nonce = N;

	fn generate_nonce() -> Self::Nonce {
		// For now, we expect the nonce to be an u64 to stay compatible with the CFE
		let u64_nonce = T::now().as_nanos() as u64;
		u64_nonce.unique_saturated_into()
	}
}