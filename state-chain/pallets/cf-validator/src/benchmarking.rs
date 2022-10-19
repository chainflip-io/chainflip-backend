#![cfg(feature = "runtime-benchmarks")]

use super::*;

use pallet_cf_reputation::Config as ReputationConfig;
use pallet_cf_staking::Config as StakingConfig;
use pallet_session::Config as SessionConfig;

use cf_primitives::AccountRole;
use cf_traits::AccountRoleRegistry;

use sp_application_crypto::RuntimeAppPublic;
use sp_runtime::{Digest, DigestItem};
use sp_std::vec;

use cf_traits::AuctionOutcome;

use frame_benchmarking::{account, benchmarks, whitelisted_caller};
use frame_support::{assert_ok, dispatch::UnfilteredDispatchable, storage_alias};
use frame_system::{pallet_prelude::OriginFor, Pallet as SystemPallet, RawOrigin};

mod p2p_crypto {
	use sp_application_crypto::{app_crypto, ed25519, KeyTypeId};
	pub const PEER_ID_KEY: KeyTypeId = KeyTypeId(*b"peer");
	app_crypto!(ed25519, PEER_ID_KEY);
}

// For accessing missed aura slot tracking.
#[storage_alias]
type LastSeenSlot = StorageValue<AuraSlotExtraction, u64>;

pub trait RuntimeConfig: Config + StakingConfig + SessionConfig + ReputationConfig {}

impl<T: Config + StakingConfig + SessionConfig + ReputationConfig> RuntimeConfig for T {}

pub fn bidder_set<T: Chainflip, Id: From<<T as frame_system::Config>::AccountId>, I: Into<u32>>(
	size: I,
	set_id: I,
) -> impl Iterator<Item = Id> {
	let set_id = set_id.into();
	(0..size.into())
		.map(move |i| account::<<T as frame_system::Config>::AccountId>("bidder", i, set_id).into())
}

/// Initialises bidders for the auction by staking each one, registering session keys and peer ids
/// and submitting heartbeats.
pub fn init_bidders<T: RuntimeConfig>(n: u32, set_id: u32, flip_staked: u128) {
	for bidder in bidder_set::<T, <T as frame_system::Config>::AccountId, _>(n, set_id) {
		let bidder_origin: OriginFor<T> = RawOrigin::Signed(bidder.clone()).into();
		assert_ok!(pallet_cf_staking::Pallet::<T>::staked(
			T::EnsureWitnessed::successful_origin(),
			bidder.clone(),
			(flip_staked * 10u128.pow(18)).unique_saturated_into(),
			pallet_cf_staking::ETH_ZERO_ADDRESS,
			Default::default()
		));
		assert_ok!(pallet_cf_staking::Pallet::<T>::activate_account(bidder_origin.clone(),));

		let public_key: p2p_crypto::Public = RuntimeAppPublic::generate_pair(None);
		let signature = public_key.sign(&bidder.encode()).unwrap();
		assert_ok!(Pallet::<T>::register_peer_id(
			bidder_origin.clone(),
			public_key.clone().try_into().unwrap(),
			1337,
			1u128,
			signature.try_into().unwrap(),
		));

		// Reuse the random peer id for the session keys, we don't need real ones.
		let fake_key = public_key.to_raw_vec().repeat(4);
		assert_ok!(pallet_session::Pallet::<T>::set_keys(
			bidder_origin.clone(),
			// Public key is 32 bytes, we need 128 bytes.
			T::Keys::decode(&mut &fake_key[..]).unwrap(),
			vec![],
		));

		assert_ok!(pallet_cf_reputation::Pallet::<T>::heartbeat(bidder_origin.clone(),));
	}
}

pub fn start_vault_rotation<T: RuntimeConfig>(
	primary_candidates: u32,
	secondary_candidates: u32,
	epoch: u32,
) {
	// Use an offset to ensure the candidate sets don't clash.
	const LARGE_OFFSET: u32 = 100;
	init_bidders::<T>(primary_candidates, epoch, 100_000u128);
	init_bidders::<T>(secondary_candidates, epoch + LARGE_OFFSET, 90_000u128);

	Pallet::<T>::start_vault_rotation(RotationState::from_auction_outcome::<T>(AuctionOutcome {
		winners: bidder_set::<T, ValidatorIdOf<T>, _>(primary_candidates, epoch).collect(),
		losers: bidder_set::<T, ValidatorIdOf<T>, _>(secondary_candidates, epoch + LARGE_OFFSET)
			.map(|id| (id, 90_000u32.into()).into())
			.collect(),
		bond: 100u32.into(),
	}));

	assert!(matches!(CurrentRotationPhase::<T>::get(), RotationPhase::VaultsRotating(..)));
}

pub fn rotate_authorities<T: RuntimeConfig>(candidates: u32, epoch: u32) {
	let old_epoch = Pallet::<T>::epoch_index();

	// Use an offset to ensure the candidate sets don't clash.
	init_bidders::<T>(candidates, epoch, 100_000u128);

	// Resolves the auction and starts the vault rotation.
	Pallet::<T>::start_authority_rotation();

	assert!(matches!(CurrentRotationPhase::<T>::get(), RotationPhase::VaultsRotating(..)));

	// Simulate success.
	T::VaultRotator::set_vault_rotation_outcome(AsyncResult::Ready(Ok(())));

	// The rest should take care of itself.
	let mut iterations = 0;
	while !matches!(CurrentRotationPhase::<T>::get(), RotationPhase::Idle) {
		let block = frame_system::Pallet::<T>::current_block_number();
		Pallet::<T>::on_initialize(block);
		pallet_session::Pallet::<T>::on_initialize(block);
		iterations += 1;
		if iterations > 4 {
			panic!(
				"Rotation should not take more than 4 iterations. Stuck at {:?}",
				CurrentRotationPhase::<T>::get()
			);
		}
	}

	assert_eq!(Pallet::<T>::epoch_index(), old_epoch + 1, "authority rotation failed");
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
		assert_eq!(Pallet::<T>::blocks_per_epoch(), 2_u32.into())
	}
	set_backup_reward_node_percentage {
		let call = Call::<T>::set_backup_reward_node_percentage { percentage: 20 };
		let o = <T as Config>::EnsureGovernance::successful_origin();
	}: {
		call.dispatch_bypass_filter(o)?
	}
	verify {
		assert_eq!(Pallet::<T>::backup_reward_node_percentage(), 20u8)
	}
	cfe_version {
		let caller: T::AccountId = whitelisted_caller();
		<T as pallet::Config>::AccountRoleRegistry::register_account(caller.clone(), AccountRole::Validator);
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
		<T as pallet::Config>::AccountRoleRegistry::register_account(caller.clone(), AccountRole::Validator);
		let pair: p2p_crypto::Public = RuntimeAppPublic::generate_pair(None);
		let signature: Ed25519Signature = pair.sign(&caller.encode()).unwrap().try_into().unwrap();
		let public_key: Ed25519PublicKey = pair.try_into().unwrap();
	}: _(RawOrigin::Signed(caller.clone()), public_key, 0, 0, signature)
	verify {
		assert!(MappedPeers::<T>::contains_key(&public_key));
		assert!(AccountPeerMapping::<T>::contains_key(&caller));
	}

	set_vanity_name {
		let caller: T::AccountId = whitelisted_caller();
		let name = str::repeat("x", 64).as_bytes().to_vec();
	}: _(RawOrigin::Signed(caller.clone()), name.clone())
	verify {
		assert_eq!(VanityNames::<T>::get().get(&caller), Some(&name));
	}

	expire_epoch {
		// 3 is the minimum number bidders for a successful auction.
		let a in 3 .. 150;

		// This is the initial authority set that will be expired.
		rotate_authorities::<T>(a, 1);
		// A new distinct authority set. The previous authorities will now be historical authorities.
		rotate_authorities::<T>(a, 2);

		const EPOCH_TO_EXPIRE: EpochIndex = 2;
		assert_eq!(
			Pallet::<T>::epoch_index(),
			EPOCH_TO_EXPIRE + 1,
		);
		// Ensure that we are expiring the expected number of authorities.
		assert_eq!(
			EpochHistory::<T>::epoch_authorities(EPOCH_TO_EXPIRE).len(),
			a as usize,
		);
	}: {
		Pallet::<T>::expire_epoch(EPOCH_TO_EXPIRE);
	}
	verify {
		assert_eq!(LastExpiredEpoch::<T>::get(), EPOCH_TO_EXPIRE);
	}

	missed_authorship_slots {
		// Unlikely we will ever miss 10 successive blocks.
		let m in 1 .. 10;

		let last_slot = 1_000u64;

		SystemPallet::<T>::initialize(&1u32.into(), &SystemPallet::<T>::parent_hash(), &Digest {
			logs: vec![DigestItem::PreRuntime(*b"aura", last_slot.encode())]
		});
		Pallet::<T>::on_initialize(1u32.into());
		assert_eq!(LastSeenSlot::get(), Some(last_slot));

		let expected_slot = last_slot + 1;
		SystemPallet::<T>::initialize(&1u32.into(), &SystemPallet::<T>::parent_hash(), &Digest {
			logs: vec![DigestItem::PreRuntime(*b"aura", (expected_slot + m as u64).encode())]
		});
	}: {
		Pallet::<T>::punish_missed_authorship_slots();
	}
	verify {
		assert_eq!(LastSeenSlot::get(), Some(expected_slot + m as u64));
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
		init_bidders::<T>(a, 1, 100_000u128);
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
		start_vault_rotation::<T>(a, a / 3, 1);

		// This assertion ensures we are using the correct weight parameter.
		assert_eq!(
			match CurrentRotationPhase::<T>::get() {
				RotationPhase::VaultsRotating(rotation_state) => Some(rotation_state.num_primary_candidates()),
				_ => None,
			}.expect("phase should be VaultsRotating"),
			a
		);
		assert!(matches!(
			CurrentRotationPhase::<T>::get(),
			RotationPhase::VaultsRotating(..)
		));
	}: {
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
		start_vault_rotation::<T>(a, 50, 1);

		// Simulate success.
		T::VaultRotator::set_vault_rotation_outcome(AsyncResult::Ready(Ok(())));

		// This assertion ensures we are using the correct weight parameter.
		assert_eq!(
			match CurrentRotationPhase::<T>::get() {
				RotationPhase::VaultsRotating(rotation_state) => Some(rotation_state.num_primary_candidates()),
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
		start_vault_rotation::<T>(150, 50, 1);

		// Simulate failure.
		let offenders = bidder_set::<T, ValidatorIdOf<T>, _>(o, 1).collect::<BTreeSet<_>>();
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
				RotationPhase::VaultsRotating(rotation_state)
					if rotation_state.authority_candidates::<BTreeSet<_>>().is_disjoint(
						&offenders
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
		start_vault_rotation::<T>(a, 50, 1);
		match CurrentRotationPhase::<T>::get() {
			RotationPhase::VaultsRotating(rotation_state) =>
				CurrentRotationPhase::<T>::put(RotationPhase::VaultsRotated(rotation_state)),
			_ => panic!("phase should be VaultsRotated"),
		}

		// This assertion ensures we are using the correct weight parameter.
		assert_eq!(
			match CurrentRotationPhase::<T>::get() {
				RotationPhase::VaultsRotated(rotation_state) => Some(rotation_state.num_primary_candidates()),
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
	set_auction_parameters {
		let params = SetSizeParameters {
			min_size: 3,
			max_size: 150,
			max_expansion: 15,
		};
	}: _(RawOrigin::Root, params)
	verify {
		assert_eq!(
			Pallet::<T>::auction_parameters(),
			params
		);
	}

}
