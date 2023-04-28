//! Benchmarking setup for pallet-template
#![cfg(feature = "runtime-benchmarks")]

use super::*;

use cf_traits::{AccountRoleRegistry, Chainflip};
use frame_benchmarking::{benchmarks, whitelisted_caller};
use frame_support::{
	dispatch::UnfilteredDispatchable,
	traits::{EnsureOrigin, OnNewAccount},
};
use frame_system::RawOrigin;

benchmarks! {

	funded {
		let amount: T::Balance = T::Balance::from(100u32);
		let withdrawal_address: EthereumAddress = [42u8; 20];
		let tx_hash: pallet::EthTransactionHash = [211u8; 32];
		let caller: T::AccountId = whitelisted_caller();

		let call = Call::<T>::funded {
			account_id: caller.clone(),
			amount,
			withdrawal_address,
			tx_hash,
		};
		let origin = T::EnsureWitnessed::successful_origin();

	}: { call.dispatch_bypass_filter(origin)? }
	verify {
		assert_eq!(T::Flip::account_balance(&caller), amount);
	}

	redeem {
		// If we redeem an amount which takes us below the minimum balance, the redemption
		// will fail.
		let balance_to_redeem = RedemptionAmount::Exact(MinimumFunding::<T>::get());
		let tx_hash: pallet::EthTransactionHash = [211u8; 32];
		let withdrawal_address: EthereumAddress = [42u8; 20];

		let caller: T::AccountId = whitelisted_caller();
		let origin = T::EnsureWitnessed::successful_origin();

		let call = Call::<T>::funded {
			account_id: caller.clone(),
			amount: MinimumFunding::<T>::get() * T::Balance::from(2u128),
			withdrawal_address,
			tx_hash
		};
		call.dispatch_bypass_filter(origin)?;

	} :_(RawOrigin::Signed(caller.clone()), balance_to_redeem, withdrawal_address)
	verify {
		assert!(PendingRedemptions::<T>::contains_key(&caller));
	}
	redeem_all {
		let withdrawal_address: EthereumAddress = [42u8; 20];
		let caller: T::AccountId = whitelisted_caller();

		let tx_hash: pallet::EthTransactionHash = [211u8; 32];

		let caller: T::AccountId = whitelisted_caller();
		let origin = T::EnsureWitnessed::successful_origin();

		Call::<T>::funded {
			account_id: caller.clone(),
			amount: MinimumFunding::<T>::get(),
			withdrawal_address,
			tx_hash
		}.dispatch_bypass_filter(origin)?;

		let call = Call::<T>::redeem {
			amount: RedemptionAmount::Max,
			address: withdrawal_address,
		};
	}: { call.dispatch_bypass_filter(RawOrigin::Signed(caller.clone()).into())? }
	verify {
		assert!(PendingRedemptions::<T>::contains_key(&caller));
	}

	redeemed {
		let tx_hash: pallet::EthTransactionHash = [211u8; 32];
		let withdrawal_address: EthereumAddress = [42u8; 20];

		let caller: T::AccountId = whitelisted_caller();
		let origin = T::EnsureWitnessed::successful_origin();

		Call::<T>::funded {
			account_id: caller.clone(),
			amount: MinimumFunding::<T>::get(),
			withdrawal_address,
			tx_hash
		}.dispatch_bypass_filter(origin.clone())?;

		// Push a redemption
		let redeemable = T::Flip::redeemable_balance(&caller);
		Pallet::<T>::redeem(RawOrigin::Signed(caller.clone()).into(), RedemptionAmount::Max, withdrawal_address)?;

		let call = Call::<T>::redeemed {
			account_id: caller.clone(),
			redeemed_amount: redeemable,
			tx_hash
		};

	}: { call.dispatch_bypass_filter(origin)? }
	verify {
		assert!(!PendingRedemptions::<T>::contains_key(&caller));
	}

	redemption_expired {
		let tx_hash: pallet::EthTransactionHash = [211u8; 32];
		let withdrawal_address: EthereumAddress = [42u8; 20];

		let caller: T::AccountId = whitelisted_caller();
		let origin = T::EnsureWitnessed::successful_origin();

		Call::<T>::funded {
			account_id: caller.clone(),
			amount: MinimumFunding::<T>::get(),
			withdrawal_address,
			tx_hash
		}.dispatch_bypass_filter(origin.clone())?;

		// Push a redemption
		let redeemable = T::Flip::redeemable_balance(&caller);
		Pallet::<T>::redeem(RawOrigin::Signed(caller.clone()).into(), RedemptionAmount::Max, withdrawal_address)?;

		let call = Call::<T>::redemption_expired {
			account_id: caller.clone(),
			block_number: 2u32.into(),
		};


	} : { call.dispatch_bypass_filter(origin)? }
	verify {
		assert!(!PendingRedemptions::<T>::contains_key(&caller));
	}

	stop_bidding {
		let caller: T::AccountId = whitelisted_caller();
		<T as frame_system::Config>::OnNewAccount::on_new_account(&caller);
		T::AccountRoleRegistry::register_as_validator(&caller).unwrap();
		ActiveBidder::<T>::insert(caller.clone(), true);
	}:_(RawOrigin::Signed(caller.clone()))
	verify {
		assert!(!ActiveBidder::<T>::get(caller));
	}

	start_bidding {
		let caller: T::AccountId = whitelisted_caller();
		<T as frame_system::Config>::OnNewAccount::on_new_account(&caller);
		T::AccountRoleRegistry::register_as_validator(&caller).unwrap();
		ActiveBidder::<T>::insert(caller.clone(), false);
	}:_(RawOrigin::Signed(caller.clone()))
	verify {
		assert!(ActiveBidder::<T>::get(caller));
	}
	update_minimum_funding {
		let call = Call::<T>::update_minimum_funding {
			minimum_funding: MinimumFunding::<T>::get(),
		};

		let origin = <T as Chainflip>::EnsureGovernance::successful_origin();
	} : { call.dispatch_bypass_filter(origin)? }
	verify {
		assert_eq!(MinimumFunding::<T>::get(), MinimumFunding::<T>::get());
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
}
