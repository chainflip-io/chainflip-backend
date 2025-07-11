// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

#![cfg(feature = "runtime-benchmarks")]

use super::*;

use cf_primitives::AccountRole;
use cf_traits::Chainflip;
use frame_benchmarking::v2::*;
use frame_support::{
	assert_ok,
	traits::{EnsureOrigin, UnfilteredDispatchable},
};
use frame_system::RawOrigin;

fn fund_with_minimum<T: Config>(account_id: &T::AccountId) {
	assert_ok!(Call::<T>::funded {
		account_id: account_id.clone(),
		amount: MinimumFunding::<T>::get(),
		funder: Default::default(),
		tx_hash: Default::default()
	}
	.dispatch_bypass_filter(T::EnsureWitnessed::try_successful_origin().unwrap()));
}

fn request_max_redemption<T: Config>(account_id: &T::AccountId) {
	assert_ok!(Call::<T>::redeem {
		amount: RedemptionAmount::Max,
		address: Default::default(),
		executor: Default::default(),
	}
	.dispatch_bypass_filter(RawOrigin::Signed(account_id.clone()).into()));
}

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn funded() {
		let caller: T::AccountId = whitelisted_caller();

		#[block]
		{
			fund_with_minimum::<T>(&caller);
		}

		assert_eq!(T::Flip::balance(&caller), MinimumFunding::<T>::get());
	}

	#[benchmark]
	fn redeem() {
		let caller: T::AccountId = whitelisted_caller();

		fund_with_minimum::<T>(&caller);

		#[extrinsic_call]
		redeem(
			RawOrigin::Signed(caller.clone()),
			RedemptionAmount::Max,
			Default::default(),
			Default::default(),
		);

		assert!(PendingRedemptions::<T>::contains_key(&caller));
	}

	#[benchmark]
	fn redeemed() {
		let caller: T::AccountId = whitelisted_caller();
		let origin = T::EnsureWitnessed::try_successful_origin().unwrap();

		fund_with_minimum::<T>(&caller);
		request_max_redemption::<T>(&caller);

		let call = Call::<T>::redeemed {
			account_id: caller.clone(),
			redeemed_amount: MinimumFunding::<T>::get(),
			tx_hash: Default::default(),
		};

		#[block]
		{
			assert_ok!(call.dispatch_bypass_filter(origin));
		}

		assert!(!PendingRedemptions::<T>::contains_key(&caller));
	}

	#[benchmark]
	fn redemption_expired() {
		let caller: T::AccountId = whitelisted_caller();
		let origin = T::EnsureWitnessed::try_successful_origin().unwrap();

		fund_with_minimum::<T>(&caller);
		request_max_redemption::<T>(&caller);

		let call =
			Call::<T>::redemption_expired { account_id: caller.clone(), block_number: 2u32.into() };

		#[block]
		{
			assert_ok!(call.dispatch_bypass_filter(origin));
		}

		assert!(!PendingRedemptions::<T>::contains_key(&caller));
	}

	#[benchmark]
	fn update_minimum_funding() {
		let call =
			Call::<T>::update_minimum_funding { minimum_funding: MinimumFunding::<T>::get() };

		let origin = <T as Chainflip>::EnsureGovernance::try_successful_origin().unwrap();

		#[block]
		{
			assert_ok!(call.dispatch_bypass_filter(origin));
		}

		assert_eq!(MinimumFunding::<T>::get(), MinimumFunding::<T>::get());
	}

	#[benchmark]
	fn update_redemption_tax() {
		let amount = 1u32.into();
		let call = Call::<T>::update_redemption_tax { amount };

		#[block]
		{
			assert_ok!(
				call.dispatch_bypass_filter(T::EnsureGovernance::try_successful_origin().unwrap())
			);
		}

		assert_eq!(crate::RedemptionTax::<T>::get(), amount);
	}

	#[benchmark]
	fn bind_redeem_address() {
		let caller: T::AccountId = whitelisted_caller();

		#[extrinsic_call]
		bind_redeem_address(RawOrigin::Signed(caller.clone()), Default::default());

		assert!(BoundRedeemAddress::<T>::contains_key(&caller));
	}

	#[benchmark]
	fn update_restricted_addresses(a: Linear<1, 100>, b: Linear<1, 100>, c: Linear<1, 100>) {
		for i in 0..c {
			let some_balance = FlipBalance::<T>::from(100_u32);
			let some_account: AccountId<T> = account("doogle", 0, i);
			let balances: BTreeMap<EthereumAddress, FlipBalance<T>> =
				BTreeMap::from([(Default::default(), some_balance)]);
			RestrictedBalances::<T>::insert(some_account, balances);
		}
		let call = Call::<T>::update_restricted_addresses {
			addresses_to_add: (1..a as u32).map(|_| Default::default()).collect::<Vec<_>>(),
			addresses_to_remove: (1..b as u32).map(|_| Default::default()).collect::<Vec<_>>(),
		};

		#[block]
		{
			assert_ok!(
				call.dispatch_bypass_filter(T::EnsureGovernance::try_successful_origin().unwrap())
			);
		}
	}

	#[benchmark]
	fn bind_executor_address() {
		let caller: T::AccountId = whitelisted_caller();

		#[extrinsic_call]
		bind_executor_address(RawOrigin::Signed(caller.clone()), Default::default());

		assert!(BoundExecutorAddress::<T>::contains_key(&caller));
	}

	#[benchmark]
	fn rebalance() {
		let validators = T::AccountRoleRegistry::generate_whitelisted_callers_with_role(
			AccountRole::Validator,
			2,
		)
		.unwrap();

		let sender = validators[0].clone();
		let recipient = validators[1].clone();

		fund_with_minimum::<T>(&sender);

		T::AccountRoleRegistry::deregister_as_validator(&sender).unwrap();

		#[extrinsic_call]
		rebalance(RawOrigin::Signed(sender), recipient, Default::default(), RedemptionAmount::Max);
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
}
