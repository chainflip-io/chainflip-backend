#![cfg(feature = "runtime-benchmarks")]

use super::*;
use frame_benchmarking::v2::*;

// Keep this to avoid CI warnings about no benchmarks in the crate.
#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn set_vanity_name() {
		let caller: T::AccountId = whitelisted_caller();
		let name = str::repeat("x", 64).as_bytes().to_vec();

		#[extrinsic_call]
		set_vanity_name(RawOrigin::Signed(caller.clone()), name.clone());

		assert_eq!(VanityNames::<T>::get().get(&caller), Some(&name));
	}
}
