//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use codec::{Decode, Encode};
use frame_benchmarking::{account, benchmarks, impl_benchmark_test_suite, whitelisted_caller};
use frame_support::{dispatch::UnfilteredDispatchable, traits::OnInitialize};
use frame_system::RawOrigin;
use sp_std::vec::Vec;

type AccountIdOf<T> = <T as frame_system::Config>::AccountId;

fn create_accounts<T: Config>(count: u32) -> Vec<AccountIdOf<T>> {
	(0..=count).map(|i| account("doogle", i, 0)).collect()
}

/// Takes something [Encode]able, encodes it, and then [Decode]s the result to the desired
/// output type. Panics if the output type is incompatible with the encoded bytes of the
/// input type.
///
/// This is useful when you know that `In` and `Out` are the same at runtime, but the compiler
/// can't infer this from the type contraints.
fn transmogrify<In: Encode, Out: Decode>(thing: In) -> Out {
	let bytes = thing.encode();
	Out::decode(&mut bytes.as_slice()).unwrap()
}

const MIN_STAKE: u128 = 50_000 * 10u128.pow(18);

benchmarks! {

	staked {
		let balance: T::Balance = T::Balance::from(100u32);
		let eth_addr: EthereumAddress = [42u8; 20];
		let tx_hash: pallet::EthTransactionHash = [211u8; 32];
		let caller: T::AccountId = whitelisted_caller();

		let call = Call::<T>::staked(caller.clone(), balance, eth_addr, tx_hash);
		let origin = T::EnsureWitnessedByHistoricalActiveEpoch::successful_origin();

	}: { call.dispatch_bypass_filter(origin)? }
	verify {
		assert!(AccountRetired::<T>::get(&caller));
		assert_eq!(T::Flip::stakeable_balance(&caller), balance);
	}

	claim {
		let balance_to_claim: T::Balance = T::Balance::from(50u32);
		let balance_to_stake: T::Balance = T::Balance::from(MIN_STAKE);
		let tx_hash: pallet::EthTransactionHash = [211u8; 32];
		let eth_addr: EthereumAddress = [42u8; 20];

		let caller: T::AccountId = whitelisted_caller();
		let origin = T::EnsureWitnessedByHistoricalActiveEpoch::successful_origin();

		// Stake some funds to claim
		let stake_call = Call::<T>::staked(caller.clone(), balance_to_stake, eth_addr, tx_hash);
		stake_call.dispatch_bypass_filter(origin)?;

	} :_(RawOrigin::Signed(caller.clone()), balance_to_claim, eth_addr)
	verify {
		assert!(PendingClaims::<T>::contains_key(&caller));
	}

	claim_all {
		let eth_addr: EthereumAddress = [42u8; 20];
		let caller: T::AccountId = whitelisted_caller();

		let balance_to_stake: T::Balance = T::Balance::from(MIN_STAKE);
		let tx_hash: pallet::EthTransactionHash = [211u8; 32];

		let caller: T::AccountId = whitelisted_caller();
		let origin = T::EnsureWitnessedByHistoricalActiveEpoch::successful_origin();

		// Stake some funds to claim
		let stake_call = Call::<T>::staked(caller.clone(), balance_to_stake, eth_addr, tx_hash);
		stake_call.dispatch_bypass_filter(origin)?;

	}:_(RawOrigin::Signed(caller.clone()), eth_addr)
	verify {
		assert!(PendingClaims::<T>::contains_key(&caller));
	}

	claimed {
		let balance_to_stake: T::Balance = T::Balance::from(MIN_STAKE);
		let tx_hash: pallet::EthTransactionHash = [211u8; 32];
		let eth_addr: EthereumAddress = [42u8; 20];

		let caller: T::AccountId = whitelisted_caller();
		let origin = T::EnsureWitnessedByHistoricalActiveEpoch::successful_origin();

		// Stake some funds to claim
		let stake_call = Call::<T>::staked(caller.clone(), balance_to_stake, eth_addr, tx_hash);
		stake_call.dispatch_bypass_filter(origin.clone())?;

		// Push a claim
		let claimable = T::Flip::claimable_balance(&caller);
		Pallet::<T>::do_claim(&caller, claimable, eth_addr)?;

		let call = Call::<T>::claimed(caller.clone(), claimable, tx_hash);

	}: { call.dispatch_bypass_filter(origin)? }
	verify {
		assert!(!PendingClaims::<T>::contains_key(&caller));
	}

	post_claim_signature {
		let tx_hash: pallet::EthTransactionHash = [211u8; 32];
		let eth_addr: EthereumAddress = [42u8; 20];
		let balance_to_stake: T::Balance = T::Balance::from(MIN_STAKE);

		let caller: T::AccountId = whitelisted_caller();
		let witness_origin = T::EnsureWitnessedByHistoricalActiveEpoch::successful_origin();
		let threshold_origin = T::EnsureThresholdSigned::successful_origin();

		// Stake some funds to claim
		let stake_call = Call::<T>::staked(caller.clone(), balance_to_stake, eth_addr, tx_hash);
		stake_call.dispatch_bypass_filter(witness_origin)?;

		// Push a claim
		let claimable = T::Flip::claimable_balance(&caller);
		Pallet::<T>::do_claim(&caller, claimable, eth_addr)?;

		// TODO: insert a valid signature...

		let call = Call::<T>::post_claim_signature(caller.clone(), transmogrify(1u32));
	}: { call.dispatch_bypass_filter(threshold_origin)? }
	verify {
		assert!(PendingClaims::<T>::contains_key(&caller));
	}

	retire_account {
		let caller: T::AccountId = whitelisted_caller();
		AccountRetired::<T>::insert(caller.clone(), false);

	}:_(RawOrigin::Signed(caller.clone()))
	verify {
		assert!(AccountRetired::<T>::get(caller));
	}

	activate_account {
		let caller: T::AccountId = whitelisted_caller();
		AccountRetired::<T>::insert(caller.clone(), true);

	}:_(RawOrigin::Signed(caller.clone()))
	verify {
		assert!(!AccountRetired::<T>::get(caller));
	}

	on_initialize_best_case {
	}: {
		Pallet::<T>::on_initialize((2u32).into());
	}
	verify {
		assert!(ClaimExpiries::<T>::decode_len().unwrap_or_default() == 0);
	}

	// TODO: we need to manipulate the time to expire the claims
	// otherwise we didn't include the iteration in our benchmark
	on_initialize_worst_case {
		let b in 0 .. 150 as u32;
		let accounts = create_accounts::<T>(150);

		let eth_base_addr: EthereumAddress = [1u8; 20];
		let now = Duration::from_millis(100);
		for i in 0 .. b {
			// Stake some funds
			let staker = &accounts[i as usize];
			let eth_addr = eth_base_addr.map(|x| x + i as u8);
			let stake_call = Call::<T>::staked(staker.clone(), MIN_STAKE.into(), eth_addr, [0; 32]);
			stake_call.dispatch_bypass_filter(T::EnsureWitnessedByHistoricalActiveEpoch::successful_origin())?;
			// Submit a claim
			let claimable = T::Flip::claimable_balance(staker);
			Pallet::<T>::do_claim(staker, claimable, eth_addr)?;
			Pallet::<T>::register_claim_expiry(staker.clone(), now);
		}
	}: {
		Pallet::<T>::expire_pending_claims();
	}
}

impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
