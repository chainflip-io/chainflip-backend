// #![cfg_attr(not(feature = "std"), no_std)]

use frame_benchmarking::{benchmarks, whitelist_account, whitelisted_caller};
use frame_support::{
	assert_ok,
	codec::Decode,
	storage,
	traits::{KeyOwnerProofSystem, OnFinalize, OnInitialize},
};
use frame_system::RawOrigin;
use pallet_grandpa::*;
use rand::{RngCore, SeedableRng};
use sp_core::H256;
use sp_finality_grandpa;
use sp_runtime::traits::Convert;
use sp_std::{prelude::*, vec};

use fg_primitives::{AuthorityList, GRANDPA_AUTHORITIES_KEY};

use cf_traits::Chainflip;
use frame_benchmarking::account;
use frame_support::{dispatch::UnfilteredDispatchable, traits::IsType};
use pallet_cf_reputation::Call as ReputationCall;
use pallet_cf_validator::{CurrentAuthorities, CurrentRotationPhase, RotationPhase};

const SEED: u32 = 0;

pub struct Pallet<T: Config>(pallet_grandpa::Pallet<T>);
pub trait Config:
	pallet_grandpa::Config
	+ pallet_cf_validator::Config
	+ pallet_cf_reputation::Config
	+ pallet_session::Config
	+ pallet_session::historical::Config
{
}

fn add_authorities<T, I>(authorities: I)
where
	T: frame_system::Config + pallet_cf_validator::Config + pallet_cf_reputation::Config,
	I: Clone + Iterator<Item = <T as Chainflip>::ValidatorId>,
{
	CurrentAuthorities::<T>::put(authorities.clone().collect::<Vec<_>>());
	for validator_id in authorities {
		let account_id = validator_id.into_ref();
		whitelist_account!(account_id);
		ReputationCall::<T>::heartbeat {}
			.dispatch_bypass_filter(RawOrigin::Signed(account_id.clone()).into())
			.unwrap();
	}
}

benchmarks! {
	report_equivocation {
		let caller: T::AccountId = whitelisted_caller();
		let x in 0 .. 1;

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
		add_authorities::<T, _>(all_accounts);

		for i in 0..150u32 {
			pallet_session::Pallet::<T>::on_initialize(i.into());
			pallet_grandpa::Pallet::<T>::on_finalize(i.into());
		}

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
