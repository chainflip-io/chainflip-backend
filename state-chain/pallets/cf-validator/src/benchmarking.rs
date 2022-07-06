#![cfg(feature = "runtime-benchmarks")]

use super::*;

use cf_traits::{AuctionOutcome, Bid};
use pallet_cf_online::Config as OnlineConfig;
use pallet_cf_staking::Config as StakingConfig;
use pallet_session::Config as SessionConfig;

use sp_application_crypto::RuntimeAppPublic;

use frame_benchmarking::{account, benchmarks, whitelisted_caller};
use frame_support::{assert_ok, dispatch::UnfilteredDispatchable};
use frame_system::{pallet_prelude::OriginFor, RawOrigin};

mod p2p_crypto {
	use sp_application_crypto::{app_crypto, ed25519, KeyTypeId};
	pub const PEER_ID_KEY: KeyTypeId = KeyTypeId(*b"peer");
	app_crypto!(ed25519, PEER_ID_KEY);
}

pub trait RuntimeConfig: Config + StakingConfig + SessionConfig + OnlineConfig {}

impl<T: Config + StakingConfig + SessionConfig + OnlineConfig> RuntimeConfig for T {}

pub fn bidder_account_id<T: RuntimeConfig, I: Into<u32>>(i: I) -> T::AccountId {
	account::<T::AccountId>("auction-bidder", i.into(), 0)
}

pub fn bidder_validator_id<T: RuntimeConfig, I: Into<u32>>(i: I) -> <T as Chainflip>::ValidatorId {
	bidder_account_id::<T, I>(i).into()
}

/// Initialises bidders by staking each one, registering session keys and peer ids.
pub fn init_bidders<T: RuntimeConfig>(n: u32) {
	for (i, bidder) in (0..n).map(bidder_account_id::<T, _>).enumerate() {
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
			// 128 / 4 because u32 is 4 bytes.
			T::Keys::decode(&mut &i.to_be_bytes().repeat(128 / 4)[..]).unwrap(),
			vec![],
		));

		let public_key: p2p_crypto::Public = RuntimeAppPublic::generate_pair(None);
		let signature = public_key.sign(&bidder.encode()).unwrap();
		assert_ok!(Pallet::<T>::register_peer_id(
			bidder_origin.clone(),
			public_key.try_into().unwrap(),
			1337,
			1u128,
			signature.try_into().unwrap(),
		));

		assert_ok!(pallet_cf_online::Pallet::<T>::heartbeat(bidder_origin.clone(),));
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
		let call = Call::<T>::set_authority_set_min_size { min_size: 1 };
		let o = <T as Config>::EnsureGovernance::successful_origin();
	}: {
		call.dispatch_bypass_filter(o)?
	}
	verify {
		assert_eq!(Pallet::<T>::authority_set_min_size(), 1u8)
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
	register_peer_id {
		let caller: T::AccountId = account("doogle", 0, 0);
		let pair: p2p_crypto::Public = RuntimeAppPublic::generate_pair(None);
		let signature: Ed25519Signature = pair.sign(&caller.encode()).unwrap().try_into().unwrap();
		let public_key: Ed25519PublicKey = pair.try_into().unwrap();
	}: _(RawOrigin::Signed(caller.clone().into()), public_key, 0, 0, signature)
	verify {
		assert!(MappedPeers::<T>::contains_key(&public_key));
		assert!(AccountPeerMapping::<T>::contains_key(&caller));
	}

	set_vanity_name {
		let caller: T::AccountId = whitelisted_caller();
		let name = str::repeat("x", 64).as_bytes().to_vec();
	}: _(RawOrigin::Signed(caller.clone()), name.clone())
	verify {
		assert_eq!(VanityNames::<T>::get().get(&caller.into()), Some(&name));
	}

	/**** Rotation Benchmarks ****/

	/**** 1. RotationPhase::Idle ****/

	rotation_phase_idle {
		assert!(T::MissedAuthorshipSlots::missed_slots().is_empty());
	}: {
		Pallet::<T>::on_initialize(1u32.into());
	}
	verify {
		assert_eq!(CurrentRotationPhase::<T>::get(), RotationPhase::Idle);
	}

	start_authority_rotation {
		// a = number of bidders.
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

	start_authority_rotation_in_maintenance_mode {
		T::SystemState::activate_maintenance_mode();
	}: {
		Pallet::<T>::start_authority_rotation();
	}
	verify {
		assert!(matches!(
			CurrentRotationPhase::<T>::get(),
			RotationPhase::Idle
		));
	}

	/**** 2. RotationPhase::VaultsRotating ****/

	rotation_phase_vaults_rotating {
		// a = authority set target size
		let a in 3 .. 150;

		let winners = (0..a).map(bidder_validator_id::<T, _>).collect::<Vec<_>>();
		let losers = (a..a + 50)
			.map(|i| Bid::from((bidder_validator_id::<T, _>(i), 90u32.into())))
			.collect::<Vec<_>>();

		Pallet::<T>::start_vault_rotation(RotationStatus::from_auction_outcome::<T>(AuctionOutcome {
			winners,
			losers,
			bond: 100u32.into(),
		}));

		// This assertion ensures we are using the correct weight parameter.
		assert_eq!(
			match CurrentRotationPhase::<T>::get() {
				RotationPhase::VaultsRotating(rotation_status) => Some(rotation_status.weight_params()),
				_ => None,
			}.expect("phase should be VaultsRotating"),
			a
		);
	}: {
		Pallet::<T>::on_initialize(1u32.into());
	}
	verify {
		assert_eq!(T::VaultRotator::get_vault_rotation_outcome(), AsyncResult::Pending);
	}
}

// impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
