//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use cf_primitives::AccountRole;
use cf_traits::{AccountRoleRegistry, Chainflip};
use frame_benchmarking::{benchmarks, whitelisted_caller};
use frame_support::{dispatch::UnfilteredDispatchable, traits::EnsureOrigin};
use frame_system::RawOrigin;

benchmarks! {

	staked {
		let amount: T::Balance = T::Balance::from(100u32);
		let withdrawal_address: EthereumAddress = [42u8; 20];
		let tx_hash: pallet::EthTransactionHash = [211u8; 32];
		let caller: T::AccountId = whitelisted_caller();

		let call = Call::<T>::staked {
			account_id: caller.clone(),
			amount,
			withdrawal_address,
			tx_hash,
		};
		let origin = T::EnsureWitnessed::successful_origin();

	}: { call.dispatch_bypass_filter(origin)? }
	verify {
		assert_eq!(T::Flip::staked_balance(&caller), amount);
	}

	claim {
		// If we claim an amount which takes us below the minimum stake, the claim
		// will fail.
		let balance_to_stake: T::Balance = MinimumStake::<T>::get() * T::Balance::from(2u128);
		let balance_to_claim = ClaimAmount::Exact(MinimumStake::<T>::get());
		let tx_hash: pallet::EthTransactionHash = [211u8; 32];
		let withdrawal_address: EthereumAddress = [42u8; 20];

		let caller: T::AccountId = whitelisted_caller();
		let origin = T::EnsureWitnessed::successful_origin();

		// Stake some funds to claim
		let stake_call = Call::<T>::staked {
			account_id: caller.clone(),
			amount: balance_to_stake,
			withdrawal_address,
			tx_hash
		};
		stake_call.dispatch_bypass_filter(origin)?;

	} :_(RawOrigin::Signed(caller.clone()), balance_to_claim, withdrawal_address)
	verify {
		assert!(PendingClaims::<T>::contains_key(&caller));
	}
	claim_all {
		let withdrawal_address: EthereumAddress = [42u8; 20];
		let caller: T::AccountId = whitelisted_caller();

		let balance_to_stake: T::Balance = MinimumStake::<T>::get();
		let tx_hash: pallet::EthTransactionHash = [211u8; 32];

		let caller: T::AccountId = whitelisted_caller();
		let origin = T::EnsureWitnessed::successful_origin();

		Call::<T>::staked {
			account_id: caller.clone(),
			amount: balance_to_stake,
			withdrawal_address,
			tx_hash
		}.dispatch_bypass_filter(origin)?;

		let call = Call::<T>::claim {
			amount: ClaimAmount::Max,
			address: withdrawal_address,
		};
	}: { call.dispatch_bypass_filter(RawOrigin::Signed(caller.clone()).into())? }
	verify {
		assert!(PendingClaims::<T>::contains_key(&caller));
	}

	claimed {
		let tx_hash: pallet::EthTransactionHash = [211u8; 32];
		let withdrawal_address: EthereumAddress = [42u8; 20];

		let caller: T::AccountId = whitelisted_caller();
		let origin = T::EnsureWitnessed::successful_origin();

		// Stake some funds to claim
		Call::<T>::staked {
			account_id: caller.clone(),
			amount: MinimumStake::<T>::get(),
			withdrawal_address,
			tx_hash
		}.dispatch_bypass_filter(origin.clone())?;

		// Push a claim
		let claimable = T::Flip::claimable_balance(&caller);
		Pallet::<T>::claim(RawOrigin::Signed(caller.clone()).into(), ClaimAmount::Max, withdrawal_address)?;

		let call = Call::<T>::claimed {
			account_id: caller.clone(),
			claimed_amount: claimable,
			tx_hash
		};

	}: { call.dispatch_bypass_filter(origin)? }
	verify {
		assert!(!PendingClaims::<T>::contains_key(&caller));
	}

	claim_expired {
		let tx_hash: pallet::EthTransactionHash = [211u8; 32];
		let withdrawal_address: EthereumAddress = [42u8; 20];

		let caller: T::AccountId = whitelisted_caller();
		let origin = T::EnsureWitnessed::successful_origin();

		// Stake some funds to claim
		Call::<T>::staked {
			account_id: caller.clone(),
			amount: MinimumStake::<T>::get(),
			withdrawal_address,
			tx_hash
		}.dispatch_bypass_filter(origin.clone())?;

		// Push a claim
		let claimable = T::Flip::claimable_balance(&caller);
		Pallet::<T>::claim(RawOrigin::Signed(caller.clone()).into(), ClaimAmount::Max, withdrawal_address)?;

		let call = Call::<T>::claim_expired {
			account_id: caller.clone(),
			block_number: 2u32.into(),
		};


	} : { call.dispatch_bypass_filter(origin)? }
	verify {
		assert!(!PendingClaims::<T>::contains_key(&caller));
	}

	stop_bidding {
		let caller: T::AccountId = whitelisted_caller();
		ActiveBidder::<T>::insert(caller.clone(), true);
		T::AccountRoleRegistry::register_account(caller.clone(), AccountRole::Validator);
	}:_(RawOrigin::Signed(caller.clone()))
	verify {
		assert!(!ActiveBidder::<T>::get(caller));
	}

	start_bidding {
		let caller: T::AccountId = whitelisted_caller();
		ActiveBidder::<T>::insert(caller.clone(), false);
		T::AccountRoleRegistry::register_account(caller.clone(), AccountRole::Validator);
	}:_(RawOrigin::Signed(caller.clone()))
	verify {
		assert!(ActiveBidder::<T>::get(caller));
	}
	update_minimum_stake {
		let call = Call::<T>::update_minimum_stake {
			minimum_stake: MinimumStake::<T>::get(),
		};

		let origin = <T as Chainflip>::EnsureGovernance::successful_origin();
	} : { call.dispatch_bypass_filter(origin)? }
	verify {
		assert_eq!(MinimumStake::<T>::get(), MinimumStake::<T>::get());
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
}
