//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use cf_primitives::AccountRole;
use cf_traits::AccountRoleRegistry;
use frame_benchmarking::{account, benchmarks, whitelisted_caller};
use frame_support::{
	dispatch::UnfilteredDispatchable,
	traits::{EnsureOrigin, OnInitialize},
};
use frame_system::RawOrigin;
use sp_std::vec::Vec;

type AccountIdOf<T> = <T as frame_system::Config>::AccountId;

fn create_accounts<T: Config>(count: u32) -> Vec<AccountIdOf<T>> {
	(0..=count).map(|i| account("doogle", i, 0)).collect()
}

benchmarks! {

	where_clause {
		where
			T: pallet_cf_account_roles::Config,
	}

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

	retire_account {
		let caller: T::AccountId = whitelisted_caller();
		ActiveBidder::<T>::insert(caller.clone(), true);
		T::AccountRoleRegistry::register_account(caller.clone(), AccountRole::Validator);
	}:_(RawOrigin::Signed(caller.clone()))
	verify {
		assert!(!ActiveBidder::<T>::get(caller));
	}

	activate_account {
		let caller: T::AccountId = whitelisted_caller();
		ActiveBidder::<T>::insert(caller.clone(), false);
		T::AccountRoleRegistry::register_account(caller.clone(), AccountRole::Validator);
	}:_(RawOrigin::Signed(caller.clone()))
	verify {
		assert!(ActiveBidder::<T>::get(caller));
	}

	on_initialize_best_case {
	}: {
		Pallet::<T>::on_initialize((2u32).into());
	}
	verify {
		assert!(ClaimExpiries::<T>::decode_len().unwrap_or_default() == 0);
	}
	expire_pending_claims_at {
		let b in 0 .. 150_u32;
		let accounts = create_accounts::<T>(150);

		let eth_base_addr: EthereumAddress = [1u8; 20];
		for i in 0 .. b {
			// Stake some funds
			let staker = &accounts[i as usize];
			let withdrawal_address = eth_base_addr.map(|x| x + i as u8);
			Call::<T>::staked {
				account_id: staker.clone(),
				amount: MinimumStake::<T>::get(),
				withdrawal_address,
				tx_hash: [0; 32]
			}.dispatch_bypass_filter(T::EnsureWitnessed::successful_origin())?;
			Pallet::<T>::claim(RawOrigin::Signed(staker.clone()).into(), ClaimAmount::Max, withdrawal_address)?;

			// we're registering the claim to be expired at the unix epoch.
			// T::TimeSource::now().as_secs() evaulates to 0 in the benchmarks. So this ensures
			// we hit the most expensive case, all possible expiries expiring.
			Pallet::<T>::register_claim_expiry(staker.clone(), 0);
		}
	}: {
		Pallet::<T>::expire_pending_claims_at(u64::MAX);
	}
	update_minimum_stake {
		let call = Call::<T>::update_minimum_stake {
			minimum_stake: MinimumStake::<T>::get(),
		};

		let origin = <T as pallet::Config>::EnsureGovernance::successful_origin();
	} : { call.dispatch_bypass_filter(origin)? }
	verify {
		assert_eq!(MinimumStake::<T>::get(), MinimumStake::<T>::get());
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
}
