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

pub fn bidder_account_id<T: frame_system::Config, I: Into<u32>>(i: I) -> T::AccountId {
	account::<T::AccountId>("auction-bidder", i.into(), 0)
}

pub fn bidder_validator_id<T: Chainflip, I: Into<u32>>(i: I) -> T::ValidatorId {
	bidder_account_id::<T, I>(i).into()
}

/// Initialises bidders for the auction by staking each one, registering session keys and peer ids
/// and submitting heartbeats.
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

/// Builds a RotationStatus with the desired numbers of candidates.
pub fn new_rotation_status<T: Config>(
	num_primary_candidates: u32,
	num_secondary_candidates: u32,
) -> RuntimeRotationStatus<T> {
	let winners = (0..num_primary_candidates).map(bidder_validator_id::<T, _>).collect::<Vec<_>>();
	let losers = (num_primary_candidates..num_primary_candidates + num_secondary_candidates)
		.map(|i| Bid::from((bidder_validator_id::<T, _>(i), 90u32.into())))
		.collect::<Vec<_>>();

	// In order to be considered as secondary candidates, a validator must either a curret backup
	// or authority. Making them authorities is easier.
	CurrentAuthorities::<T>::put(
		losers.iter().map(|bid| bid.bidder_id.clone()).collect::<Vec<_>>(),
	);

	RotationStatus::from_auction_outcome::<T>(AuctionOutcome {
		winners,
		losers,
		bond: 100u32.into(),
	})
}

pub fn setup_and_start_vault_rotation<T: Config>(
	num_primary_candidates: u32,
	num_secondary_candidates: u32,
) {
	Pallet::<T>::start_vault_rotation(new_rotation_status::<T>(
		num_primary_candidates,
		num_secondary_candidates,
	));
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

	// expire_epoch {

	// }: {

	// }
	// verify {

	// }

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

	rotation_phase_vaults_rotating_pending {
		// a = authority set target size
		let a in 3 .. 150;

		// Set up a vault rotation with a primary candidates and 50 auction losers (the losers just have to be
		// enough to fill up available secondary slots).
		setup_and_start_vault_rotation::<T>(a, 50);

		// This assertion ensures we are using the correct weight parameter.
		assert_eq!(
			match CurrentRotationPhase::<T>::get() {
				RotationPhase::VaultsRotating(rotation_status) => Some(rotation_status.weight_params()),
				_ => None,
			}.expect("phase should be VaultsRotating"),
			a
		);
	}: {
		assert!(matches!(
			CurrentRotationPhase::<T>::get(),
			RotationPhase::VaultsRotating(..)
		));
		Pallet::<T>::on_initialize(1u32.into());
	}
	verify {
		assert_eq!(T::VaultRotator::get_vault_rotation_outcome(), AsyncResult::Pending);
	}

	rotation_phase_vaults_rotating_success {
		// a = authority set target size
		let a in 3 .. 150;

		// Set up a vault rotation with a primary candidates and 50 auction losers (the losers just have to be
		// enough to fill up available secondary slots).
		setup_and_start_vault_rotation::<T>(a, 50);

		// Simulate success.
		T::VaultRotator::set_vault_rotation_outcome(AsyncResult::Ready(Ok(())));

		// This assertion ensures we are using the correct weight parameter.
		assert_eq!(
			match CurrentRotationPhase::<T>::get() {
				RotationPhase::VaultsRotating(rotation_status) => Some(rotation_status.weight_params()),
				_ => None,
			}.expect("phase should be VaultsRotating"),
			a,
			"Incorrect weight parameters."
		);
	}: {
		Pallet::<T>::on_initialize(1u32.into());
	}
	verify {
		assert!(matches!(
			CurrentRotationPhase::<T>::get(),
			RotationPhase::VaultsRotated(..)
		));
	}

	rotation_phase_vaults_rotating_failure {
		// o = number of offenders - can be at most 1/3 of the set size.
		let o in 1 .. { 150 / 3 };

		// Set up a vault rotation.
		setup_and_start_vault_rotation::<T>(150, 50);

		// Simulate failure.
		let offenders = (0..o).map(bidder_validator_id::<T, _>).collect::<Vec<_>>();
		T::VaultRotator::set_vault_rotation_outcome(AsyncResult::Ready(Err(offenders.clone())));

		// This assertion ensures we are using the correct weight parameters.
		assert_eq!(offenders.len() as u32, o, "Incorrect weight parameters.");
	}: {
		Pallet::<T>::on_initialize(1u32.into());
	}
	verify {
		assert!(
			matches!(
				CurrentRotationPhase::<T>::get(),
				RotationPhase::VaultsRotating(rotation_status)
					if rotation_status.authority_candidates::<BTreeSet<_>>().is_disjoint(
						&offenders.clone().into_iter().collect::<BTreeSet<_>>()
					)
			),
			"Offenders should not be authority candidates."
		);
	}

	/**** 3. RotationPhase::VaultsRotated ****/
	/**** 4. RotationPhase::SessionRotating ****/
	/**** (Both phases have equal weight) ****/

	rotation_phase_vaults_rotated {
		// a = authority set target size
		let a in 3 .. 150;

		// Set up a vault rotation.
		let rotation_status = new_rotation_status::<T>(a, 50);
		CurrentRotationPhase::<T>::put(RotationPhase::VaultsRotated(rotation_status));

		// This assertion ensures we are using the correct weight parameter.
		assert_eq!(
			match CurrentRotationPhase::<T>::get() {
				RotationPhase::VaultsRotated(rotation_status) => Some(rotation_status.weight_params()),
				_ => None,
			}.expect("phase should be VaultsRotated"),
			a,
			"Incorrect weight parameters."
		);
	}: {
		Pallet::<T>::on_initialize(1u32.into());
	}
	verify {
		assert!(
			matches!(
				CurrentRotationPhase::<T>::get(),
				RotationPhase::VaultsRotated(..),
			),
		);
	}

}

// impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
