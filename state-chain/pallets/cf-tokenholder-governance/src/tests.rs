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

use crate::tests::TokenholderGovernance;
use cf_test_utilities::last_event;
use frame_support::{assert_noop, assert_ok};

use crate::{mock::*, *};

fn go_to_block(n: u64) {
	System::set_block_number(n);
	TokenholderGovernance::on_initialize(n);
}

fn awaiting_gov_key() -> Vec<u8> {
	let (_, (_, awaiting_key)) = GovKeyUpdateAwaitingEnactment::<Test>::get().unwrap();
	awaiting_key
}

fn submit_and_pass_proposal(proposal: Proposal) {
	assert_ok!(TokenholderGovernance::submit_proposal(
		RuntimeOrigin::signed(ALICE),
		proposal.clone()
	));
	assert_eq!(
		last_event::<Test>(),
		mock::RuntimeEvent::TokenholderGovernance(Event::ProposalSubmitted {
			proposal: proposal.clone()
		}),
	);

	let proposal_decision_block =
		<frame_system::Pallet<Test>>::block_number() + <mock::Test as Config>::VotingPeriod::get();

	assert_eq!(Proposals::<Test>::get(proposal_decision_block), Some(proposal.clone()));
	// Back the proposal to ensure threshold
	assert_ok!(TokenholderGovernance::back_proposal(RuntimeOrigin::signed(BOB), proposal.clone()));
	assert_ok!(TokenholderGovernance::back_proposal(
		RuntimeOrigin::signed(CHARLES),
		proposal.clone()
	));
	// Jump to the block in which we expect the proposal to pass
	TokenholderGovernance::on_initialize(proposal_decision_block);
	assert!(!Proposals::<Test>::contains_key(proposal_decision_block));
	assert_eq!(
		last_event::<Test>(),
		mock::RuntimeEvent::TokenholderGovernance(Event::ProposalPassed {
			proposal: proposal.clone()
		}),
	);
}

#[test]
fn update_gov_key_via_onchain_proposal() {
	new_test_ext().execute_with(|| {
		let new_key = vec![1; 32];

		// Initially no key is set
		assert_eq!(GovKeys::<Test>::get(ForeignChain::Ethereum), None);

		let proposal = Proposal::SetGovernanceKey(ForeignChain::Ethereum, new_key.clone());
		submit_and_pass_proposal(proposal.clone());
		// Expect the proposal to be moved to the enactment stage
		assert!(GovKeyUpdateAwaitingEnactment::<Test>::get().is_some());
		TokenholderGovernance::on_initialize(
			<frame_system::Pallet<Test>>::block_number() +
				<mock::Test as Config>::EnactmentDelay::get(),
		);
		assert!(GovKeyUpdateAwaitingEnactment::<Test>::get().is_none());
		assert_eq!(
			last_event::<Test>(),
			mock::RuntimeEvent::TokenholderGovernance(Event::ProposalEnacted { proposal }),
		);

		// The key should now be set
		assert_eq!(GovKeys::<Test>::get(ForeignChain::Ethereum), Some(new_key));
	});
}

#[test]
fn update_community_key_via_onchain_proposal() {
	new_test_ext().execute_with(|| {
		let proposal = Proposal::SetCommunityKey(sp_core::H160::from_slice(&[1; 20]));
		submit_and_pass_proposal(proposal.clone());
		// Expect the proposal to be moved to the enactment stage
		assert!(CommKeyUpdateAwaitingEnactment::<Test>::get().is_some());
		TokenholderGovernance::on_initialize(
			<frame_system::Pallet<Test>>::block_number() +
				<mock::Test as Config>::EnactmentDelay::get(),
		);
		assert!(CommKeyUpdateAwaitingEnactment::<Test>::get().is_none());
		assert_eq!(
			last_event::<Test>(),
			mock::RuntimeEvent::TokenholderGovernance(Event::ProposalEnacted { proposal }),
		);
	});
}

#[test]
fn fees_are_burned_on_successful_proposal() {
	new_test_ext().execute_with(|| {
		let gov_key_proposal = Proposal::SetGovernanceKey(ForeignChain::Ethereum, vec![1; 32]);
		let balance_before = <Test as Chainflip>::FundingInfo::total_balance_of(&ALICE);
		assert_ok!(TokenholderGovernance::submit_proposal(
			RuntimeOrigin::signed(ALICE),
			gov_key_proposal
		));
		assert_eq!(
			<Test as Chainflip>::FundingInfo::total_balance_of(&ALICE),
			balance_before - <mock::Test as Config>::ProposalFee::get()
		);
	});
}

#[test]
fn cannot_back_proposal_twice() {
	new_test_ext().execute_with(|| {
		let gov_key_proposal = Proposal::SetGovernanceKey(ForeignChain::Ethereum, vec![1; 32]);

		assert_ok!(TokenholderGovernance::submit_proposal(
			RuntimeOrigin::signed(ALICE),
			gov_key_proposal.clone()
		));
		assert_ok!(TokenholderGovernance::back_proposal(
			RuntimeOrigin::signed(BOB),
			gov_key_proposal.clone()
		));
		assert_noop!(
			TokenholderGovernance::back_proposal(RuntimeOrigin::signed(BOB), gov_key_proposal),
			Error::<Test>::AlreadyBacked
		);
	});
}

#[test]
fn cannot_back_not_existing_proposal() {
	new_test_ext().execute_with(|| {
		let gov_key_proposal = Proposal::SetGovernanceKey(ForeignChain::Ethereum, vec![1; 32]);

		assert_noop!(
			TokenholderGovernance::back_proposal(RuntimeOrigin::signed(BOB), gov_key_proposal),
			Error::<Test>::ProposalDoesntExist
		);
	});
}

#[test]
fn cannot_create_proposal_with_insufficient_liquidity() {
	new_test_ext().execute_with(|| {
		let gov_key_proposal = Proposal::SetGovernanceKey(ForeignChain::Ethereum, vec![1; 32]);

		let balance_before = <Test as Chainflip>::FundingInfo::total_balance_of(&BROKE_PAUL);
		assert_noop!(
			TokenholderGovernance::submit_proposal(
				RuntimeOrigin::signed(BROKE_PAUL),
				gov_key_proposal,
			),
			cf_traits::mocks::fee_payment::ERROR_INSUFFICIENT_LIQUIDITY
		);
		assert_eq!(balance_before, <Test as Chainflip>::FundingInfo::total_balance_of(&BROKE_PAUL));
	});
}

#[test]
fn not_enough_backed_liquidity_for_proposal_enactment() {
	new_test_ext().execute_with(|| {
		let proposed_key = vec![1; 32];
		let proposal = Proposal::SetGovernanceKey(ForeignChain::Ethereum, proposed_key);

		assert_ok!(TokenholderGovernance::submit_proposal(
			RuntimeOrigin::signed(ALICE),
			proposal.clone()
		));
		TokenholderGovernance::on_initialize(
			<frame_system::Pallet<Test>>::block_number() +
				<mock::Test as Config>::VotingPeriod::get(),
		);
		assert!(!Proposals::<Test>::contains_key(
			<frame_system::Pallet<Test>>::block_number() +
				<mock::Test as Config>::VotingPeriod::get()
		));
		assert!(!Backers::<Test>::contains_key(&proposal));
		assert!(GovKeyUpdateAwaitingEnactment::<Test>::get().is_none());
		assert_eq!(
			last_event::<Test>(),
			mock::RuntimeEvent::TokenholderGovernance(Event::ProposalRejected { proposal }),
		);
	});
}

#[test]
fn replace_proposal_during_enactment_period() {
	new_test_ext().execute_with(|| {
		let proposed_key_1 = vec![1; 32];
		let proposed_key_2 = vec![2; 32];
		fn create_and_back_proposal(proposal: Proposal) {
			assert_ok!(TokenholderGovernance::submit_proposal(
				RuntimeOrigin::signed(ALICE),
				proposal.clone()
			));
			assert_ok!(TokenholderGovernance::back_proposal(
				RuntimeOrigin::signed(BOB),
				proposal.clone()
			));
			assert_ok!(TokenholderGovernance::back_proposal(
				RuntimeOrigin::signed(CHARLES),
				proposal
			));
		}
		go_to_block(5);
		create_and_back_proposal(Proposal::SetGovernanceKey(
			ForeignChain::Ethereum,
			proposed_key_1.clone(),
		));
		go_to_block(15);
		assert_eq!(awaiting_gov_key(), proposed_key_1);
		create_and_back_proposal(Proposal::SetGovernanceKey(
			ForeignChain::Ethereum,
			proposed_key_2.clone(),
		));
		go_to_block(25);
		assert_eq!(awaiting_gov_key(), proposed_key_2);
	});
}

#[test]
fn incompatible_gov_key_is_noop() {
	new_test_ext().execute_with(|| {
		MockBroadcaster::set_behaviour(MockBroadcasterBehaviour {
			key_compatible: false,
			..Default::default()
		});
		assert_noop!(
			TokenholderGovernance::submit_proposal(
				RuntimeOrigin::signed(ALICE),
				Proposal::SetGovernanceKey(ForeignChain::Ethereum, Default::default()),
			),
			Error::<Test>::IncompatibleGovkey
		);
	});
}

#[test]
fn govkey_broadcast_to_correct_chain() {
	new_test_ext().execute_with(|| {
		let gov_key = b"SO_IMPORTANT".to_vec();
		GovKeyUpdateAwaitingEnactment::<Test>::put((1, (ForeignChain::Polkadot, gov_key.clone())));
		TokenholderGovernance::on_initialize(1);
		assert_eq!(
			MockBroadcaster::broadcasted_gov_key().unwrap(),
			(ForeignChain::Polkadot, None, gov_key.clone(),)
		);
		GovKeyUpdateAwaitingEnactment::<Test>::put((1, (ForeignChain::Ethereum, gov_key.clone())));
		TokenholderGovernance::on_initialize(1);
		assert_eq!(
			MockBroadcaster::broadcasted_gov_key().unwrap(),
			(ForeignChain::Ethereum, None, gov_key,)
		);
	});
}
