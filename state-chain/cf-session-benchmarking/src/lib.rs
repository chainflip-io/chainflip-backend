#![cfg_attr(not(feature = "std"), no_std)]
use sp_runtime::traits::TrailingZeroInput;

use frame_benchmarking::{account, benchmarks, whitelisted_caller};
use frame_support::{assert_ok, codec::Decode};
use frame_system::RawOrigin;
use pallet_session::*;
use rand::{RngCore, SeedableRng};
use sp_runtime::traits::Convert;
use sp_std::{prelude::*, vec};

pub struct Pallet<T: Config>(pallet_session::Pallet<T>);
pub trait Config: pallet_session::Config + pallet_session::historical::Config {}

#[derive(Clone)]
struct Validator<T: Config> {
	account_id: T::AccountId,
	keys: T::Keys,
	proof: Vec<u8>,
}

fn generate_validator<T: Config>(seed: u32) -> Validator<T> {
	let controller: T::AccountId = account("doogle", seed, seed);
	let keys = {
		let mut keys = [0u8; 128];
		let mut rng = rand::rngs::StdRng::seed_from_u64(seed as u64);
		rng.fill_bytes(&mut keys);
		keys
	};
	Validator {
		account_id: controller,
		keys: Decode::decode(&mut &keys[..]).unwrap(),
		proof: vec![],
	}
}

fn setup_validator<T: Config>(validator: Validator<T>) -> Result<(), sp_runtime::DispatchError> {
	frame_system::Pallet::<T>::inc_providers(&validator.account_id);
	pallet_session::Pallet::<T>::set_keys(
		RawOrigin::Signed(validator.account_id).into(),
		validator.keys,
		validator.proof,
	)
}

benchmarks! {
	set_keys {
		for seed in 0..150 {
			assert_ok!(setup_validator::<T>(generate_validator::<T>(seed)));
		}
		let validator = generate_validator::<T>(151);
		assert_ok!(setup_validator::<T>(validator.clone()));
	}: _(RawOrigin::Signed(validator.account_id), validator.keys, validator.proof)
	purge_keys {
		for seed in 0..150 {
			assert_ok!(setup_validator::<T>(generate_validator::<T>(seed)));
		}
		let validator = generate_validator::<T>(151);
		assert_ok!(setup_validator::<T>(validator.clone()));
		<NextKeys<T>>::insert(T::ValidatorIdOf::convert(validator.clone().account_id).unwrap(), validator.keys);
		// let keys = T::Keys::decode(&mut TrailingZeroInput::zeroes()).unwrap();
	}: _(RawOrigin::Signed(validator.account_id))
}
