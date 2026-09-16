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

use crate::council::MAX_MEMBERS;
use frame_benchmarking::v2::*;
use frame_support::{
	assert_ok,
	traits::{Get, OnInitialize, UnfilteredDispatchable},
};
use frame_system::RawOrigin;

/// A maximum-size council at `MAX_DEPTH`: `MAX_MEMBERS` voters in sub-groups. Also returns the
/// last voter, which every quorum check has to reach.
fn max_size_council<T: Config>(seed: u32) -> (Council<T::AccountId>, T::AccountId) {
	const GROUPS: u32 = 8;
	// Otherwise the division drops voters, including the returned last one.
	const _: () = assert!((MAX_MEMBERS as u32).is_multiple_of(GROUPS));
	let group_size = MAX_MEMBERS as u32 / GROUPS;
	let voter = |i: u32| account::<T::AccountId>("voter", i, seed);
	(
		Council::WeightedGroup {
			threshold: 1,
			members: (0..GROUPS)
				.map(|group| {
					(
						1,
						Council::simple_group(
							1,
							(0..group_size).map(|i| voter(group * group_size + i)),
						),
					)
				})
				.collect(),
		},
		voter(MAX_MEMBERS as u32 - 1),
	)
}

/// Gives each member the sufficient that genesis and `set_council` would, so removing
/// them later goes through account reaping, as in production.
fn install_council<T: Config>(council: Council<T::AccountId>) {
	for member in council.members() {
		frame_system::Pallet::<T>::inc_sufficients(&member);
	}
	<Members<T>>::put(council);
}

/// Submits a proposal to install `new_council`, registering its members as pending.
fn propose_new_council<T: Config>(new_council: &Council<T::AccountId>, proposer: T::AccountId) {
	assert_ok!(Call::<T>::propose_governance_extrinsic {
		call: Box::new(Call::<T>::set_council { new_council: new_council.clone() }.into()),
		execution: ExecutionMode::Automatic,
	}
	.dispatch_bypass_filter(RawOrigin::Signed(proposer).into()));
}

#[benchmarks]
mod benchmarks {
	use super::*;
	use sp_std::vec;

	#[benchmark]
	fn propose_governance_extrinsic() {
		let (council, caller) = max_size_council::<T>(0);
		<Members<T>>::put(council);
		// Worst case: a disjoint council, so every incoming member is registered as pending
		// and given an account reference to sign their approval with.
		let call: Box<<T as Config>::RuntimeCall> =
			Box::new(Call::<T>::set_council { new_council: max_size_council::<T>(1).0 }.into());

		#[extrinsic_call]
		propose_governance_extrinsic(RawOrigin::Signed(caller), call, ExecutionMode::Automatic);

		assert_eq!(ProposalIdCounter::<T>::get(), 1);
		assert_eq!(PendingMembers::<T>::get(1).len(), MAX_MEMBERS);
	}

	#[benchmark]
	fn approve() {
		let (council, caller) = max_size_council::<T>(0);
		let (new_council, _) = max_size_council::<T>(1);
		install_council::<T>(council);
		// Worst case: this approval resolves the proposal, so it also releases the account
		// reference held for each incoming member.
		propose_new_council::<T>(&new_council, account::<T::AccountId>("voter", 0, 0));
		Proposals::<T>::mutate(1, |proposal| {
			if let Some(proposal) = proposal {
				proposal.approved.extend(new_council.members());
			}
		});

		#[extrinsic_call]
		approve(RawOrigin::Signed(caller), 1);

		assert_eq!(ExecutionPipeline::<T>::decode_len(), Some(1));
	}

	#[benchmark]
	fn set_council() {
		install_council::<T>(max_size_council::<T>(0).0);
		let (new_council, _) = max_size_council::<T>(1);
		let call = Call::<T>::set_council { new_council: new_council.clone() };
		let origin = T::EnsureGovernance::try_successful_origin().unwrap();

		#[block]
		{
			assert_ok!(call.dispatch_bypass_filter(origin));
		}

		assert_eq!(Members::<T>::get(), new_council);
	}

	#[benchmark]
	fn call_as_sudo() {
		let call: <T as Config>::RuntimeCall =
			frame_system::Call::set_code_without_checks { code: vec![1, 2, 3, 4] }.into();
		let sudo_call = Call::<T>::call_as_sudo { call: Box::new(call) };
		let origin = T::EnsureGovernance::try_successful_origin().unwrap();

		#[block]
		{
			assert_ok!(sudo_call.dispatch_bypass_filter(origin));
		}
	}

	#[benchmark]
	// Benchmarks the weight of Partitioning expired proposal.
	fn on_initialize(b: Linear<1, 100>) {
		for _n in 1..b {
			let call = Box::new(frame_system::Call::remark { remark: vec![] }.into());
			Pallet::<T>::push_proposal(call, ExecutionMode::Automatic);
		}
		#[block]
		{
			Pallet::<T>::on_initialize(2u32.into());
		}
	}

	#[benchmark]
	fn on_initialize_best_case() {
		#[block]
		{
			Pallet::<T>::on_initialize(2u32.into());
		}
	}

	#[benchmark]
	fn expire_proposals(b: Linear<1, 100>) {
		let (council, proposer) = max_size_council::<T>(0);
		install_council::<T>(council);
		// Worst case: each expiring proposal releases a full set of incoming members.
		let (new_council, _) = max_size_council::<T>(1);
		for _ in 1..b {
			propose_new_council::<T>(&new_council, proposer.clone());
		}

		#[block]
		{
			Pallet::<T>::expire_proposals(<ActiveProposals<T>>::get());
		}
	}

	#[benchmark]
	fn set_whitelisted_call_hash() {
		let call_hash = [0xb; 32];

		let call = Call::<T>::set_whitelisted_call_hash { call_hash };

		#[block]
		{
			assert_ok!(call.dispatch_bypass_filter(
				T::EnsureWitnessedAtCurrentEpoch::try_successful_origin().unwrap()
			));
		}

		assert_eq!(GovKeyWhitelistedCallHash::<T>::get().unwrap(), call_hash);
	}

	#[benchmark]
	fn submit_govkey_call() {
		let next_nonce = 788;
		NextGovKeyCallHashNonce::<T>::put(next_nonce);

		let set_council_call: <T as Config>::RuntimeCall = Call::<T>::set_council {
			new_council: Council::Individual { id: account::<T::AccountId>("voter", 0, 0) },
		}
		.into();

		let call_hash = frame_support::Hashable::blake2_256(&(
			set_council_call.clone(),
			next_nonce,
			T::Version::get(),
		));

		GovKeyWhitelistedCallHash::<T>::put(call_hash);

		let call = Call::<T>::submit_govkey_call { call: Box::new(set_council_call) };

		#[block]
		{
			assert_ok!(
				call.dispatch_bypass_filter(T::EnsureGovernance::try_successful_origin().unwrap())
			);
		}

		assert_eq!(NextGovKeyCallHashNonce::<T>::get(), next_nonce + 1);
		assert!(GovKeyWhitelistedCallHash::<T>::get().is_none());
	}

	#[benchmark]
	fn dispatch_whitelisted_call() {
		let (council, caller) = max_size_council::<T>(0);
		install_council::<T>(council);
		let call: <T as Config>::RuntimeCall =
			Call::<T>::set_council { new_council: Council::Individual { id: caller.clone() } }
				.into();
		Pallet::<T>::push_proposal(Box::new(call.clone()), ExecutionMode::Manual);
		PreAuthorisedGovCalls::<T>::insert(1, call.encode());

		#[extrinsic_call]
		dispatch_whitelisted_call(RawOrigin::Signed(caller.clone()), 1);
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test,);
}
