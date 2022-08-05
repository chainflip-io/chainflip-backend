use codec::Encode;
use frame_benchmarking::{benchmarks, whitelisted_caller};
use frame_support::{
	assert_ok,
	codec::Decode,
	storage,
	traits::{KeyOwnerProofSystem, OnInitialize},
};
use frame_system::{pallet_prelude::OriginFor, RawOrigin};
use pallet_grandpa::*;
use sp_finality_grandpa;
use sp_runtime::traits::{BlockNumberProvider, Saturating};
use sp_std::{prelude::*, vec};

use pallet_authorship::Config as AuthorshipConfig;
use pallet_cf_reputation::Config as ReputationConfig;
use pallet_cf_staking::Config as StakingConfig;
use pallet_cf_validator::Config as ValidatorConfig;
use pallet_session::Config as SessionConfig;

use frame_support::traits::EnsureOrigin;

use cf_traits::{AsyncResult, AuctionOutcome, EpochInfo, VaultRotator};

use fg_primitives::{AuthorityList, GRANDPA_AUTHORITIES_KEY};

use cf_traits::Chainflip;
use frame_benchmarking::account;

use pallet_cf_validator::{CurrentRotationPhase, RotationPhase};

use frame_support::traits::OnFinalize;
use sp_application_crypto::RuntimeAppPublic;
use sp_runtime::traits::UniqueSaturatedInto;

const SEED: u32 = 0;
pub struct Pallet<T: Config>(pallet_grandpa::Pallet<T>);
pub trait Config:
	pallet_grandpa::Config
	+ pallet_cf_validator::Config
	+ pallet_authorship::Config
	+ pallet_cf_reputation::Config
	+ pallet_cf_staking::Config
	+ pallet_session::Config
	+ pallet_session::historical::Config
{
}

mod p2p_crypto {
	use sp_application_crypto::{app_crypto, ed25519, KeyTypeId};
	pub const PEER_ID_KEY: KeyTypeId = KeyTypeId(*b"peer");
	app_crypto!(ed25519, PEER_ID_KEY);
}

pub trait RuntimeConfig:
	Config + StakingConfig + SessionConfig + ReputationConfig + ValidatorConfig + AuthorshipConfig
{
}

impl<
		T: Config
			+ StakingConfig
			+ SessionConfig
			+ ReputationConfig
			+ ValidatorConfig
			+ AuthorshipConfig,
	> RuntimeConfig for T
{
}

pub fn bidder_set<T: Chainflip, Id: From<<T as frame_system::Config>::AccountId>, I: Into<u32>>(
	size: I,
	set_id: I,
) -> impl Iterator<Item = Id> {
	let set_id = set_id.into();
	(0..size.into())
		.map(move |i| account::<<T as frame_system::Config>::AccountId>("bidder", i, set_id).into())
}

pub fn init_bidders<T: RuntimeConfig>(n: u32, set_id: u32, flip_staked: u128) {
	for bidder in bidder_set::<T, <T as frame_system::Config>::AccountId, _>(n, set_id) {
		let bidder_origin: OriginFor<T> = RawOrigin::Signed(bidder.clone()).into();
		assert_ok!(pallet_cf_staking::Pallet::<T>::staked(
			<T as Chainflip>::EnsureWitnessed::successful_origin(),
			bidder.clone(),
			(flip_staked * 10u128.pow(18)).unique_saturated_into(),
			pallet_cf_staking::ETH_ZERO_ADDRESS,
			Default::default()
		));
		assert_ok!(pallet_cf_staking::Pallet::<T>::activate_account(bidder_origin.clone(),));

		let public_key: p2p_crypto::Public = RuntimeAppPublic::generate_pair(None);
		let signature = public_key.sign(&bidder.encode()).unwrap();
		assert_ok!(pallet_cf_validator::Pallet::<T>::register_peer_id(
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

	pallet_cf_validator::Pallet::<T>::start_vault_rotation(
		pallet_cf_validator::RotationState::from_auction_outcome::<T>(AuctionOutcome {
			winners: bidder_set::<T, <T as Chainflip>::ValidatorId, _>(primary_candidates, epoch)
				.collect(),
			losers: bidder_set::<T, <T as Chainflip>::ValidatorId, _>(
				secondary_candidates,
				epoch + LARGE_OFFSET,
			)
			.map(|id| (id, 90_000u32.into()).into())
			.collect(),
			bond: 100u32.into(),
		}),
	);

	assert!(matches!(CurrentRotationPhase::<T>::get(), RotationPhase::VaultsRotating(..)));
}

pub fn rotate_authorities<T: RuntimeConfig>(candidates: u32, epoch: u32) {
	let old_epoch = pallet_cf_validator::Pallet::<T>::epoch_index();

	// Use an offset to ensure the candidate sets don't clash.
	init_bidders::<T>(candidates, epoch, 100_000u128);

	// Resolves the auction and starts the vault rotation.
	pallet_cf_validator::Pallet::<T>::start_authority_rotation();

	assert!(matches!(CurrentRotationPhase::<T>::get(), RotationPhase::VaultsRotating(..)));

	// Simulate success.
	#[cfg(feature = "runtime-benchmarks")]
	<pallet_cf_validator::Pallet<T> as VaultRotator>::set_vault_rotation_outcome::set_vault_rotation_outcome(AsyncResult::Ready(
		Ok(()),
	));

	// The rest should take care of itself.
	let mut iterations = 0;
	while !matches!(CurrentRotationPhase::<T>::get(), RotationPhase::Idle) {
		let block = frame_system::Pallet::<T>::current_block_number();
		pallet_cf_validator::Pallet::<T>::on_initialize(block);
		pallet_session::Pallet::<T>::on_initialize(block);
		let g: u64 = block.unique_saturated_into();
		frame_system::Pallet::<T>::initialize(
			&block,
			&frame_system::Pallet::<T>::block_hash(block),
			&{
				let mut digest = sp_runtime::Digest::default();
				digest.push(sp_runtime::DigestItem::PreRuntime(
					sp_consensus_aura::AURA_ENGINE_ID,
					sp_consensus_aura::Slot::from(g).encode(),
				));
				digest
			},
		);
		pallet_authorship::Pallet::<T>::on_finalize(block);

		iterations += 1;
		// if iterations > 4 {
		// 	panic!(
		// 		"Rotation should not take more than 4 iterations. Stuck at {:?}",
		// 		CurrentRotationPhase::<T>::get()
		// 	);
		// }
	}

	assert_eq!(
		pallet_cf_validator::Pallet::<T>::epoch_index(),
		old_epoch + 1,
		"authority rotation failed"
	);
}

benchmarks! {
	report_equivocation {
		let caller: T::AccountId = whitelisted_caller();
		let x in 0 .. 1;

		// 3 is the minimum number bidders for a successful auction.
		let a in 3 .. 150;

		// This is the initial authority set that will be expired.
		rotate_authorities::<T>(a, 1);
		// A new distinct authority set. The previous authorities will now be historical authorities.
		rotate_authorities::<T>(a, 2);

		const EQUIVOCATION_PROOF_BLOB: [u8; 257] = [
			1, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 136, 220, 52, 23,
			213, 5, 142, 196, 180, 80, 62, 12, 18, 234, 26, 10, 137, 190, 32,
			15, 233, 137, 34, 66, 61, 67, 52, 1, 79, 166, 176, 238, 207, 48,
			195, 55, 171, 225, 252, 130, 161, 56, 151, 29, 193, 32, 25, 157,
			249, 39, 80, 193, 214, 96, 167, 147, 25, 130, 45, 42, 64, 208, 182,
			164, 10, 0, 0, 0, 0, 0, 0, 0, 234, 236, 231, 45, 70, 171, 135, 246,
			136, 153, 38, 167, 91, 134, 150, 242, 215, 83, 56, 238, 16, 119, 55,
			170, 32, 69, 255, 248, 164, 20, 57, 50, 122, 115, 135, 96, 80, 203,
			131, 232, 73, 23, 149, 86, 174, 59, 193, 92, 121, 76, 154, 211, 44,
			96, 10, 84, 159, 133, 211, 56, 103, 0, 59, 2, 96, 20, 69, 2, 32,
			179, 16, 184, 108, 76, 215, 64, 195, 78, 143, 73, 177, 139, 20, 144,
			98, 231, 41, 117, 255, 220, 115, 41, 59, 27, 75, 56, 10, 0, 0, 0, 0,
			0, 0, 0, 128, 179, 250, 48, 211, 76, 10, 70, 74, 230, 219, 139, 96,
			78, 88, 112, 33, 170, 44, 184, 59, 200, 155, 143, 128, 40, 222, 179,
			210, 190, 84, 16, 182, 21, 34, 94, 28, 193, 163, 226, 51, 251, 134,
			233, 187, 121, 63, 157, 240, 165, 203, 92, 16, 146, 120, 190, 229,
			251, 129, 29, 45, 32, 29, 6
		];

		// Pallet::<T>::on_finalize(2_u64);

		let all_accounts = (0..150).map(|i| account::<<T as Chainflip>::ValidatorId>("signers", i, SEED));
		let authorities: AuthorityList = storage::unhashed::get_or_default::<VersionedAuthorityList>(GRANDPA_AUTHORITIES_KEY).into();
		let equivocation_key = authorities[0].0.clone();
		// add_authorities::<T, _>(all_accounts);

		// for i in 0..150u32 {
		// 	pallet_session::Pallet::<T>::on_initialize(i.into());
		// 	pallet_grandpa::Pallet::<T>::on_finalize(i.into());
		// }

		let key_owner_proof = T::KeyOwnerProofSystem::prove((sp_finality_grandpa::KEY_TYPE, equivocation_key)).unwrap();

		// let equivocation_proof = generate_equivocation_proof(
		// 	1,
		// 	(1, H256::random(), 10, &equivocation_keyring),
		// 	(1, H256::random(), 10, &equivocation_keyring),
		// );

		let equivocation_proof1: sp_finality_grandpa::EquivocationProof<<T as frame_system::Config>::Hash, <T as frame_system::Config>::BlockNumber> =
			Decode::decode(&mut &EQUIVOCATION_PROOF_BLOB[..]).unwrap();
		let equivocation_proof2 = equivocation_proof1.clone();
	}: _(RawOrigin::Signed(caller.clone()), Box::new(equivocation_proof1), key_owner_proof)
	note_stalled {
		let delay = 1000u32.into();
		let best_finalized_block_number = 1u32.into();
	}: _(RawOrigin::Root, delay, best_finalized_block_number)
	// verify {
	// 	assert!(Grandpa::<T>::stalled().is_some());
	// }
	// set_keys {
	// 	let caller: T::AccountId = whitelisted_caller();
	// 	let validator_id = T::ValidatorIdOf::convert(caller.clone()).unwrap();
	// 	<NextKeys<T>>::insert(validator_id.clone(), generate_key::<T>(1));
	// 	frame_system::Pallet::<T>::inc_providers(&caller);
	// 	assert_ok!(frame_system::Pallet::<T>::inc_consumers(&caller));
	// 	let new_key = generate_key::<T>(0);
	// }: _(RawOrigin::Signed(caller), new_key.clone(), vec![])
	// verify {
	// 	assert_eq!(<NextKeys<T>>::get(validator_id).expect("No key for id"), new_key);
	// }
	// purge_keys {
	// 	let caller: T::AccountId = whitelisted_caller();
	// 	let validator_id = T::ValidatorIdOf::convert(caller.clone()).unwrap();
	// 	<NextKeys<T>>::insert(validator_id.clone(), generate_key::<T>(0));
	// }: _(RawOrigin::Signed(caller))
	// verify {
	// 	assert_eq!(<NextKeys<T>>::get(validator_id), None);
	// }
}
