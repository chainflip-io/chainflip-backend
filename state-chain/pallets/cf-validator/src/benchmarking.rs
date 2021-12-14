//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use frame_benchmarking::{account, benchmarks, impl_benchmark_test_suite, whitelisted_caller};
use frame_system::RawOrigin;
use hex_literal::hex;
use scale_info::prelude::string::String;
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
		let caller: T::AccountId = account("doogle", 0, 0);
		let raw_pub_key: [u8; 32] = [215, 214, 76, 73, 117, 67, 12, 169, 46, 14, 234, 118, 144, 29, 48, 228, 172, 189, 247, 50, 137, 236, 227, 19, 38, 192, 149, 27, 144, 119, 198, 215];
		let raw_signature: [u8; 64] = [169, 132, 57, 6, 179, 1, 194, 232, 6, 89, 21, 10, 100, 2, 187, 136, 30, 254, 232, 52, 57, 240, 83, 154, 51, 110, 225, 247, 101, 191, 153, 83, 61, 208, 95, 79, 244, 62, 95, 193, 171, 75, 198, 58, 208, 93, 100, 153, 124, 69, 184, 170, 242, 74, 26, 11, 84, 80, 100, 21, 107, 96, 52, 14];
		let message = caller.encode();
		let public = Ed25519PublicKey::from_raw(raw_pub_key);
		let signature = Ed25519Signature::from_raw(raw_signature);
	}: _(RawOrigin::Signed(caller.into()), public, signature)
}

impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
