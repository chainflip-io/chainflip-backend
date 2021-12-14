//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use frame_benchmarking::{benchmarks, impl_benchmark_test_suite, whitelisted_caller};
use frame_system::RawOrigin;
use hex_literal::hex;
use sp_runtime::{app_crypto::RuntimePublic, KeyTypeId};
use sp_std::{convert::TryFrom, str::FromStr};

#[allow(unused)]
use crate::Pallet as Validator;

benchmarks! {
	set_blocks_for_epoch {
		let b = 2_u32;
	}: _(RawOrigin::Root, b.into())
	verify {
		assert_eq!(Pallet::<T>::epoch_number_of_blocks(), 2_u32.into())
	}
	force_rotation {
	}: _(RawOrigin::Root)
	verify {
		assert_eq!(Pallet::<T>::force(), true)
	}
	cfe_version {
		let caller: T::AccountId = whitelisted_caller();
		let version = SemVer {
			major: 1,
			minor: 2,
			patch: 3
		};
	}: _(RawOrigin::Signed(caller.clone()), version.clone())
	verify {
		let validator_id: T::ValidatorId = caller.into();
		assert_eq!(Pallet::<T>::validator_cfe_version(validator_id), version)
	}
	register_peer_id {
		let caller: T::AccountId = whitelisted_caller();
		let public = Ed25519PublicKey::from_raw(hex!(
			"d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a"
		));
		let signature = RuntimePublic::sign(&public, KeyTypeId(*b"dumy"), &caller.encode()).unwrap();
	}: _(RawOrigin::Signed(caller.clone()), public, signature)
}

impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
