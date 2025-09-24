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
use cf_primitives::{FlipBalance, FLIPPERINOS_PER_FLIP};
use frame_benchmarking::v2::*;
use frame_support::{assert_ok, sp_runtime::BoundedVec};
use sp_std::vec;

fn create_funded_parent_account<T: Config>(
	initial_balance: FlipBalance,
) -> (T::AccountId, FlipBalance) {
	use cf_traits::FeePayment;

	let caller: T::AccountId = whitelisted_caller();
	let balance = initial_balance * FLIPPERINOS_PER_FLIP;

	// Mint some balance to the caller
	T::FeePayment::mint_to_account(&caller, balance.into());

	(caller, balance)
}

#[benchmarks(
    where
        <T as Config>::RuntimeCall: From<frame_system::Call<T>>
)]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn set_vanity_name() {
		let caller: T::AccountId = whitelisted_caller();
		let name = BoundedVec::try_from(str::repeat("x", 64).as_bytes().to_vec()).unwrap();

		#[extrinsic_call]
		set_vanity_name(RawOrigin::Signed(caller.clone()), name.clone());

		assert_eq!(VanityNames::<T>::get().get(&caller), Some(&name));
	}

	#[benchmark]
	fn spawn_sub_account() {
		let (caller, balance) = create_funded_parent_account::<T>(1000);
		#[extrinsic_call]
		spawn_sub_account(RawOrigin::Signed(caller.clone()), 1, (balance / 2).into());
	}

	#[benchmark]
	fn as_sub_account() {
		let (caller, balance) = create_funded_parent_account::<T>(1000);
		let call = Box::new(frame_system::Call::remark { remark: vec![] }.into());

		assert_ok!(Pallet::<T>::spawn_sub_account(
			RawOrigin::Signed(caller.clone()).into(),
			1,
			(balance / 2).into(),
		));

		#[extrinsic_call]
		as_sub_account(RawOrigin::Signed(caller), 1, call);
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
}
