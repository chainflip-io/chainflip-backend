//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use cf_chains::{ApiCall, ChainCrypto, Ethereum};
use cf_primitives::AccountRole;
use cf_traits::AccountRoleRegistry;
use frame_benchmarking::{account, benchmarks, whitelisted_caller};
use frame_support::{
	dispatch::UnfilteredDispatchable,
	traits::{EnsureOrigin, OnInitialize},
};
use frame_system::RawOrigin;
use sp_std::vec::Vec;

use pallet_cf_account_types::EnsureValidator;

type AccountIdOf<T> = <T as frame_system::Config>::AccountId;
use cf_chains::benchmarking_value::BenchmarkValue;

fn create_accounts<T: Config>(count: u32) -> Vec<AccountIdOf<T>> {
	(0..=count).map(|i| account("doogle", i, 0)).collect()
}

benchmarks! {

	where_clause {
		where
			T: pallet_cf_account_types::Config,
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

	post_claim_signature {
		let withdrawal_address: EthereumAddress = [42u8; 20];

		let caller: T::AccountId = whitelisted_caller();

		// Stake some funds to claim
		Call::<T>::staked {
			account_id: caller.clone(),
			amount: MinimumStake::<T>::get(),
			withdrawal_address,
			tx_hash: [211u8; 32],
		}.dispatch_bypass_filter(T::EnsureWitnessed::successful_origin())?;

		// requests a signature. So it's in the AsyncResult::Pending state
		Pallet::<T>::claim(RawOrigin::Signed(caller.clone()).into(), ClaimAmount::Max, withdrawal_address)?;

		// inserts signature so it's in the AsyncResult::Ready state
		let signature_request_id = <T::ThresholdSigner as ThresholdSigner<Ethereum>>::RequestId::benchmark_value();
		T::ThresholdSigner::insert_signature(
			signature_request_id,
			<Ethereum as ChainCrypto>::ThresholdSignature::benchmark_value(),
		);

		let call = Call::<T>::post_claim_signature {
			account_id: caller.clone(),
			signature_request_id,
		};
	}: { call.dispatch_bypass_filter(T::EnsureThresholdSigned::successful_origin())? }
	verify {
		assert!(PendingClaims::<T>::get(&caller).expect("Should have claim for caller").is_signed());
		frame_system::Pallet::<T>::events().pop().expect("No event has been emitted from the post_claim_signature extrinsic");
	}

	// retire_account {
	// 	let caller: T::AccountId = whitelisted_caller();
	// 	ActiveBidder::<T>::insert(caller.clone(), true);
	// 	EnsureValidator::<T>::register_account(caller.clone());
	// }:_(RawOrigin::Signed(caller.clone()))
	// verify {
	// 	assert!(!ActiveBidder::<T>::get(caller));
	// }

	activate_account {
		let caller = EnsureValidator::<T>::successful_origin();
		// let account_id = caller.as_signed();
		// let call = Call::<T>::activate_account{};
	}:_(caller)
	verify {
		// assert!(ActiveBidder::<T>::get(caller));
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

		let origin = T::EnsureGovernance::successful_origin();
	} : { call.dispatch_bypass_filter(origin)? }
	verify {
		assert_eq!(MinimumStake::<T>::get(), MinimumStake::<T>::get());
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
}
