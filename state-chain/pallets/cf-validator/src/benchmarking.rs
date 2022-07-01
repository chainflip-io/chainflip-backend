//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use pallet_cf_reputation::Config as ReputationConfig;
use pallet_cf_staking::Config as StakingConfig;
use pallet_session::Config as SessionConfig;

// use sp_runtime::app_crypto::RuntimePublic;
use sp_application_crypto::RuntimeAppPublic;

use frame_benchmarking::{account, benchmarks, whitelisted_caller};
use frame_support::{assert_ok, dispatch::UnfilteredDispatchable};
use frame_system::{pallet_prelude::OriginFor, RawOrigin};

mod benchmark_crypto {
	use sp_application_crypto::{app_crypto, ed25519, KeyTypeId};
	pub const PEER_ID_KEY: KeyTypeId = KeyTypeId(*b"peer");
	app_crypto!(ed25519, PEER_ID_KEY);
}

pub trait RuntimeConfig: Config + StakingConfig + SessionConfig + ReputationConfig {}

impl<T: Config + StakingConfig + SessionConfig + ReputationConfig> RuntimeConfig for T {}

/// Initialises bidders by staking each one, registering session keys and peer ids.
pub fn init_bidders<T: RuntimeConfig>(n: u32) {
	for (i, bidder) in (0..n).map(|i| account::<T::AccountId>("auction-bidders", i, 0)).enumerate()
	{
		let bidder_origin: OriginFor<T> = RawOrigin::Signed(bidder.clone()).into();
		assert_ok!(pallet_cf_staking::Pallet::<T>::staked(
			T::EnsureWitnessed::successful_origin(),
			bidder.clone(),
			(100_000u128 * 10u128.pow(18)).unique_saturated_into(),
			pallet_cf_staking::ETH_ZERO_ADDRESS,
			Default::default()
		));
		assert_ok!(pallet_cf_staking::Pallet::<T>::activate_account(bidder_origin.clone(),));

		assert_ok!(pallet_session::Pallet::<T>::set_keys(
			bidder_origin.clone(),
			T::Keys::decode(&mut &i.to_be_bytes().repeat(128 / 4)[..]).unwrap(),
			vec![],
		));

		let public_key: benchmark_crypto::Public = RuntimeAppPublic::generate_pair(None);
		let signature = public_key.sign(&bidder.encode()).unwrap();
		assert_ok!(Pallet::<T>::register_peer_id(
			bidder_origin.clone(),
			public_key.try_into().unwrap(),
			1337,
			1u128,
			signature.try_into().unwrap(),
		));

		assert_ok!(pallet_cf_reputation::Pallet::<T>::heartbeat(bidder_origin.clone(),));
	}
}

benchmarks! {
	where_clause {
		where
			T: RuntimeConfig
	}

	set_blocks_for_epoch {
		let b = 2_u32;
		let call = Call::<T>::set_blocks_for_epoch { number_of_blocks: b.into() };
		let o = <T as Config>::EnsureGovernance::successful_origin();
	}: {
		call.dispatch_bypass_filter(o)?
	}
	verify {
		assert_eq!(Pallet::<T>::epoch_number_of_blocks(), 2_u32.into())
	}
	set_backup_node_percentage {
		let call = Call::<T>::set_backup_node_percentage { percentage: 20 };
		let o = <T as Config>::EnsureGovernance::successful_origin();
	}: {
		call.dispatch_bypass_filter(o)?
	}
	verify {
		assert_eq!(Pallet::<T>::backup_node_percentage(), 20u8)
	}
	set_authority_set_min_size {
		let call = Call::<T>::set_authority_set_min_size { min_size: 20 };
		let o = <T as Config>::EnsureGovernance::successful_origin();
	}: {
		call.dispatch_bypass_filter(o)?
	}
	verify {
		assert_eq!(Pallet::<T>::authority_set_min_size(), 20u8)
	}
	// force_rotation {
	// 	let call = Call::<T>::force_rotation {};
	// 	let o = <T as Config>::EnsureGovernance::successful_origin();
	// }: {
	// 	call.dispatch_bypass_filter(o)?
	// }
	// verify {
	// 	assert!(matches!(Pallet::<T>::current_rotation_phase(), RotationPhase::VaultsRotating(..)));
	// }
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
		assert_eq!(Pallet::<T>::node_cfe_version(validator_id), version)
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

	rotation_phase_idle {
		assert!(T::MissedAuthorshipSlots::missed_slots().is_empty());

	}: {
		Pallet::<T>::on_initialize(1u32.into());
	}
	verify {
		assert_eq!(CurrentRotationPhase::<T>::get(), RotationPhase::Idle);
	}

	start_authority_rotation {
		let a in 3 .. 400;
		init_bidders::<T>(a);
	}: {
		Pallet::<T>::start_authority_rotation();
	}
	verify {
		assert!(matches!(
			CurrentRotationPhase::<T>::get(),
			RotationPhase::VaultsRotating(..)
		));
	}
}

// TODO: add the test execution we we've a solution for the register_peer_id benchmark
// impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
