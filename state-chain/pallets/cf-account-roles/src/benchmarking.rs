#![cfg(feature = "runtime-benchmarks")]

use super::*;
use frame_benchmarking::v2::*;

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn set_vanity_name() {
		let caller: T::AccountId = whitelisted_caller();
		let name = BoundedVec::try_from(str::repeat("x", 64).as_bytes().to_vec()).unwrap();

		#[extrinsic_call]
		set_vanity_name(RawOrigin::Signed(caller.clone()), name.clone());

		assert_eq!(VanityNames::<T>::get().get(&caller), Some(&name));
	}
}
