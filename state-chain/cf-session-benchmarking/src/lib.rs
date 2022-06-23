#![cfg_attr(not(feature = "std"), no_std)]
use sp_runtime::traits::TrailingZeroInput;

use frame_benchmarking::{benchmarks, whitelisted_caller};
use frame_support::{assert_ok, codec::Decode};
use frame_system::RawOrigin;
use pallet_session::*;
use sp_runtime::traits::Convert;
use sp_std::{prelude::*, vec};

pub struct Pallet<T: Config>(pallet_session::Pallet<T>);
pub trait Config: pallet_session::Config + pallet_session::historical::Config {}

benchmarks! {
	set_keys {
		let caller: T::AccountId = whitelisted_caller();
		frame_system::Pallet::<T>::inc_providers(&caller);
		assert_ok!(frame_system::Pallet::<T>::inc_consumers(&caller));
		let keys = T::Keys::decode(&mut TrailingZeroInput::zeroes()).unwrap();
		let proof: Vec<u8> = vec![0,1,2,3];
	}: _(RawOrigin::Signed(caller), keys, proof)
	purge_keys {
		let caller: T::AccountId = whitelisted_caller();
		let validator = T::ValidatorIdOf::convert(caller.clone()).unwrap();
		let keys = T::Keys::decode(&mut TrailingZeroInput::zeroes()).unwrap();
		<NextKeys<T>>::insert(validator, keys);
	}: _(RawOrigin::Signed(caller))
}
