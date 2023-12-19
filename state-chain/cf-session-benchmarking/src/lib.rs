#![cfg(feature = "runtime-benchmarks")]
#![cfg_attr(not(feature = "std"), no_std)]

use codec::Decode;
use frame_benchmarking::{benchmarks, whitelisted_caller};
use frame_support::{assert_ok, sp_runtime::traits::Convert};
use frame_system::RawOrigin;
use pallet_session::*;
use rand::{RngCore, SeedableRng};
use sp_std::{prelude::*, vec};

pub struct Pallet<T: Config>(pallet_session::Pallet<T>);
pub trait Config: pallet_session::Config + pallet_session::historical::Config {}

fn generate_key<T: Config>(seed: u64) -> T::Keys {
	let mut key = [0u8; 128];
	let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
	rng.fill_bytes(&mut key);
	Decode::decode(&mut &key[..]).unwrap()
}

benchmarks! {
	set_keys {
		let caller: T::AccountId = whitelisted_caller();
		let validator_id = T::ValidatorIdOf::convert(caller.clone()).unwrap();
		<NextKeys<T>>::insert(validator_id.clone(), generate_key::<T>(1));
		frame_system::Pallet::<T>::inc_providers(&caller);
		assert_ok!(frame_system::Pallet::<T>::inc_consumers(&caller));
		let new_key = generate_key::<T>(0);
	}: _(RawOrigin::Signed(caller), new_key.clone(), vec![])
	verify {
		assert_eq!(<NextKeys<T>>::get(validator_id).expect("No key for id"), new_key);
	}
	purge_keys {
		let caller: T::AccountId = whitelisted_caller();
		let validator_id = T::ValidatorIdOf::convert(caller.clone()).unwrap();
		<NextKeys<T>>::insert(validator_id.clone(), generate_key::<T>(0));
	}: _(RawOrigin::Signed(caller))
	verify {
		assert_eq!(<NextKeys<T>>::get(validator_id), None);
	}
}
