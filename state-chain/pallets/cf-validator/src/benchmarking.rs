//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use frame_benchmarking::{account, benchmarks, whitelisted_caller};
use frame_support::dispatch::UnfilteredDispatchable;
use frame_system::RawOrigin;

benchmarks! {
	set_blocks_for_epoch {
		let b = 2_u32;
		let call = Call::<T>::set_blocks_for_epoch(b.into());
		let o = T::EnsureGovernance::successful_origin();
	}: {
		call.dispatch_bypass_filter(o)?
	}
	verify {
		assert_eq!(Pallet::<T>::epoch_number_of_blocks(), 2_u32.into())
	}
	force_rotation {
		let call = Call::<T>::force_rotation();
		let o = T::EnsureGovernance::successful_origin();
	}: {
		call.dispatch_bypass_filter(o)?
	}
	verify {
		assert_eq!(Pallet::<T>::rotation_phase(), RotationStatus::RunAuction)
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
		let validator_id: ValidatorIdOf<T> = caller.into();
		assert_eq!(Pallet::<T>::validator_cfe_version(validator_id), version)
	}
	// TODO: this benchmark is failing in in an test environment.
	// Pretty sure the reason for this is that the account function
	// is acting differently in the test environment.
	register_peer_id {
		// Due to the fact that we have no full_crypto features
		// available in wasm we have to create a key pair as well as
		// a matching signature under an non-wasm environment.
		// The caller has to be static, otherwise the signature won't match!
		let caller: T::AccountId = account("doogle", 0, 0);
		// The public key of the key pair we used to generate the signature.
		let raw_pub_key: [u8; 32] = [
			47, 140, 97, 41, 216, 22, 207, 81, 195, 116, 188, 127, 8, 195, 230, 62, 209, 86,
			207, 120, 174, 251, 74, 101, 80, 217, 123, 135, 153, 121, 119, 238,
		];
		// The signature over the encode AccountId of caller.
		let raw_signature: [u8; 64] = [
			73, 222, 125, 246, 56, 244, 79, 99, 156, 245, 104, 9, 97, 26, 121, 81, 200, 130,
			43, 31, 70, 42, 251, 107, 92, 134, 225, 187, 149, 124, 188, 132, 170, 9, 33, 118,
			111, 56, 185, 167, 218, 58, 125, 60, 88, 20, 103, 12, 123, 11, 79, 107, 214, 126,
			219, 231, 96, 106, 227, 246, 241, 226, 33, 8,
		];
		// Build an public key object as well as the signature from raw data.
		let public = Ed25519PublicKey::from_raw(raw_pub_key);
		let signature = Ed25519Signature::from_raw(raw_signature);
	}: _(RawOrigin::Signed(caller.clone().into()), public, 0, 0, signature)
	verify {
		assert!(MappedPeers::<T>::contains_key(&public));
		assert!(AccountPeerMapping::<T>::contains_key(&caller));
	}

	set_vanity_name {
		let caller: T::AccountId = whitelisted_caller();
		let name = str::repeat("x", 64).as_bytes().to_vec();
	}: _(RawOrigin::Signed(caller.clone()), name.clone())
	verify {
		assert_eq!(VanityNames::<T>::get().get(&caller.into()), Some(&name));
	}
}

// TODO: add the test execution we we've a solution for the register_peer_id benchmark
// impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
