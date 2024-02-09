#![cfg(feature = "runtime-benchmarks")]

use super::*;

use pallet_cf_funding::Config as FundingConfig;
use pallet_cf_reputation::Config as ReputationConfig;
use pallet_session::Config as SessionConfig;

use cf_traits::{AccountRoleRegistry, KeyRotationStatusOuter, SafeMode, SetSafeMode};
use frame_benchmarking::v2::*;
use frame_support::{
	assert_ok,
	sp_runtime::{Digest, DigestItem},
	storage_alias,
	traits::{OnNewAccount, UnfilteredDispatchable},
};
use frame_system::{pallet_prelude::OriginFor, Pallet as SystemPallet, RawOrigin};
use sp_application_crypto::RuntimeAppPublic;
use sp_std::vec;

mod p2p_crypto {
	use sp_application_crypto::{app_crypto, ed25519, KeyTypeId};
	pub const PEER_ID_KEY: KeyTypeId = KeyTypeId(*b"peer");
	app_crypto!(ed25519, PEER_ID_KEY);
}

// For accessing missed aura slot tracking.
#[storage_alias]
type LastSeenSlot = StorageValue<AuraSlotExtraction, u64>;

pub trait RuntimeConfig: Config + FundingConfig + SessionConfig + ReputationConfig {}

impl<T: Config + FundingConfig + SessionConfig + ReputationConfig> RuntimeConfig for T {}

pub fn bidder_set<T: Chainflip, Id: From<<T as frame_system::Config>::AccountId>, I: Into<u32>>(
	size: I,
	set_id: I,
) -> impl Iterator<Item = Id> {
	let set_id = set_id.into();
	(0..size.into())
		.map(move |i| account::<<T as frame_system::Config>::AccountId>("bidder", i, set_id).into())
}

/// Initialises bidders for the auction by funding each one, registering session keys and peer ids
/// and submitting heartbeats.
pub fn init_bidders<T: RuntimeConfig>(n: u32, set_id: u32, flip_funded: u128) {
	for bidder in bidder_set::<T, <T as frame_system::Config>::AccountId, _>(n, set_id) {
		let bidder_origin: OriginFor<T> = RawOrigin::Signed(bidder.clone()).into();
		assert_ok!(pallet_cf_funding::Pallet::<T>::funded(
			T::EnsureWitnessed::try_successful_origin().unwrap(),
			bidder.clone(),
			(flip_funded * FLIPPERINOS_PER_FLIP).unique_saturated_into(),
			Default::default(),
			Default::default()
		));
		<T as frame_system::Config>::OnNewAccount::on_new_account(&bidder);
		assert_ok!(<T as Chainflip>::AccountRoleRegistry::register_as_validator(&bidder));
		assert_ok!(pallet_cf_funding::Pallet::<T>::start_bidding(bidder_origin.clone(),));

		let public_key: p2p_crypto::Public = RuntimeAppPublic::generate_pair(None);
		let signature = public_key.sign(&bidder.encode()).unwrap();
		assert_ok!(Pallet::<T>::register_peer_id(
			bidder_origin.clone(),
			public_key.clone().into(),
			1337,
			1u128,
			signature.into(),
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

pub fn try_start_keygen<T: RuntimeConfig>(
	primary_candidates: u32,
	secondary_candidates: u32,
	epoch: u32,
) {
	// Use an offset to ensure the candidate sets don't clash.
	const LARGE_OFFSET: u32 = 100;
	init_bidders::<T>(primary_candidates, epoch, 100_000u128);
	init_bidders::<T>(secondary_candidates, epoch + LARGE_OFFSET, 90_000u128);

	Pallet::<T>::try_start_keygen(RotationState::from_auction_outcome::<T>(AuctionOutcome {
		winners: bidder_set::<T, ValidatorIdOf<T>, _>(primary_candidates, epoch).collect(),
		losers: bidder_set::<T, ValidatorIdOf<T>, _>(secondary_candidates, epoch + LARGE_OFFSET)
			.collect(),
		bond: 100u32.into(),
	}));

	assert!(matches!(CurrentRotationPhase::<T>::get(), RotationPhase::KeygensInProgress(..)));
}

#[benchmarks(where T: RuntimeConfig)]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn update_pallet_config() {
		let parameters = SetSizeParameters { min_size: 150, max_size: 150, max_expansion: 150 };
		let call = Call::<T>::update_pallet_config {
			update: PalletConfigUpdate::AuctionParameters { parameters },
		};
		let o = T::EnsureGovernance::try_successful_origin().unwrap();

		#[block]
		{
			assert_ok!(call.dispatch_bypass_filter(o));
		}

		assert_eq!(Pallet::<T>::auction_parameters(), parameters)
	}

	#[benchmark]
	fn cfe_version() {
		let caller: T::AccountId = whitelisted_caller();
		<T as frame_system::Config>::OnNewAccount::on_new_account(&caller);
		assert_ok!(<T as Chainflip>::AccountRoleRegistry::register_as_validator(&caller));
		let version = SemVer { major: 1, minor: 2, patch: 3 };

		#[extrinsic_call]
		cfe_version(RawOrigin::Signed(caller.clone()), version);

		let validator_id: ValidatorIdOf<T> = caller.into();
		assert_eq!(Pallet::<T>::node_cfe_version(validator_id), version)
	}

	#[benchmark]
	fn register_peer_id() {
		let caller: T::AccountId = account("doogle", 0, 0);
		<T as frame_system::Config>::OnNewAccount::on_new_account(&caller);
		assert_ok!(<T as Chainflip>::AccountRoleRegistry::register_as_validator(&caller));
		let pair: p2p_crypto::Public = RuntimeAppPublic::generate_pair(None);
		let signature: Ed25519Signature = pair.sign(&caller.encode()).unwrap().into();
		let public_key: Ed25519PublicKey = pair.into();

		#[extrinsic_call]
		register_peer_id(RawOrigin::Signed(caller.clone()), public_key, 0, 0, signature);

		assert!(MappedPeers::<T>::contains_key(public_key));
		assert!(AccountPeerMapping::<T>::contains_key(&caller));
	}

	#[benchmark]
	fn set_vanity_name() {
		let caller: T::AccountId = whitelisted_caller();
		let name = str::repeat("x", 64).as_bytes().to_vec();

		#[extrinsic_call]
		set_vanity_name(RawOrigin::Signed(caller.clone()), name.clone());

		assert_eq!(VanityNames::<T>::get().get(&caller), Some(&name));
	}

	#[benchmark]
	fn expire_epoch(a: Linear<3, 150>) {
		// a: the number bidders for a successful auction.

		const OLD_EPOCH: EpochIndex = 1;
		const EPOCH_TO_EXPIRE: EpochIndex = OLD_EPOCH + 1;

		let amount = T::Amount::from(1000u32);

		HistoricalBonds::<T>::insert(OLD_EPOCH, amount);
		HistoricalBonds::<T>::insert(EPOCH_TO_EXPIRE, amount);

		let authorities: BTreeSet<_> = (0..a).map(|id| account("hello", id, id)).collect();

		HistoricalAuthorities::<T>::insert(OLD_EPOCH, authorities.clone());
		HistoricalAuthorities::<T>::insert(EPOCH_TO_EXPIRE, authorities.clone());
		for a in authorities {
			EpochHistory::<T>::activate_epoch(&a, OLD_EPOCH);
			EpochHistory::<T>::activate_epoch(&a, EPOCH_TO_EXPIRE);
		}

		// Ensure that we are expiring the expected number of authorities.
		assert_eq!(EpochHistory::<T>::epoch_authorities(EPOCH_TO_EXPIRE).len(), a as usize,);

		#[block]
		{
			Pallet::<T>::expire_epoch(EPOCH_TO_EXPIRE);
		}
	}

	#[benchmark]
	fn missed_authorship_slots(m: Linear<1, 10>) {
		// m: successive blocks missed.

		let last_slot = 1_000u64;

		SystemPallet::<T>::initialize(
			&1u32.into(),
			&SystemPallet::<T>::parent_hash(),
			&Digest { logs: vec![DigestItem::PreRuntime(*b"aura", last_slot.encode())] },
		);
		Pallet::<T>::on_initialize(1u32.into());
		assert_eq!(LastSeenSlot::get(), Some(last_slot));

		let expected_slot = last_slot + 1;
		SystemPallet::<T>::initialize(
			&1u32.into(),
			&SystemPallet::<T>::parent_hash(),
			&Digest {
				logs: vec![DigestItem::PreRuntime(*b"aura", (expected_slot + m as u64).encode())],
			},
		);

		#[block]
		{
			Pallet::<T>::punish_missed_authorship_slots();
		}

		assert_eq!(LastSeenSlot::get(), Some(expected_slot + m as u64));
	}

	/**** Rotation Benchmarks *** */

	/**** 1. RotationPhase::Idle *** */
	#[benchmark]
	fn rotation_phase_idle() {
		assert!(T::MissedAuthorshipSlots::missed_slots().is_empty());

		#[block]
		{
			Pallet::<T>::on_initialize(1u32.into());
		}

		assert_eq!(CurrentRotationPhase::<T>::get(), RotationPhase::Idle);
	}

	#[benchmark]
	fn start_authority_rotation(a: Linear<3, 400>) {
		// a = number of bidders.

		init_bidders::<T>(a, 1, 100_000u128);

		#[block]
		{
			Pallet::<T>::start_authority_rotation();
		}

		assert!(matches!(CurrentRotationPhase::<T>::get(), RotationPhase::KeygensInProgress(..)));
	}

	#[benchmark]
	fn start_authority_rotation_while_disabled_by_safe_mode() {
		<T as Config>::SafeMode::set_code_red();

		#[block]
		{
			Pallet::<T>::start_authority_rotation();
		}

		assert!(matches!(<T as Config>::SafeMode::get(), SafeMode::CODE_RED));
		assert!(matches!(CurrentRotationPhase::<T>::get(), RotationPhase::Idle));
	}

	/**** 2. RotationPhase::KeygensInProgress *** */
	#[benchmark]
	fn rotation_phase_keygen(a: Linear<3, 150>) {
		// a = authority set target size

		// Set up a vault rotation with a primary candidates and 50 auction losers (the losers just
		// have to be enough to fill up available secondary slots).
		try_start_keygen::<T>(a, 50, 1);

		// Simulate success.
		T::KeyRotator::set_status(AsyncResult::Ready(KeyRotationStatusOuter::KeygenComplete));

		// This assertion ensures we are using the correct weight parameter.
		assert_eq!(
			match CurrentRotationPhase::<T>::get() {
				RotationPhase::KeygensInProgress(rotation_state) =>
					Some(rotation_state.num_primary_candidates()),
				_ => None,
			}
			.expect("phase should be KeygensInProgress"),
			a,
			"Incorrect weight parameters."
		);

		#[block]
		{
			Pallet::<T>::on_initialize(1u32.into());
		}

		assert!(matches!(
			CurrentRotationPhase::<T>::get(),
			RotationPhase::KeyHandoversInProgress(..)
		));
	}

	#[benchmark]
	fn rotation_phase_activating_keys(a: Linear<3, 150>) {
		// a = authority set target size

		try_start_keygen::<T>(a, 50, 1);

		let block = frame_system::Pallet::<T>::current_block_number();

		assert!(matches!(CurrentRotationPhase::<T>::get(), RotationPhase::KeygensInProgress(..)));

		T::KeyRotator::set_status(AsyncResult::Ready(KeyRotationStatusOuter::KeygenComplete));

		Pallet::<T>::on_initialize(block);

		assert!(matches!(
			CurrentRotationPhase::<T>::get(),
			RotationPhase::KeyHandoversInProgress(..)
		));

		T::KeyRotator::set_status(AsyncResult::Ready(KeyRotationStatusOuter::KeyHandoverComplete));

		Pallet::<T>::on_initialize(block);

		T::KeyRotator::set_status(AsyncResult::Ready(KeyRotationStatusOuter::RotationComplete));

		#[block]
		{
			Pallet::<T>::on_initialize(1u32.into());
		}

		assert!(matches!(CurrentRotationPhase::<T>::get(), RotationPhase::NewKeysActivated(..)));
	}

	#[benchmark]
	fn register_as_validator() {
		let caller: T::AccountId = whitelisted_caller();
		<T as frame_system::Config>::OnNewAccount::on_new_account(&caller);

		#[extrinsic_call]
		register_as_validator(RawOrigin::Signed(caller.clone()));

		assert_ok!(<T as Chainflip>::AccountRoleRegistry::ensure_validator(
			RawOrigin::Signed(caller).into()
		));
	}

	// NOTE: Test suite not included due to missing Funding and Reputation pallet in `mock::Test`.
}
