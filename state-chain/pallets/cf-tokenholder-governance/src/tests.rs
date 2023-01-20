use crate::tests::TokenholderGovernance;
use cf_test_utilities::last_event;
use frame_support::{assert_noop, assert_ok};

use crate::{mock::*, *};

fn go_to_block(n: u64) {
	System::set_block_number(n);
	TokenholderGovernance::on_initialize(n);
}

const GOV_KEY_PROPOSAL: [u8; 32] = [1u8; 32];
const ANOTHER_GOV_KEY_PROPOSAL: [u8; 32] = [2u8; 32];

#[test]
fn update_gov_key_via_onchain_proposal() {
	new_test_ext().execute_with(|| {
		let proposal = Proposal::SetGovernanceKey(GOV_KEY_PROPOSAL);
		assert_ok!(TokenholderGovernance::submit_proposal(RuntimeOrigin::signed(ALICE), proposal));
		assert_eq!(
			last_event::<Test>(),
			crate::mock::RuntimeEvent::TokenholderGovernance(crate::Event::ProposalSubmitted {
				proposal
			}),
		);
		assert!(Proposals::<Test>::contains_key(
			<frame_system::Pallet<Test>>::block_number() +
				<mock::Test as Config>::VotingPeriod::get()
		));
		// Back the proposal to ensure threshold
		assert_ok!(TokenholderGovernance::back_proposal(RuntimeOrigin::signed(BOB), proposal));
		assert_ok!(TokenholderGovernance::back_proposal(RuntimeOrigin::signed(CHARLES), proposal));
		// Jump to the block in which we expect the proposal
		TokenholderGovernance::on_initialize(
			<frame_system::Pallet<Test>>::block_number() +
				<mock::Test as Config>::VotingPeriod::get(),
		);
		assert!(!Proposals::<Test>::contains_key(
			<frame_system::Pallet<Test>>::block_number() +
				<mock::Test as Config>::VotingPeriod::get()
		));
		assert_eq!(
			last_event::<Test>(),
			crate::mock::RuntimeEvent::TokenholderGovernance(crate::Event::ProposalPassed {
				proposal
			}),
		);
		// Expect the proposal to be moved to the enactment stage
		assert!(GovKeyUpdateAwaitingEnactment::<Test>::get().is_some());
		TokenholderGovernance::on_initialize(
			<frame_system::Pallet<Test>>::block_number() +
				<mock::Test as Config>::EnactmentDelay::get(),
		);
		assert!(GovKeyUpdateAwaitingEnactment::<Test>::get().is_none());
		assert_eq!(
			last_event::<Test>(),
			crate::mock::RuntimeEvent::TokenholderGovernance(crate::Event::ProposalEnacted {
				proposal
			}),
		);
	});
}

#[test]
fn fees_are_burned_on_successful_proposal() {
	new_test_ext().execute_with(|| {
		let balance_before = Flip::total_balance_of(&ALICE);
		assert_ok!(TokenholderGovernance::submit_proposal(
			RuntimeOrigin::signed(ALICE),
			Proposal::SetGovernanceKey(GOV_KEY_PROPOSAL)
		));
		assert_eq!(
			Flip::total_balance_of(&ALICE),
			balance_before - <mock::Test as Config>::ProposalFee::get()
		);
	});
}

#[test]
fn cannot_back_proposal_twice() {
	new_test_ext().execute_with(|| {
		assert_ok!(TokenholderGovernance::submit_proposal(
			RuntimeOrigin::signed(ALICE),
			Proposal::SetGovernanceKey(GOV_KEY_PROPOSAL)
		));
		assert_ok!(TokenholderGovernance::back_proposal(
			RuntimeOrigin::signed(BOB),
			Proposal::SetGovernanceKey(GOV_KEY_PROPOSAL)
		));
		assert_noop!(
			TokenholderGovernance::back_proposal(
				RuntimeOrigin::signed(BOB),
				Proposal::SetGovernanceKey(GOV_KEY_PROPOSAL)
			),
			Error::<Test>::AlreadyBacked
		);
	});
}

#[test]
fn cannot_back_not_existing_proposal() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			TokenholderGovernance::back_proposal(
				RuntimeOrigin::signed(BOB),
				Proposal::SetGovernanceKey(GOV_KEY_PROPOSAL)
			),
			Error::<Test>::ProposalDoesntExist
		);
	});
}

#[test]
fn cannot_create_proposal_with_insufficient_liquidity() {
	new_test_ext().execute_with(|| {
		let balance_before = Flip::total_balance_of(&BROKE_PAUL);
		assert_noop!(
			TokenholderGovernance::submit_proposal(
				RuntimeOrigin::signed(BROKE_PAUL),
				Proposal::SetGovernanceKey(GOV_KEY_PROPOSAL),
			),
			pallet_cf_flip::Error::<Test>::InsufficientLiquidity
		);
		assert_eq!(balance_before, Flip::total_balance_of(&BROKE_PAUL));
	});
}

#[test]
fn not_enough_backed_liquidity_for_proposal_enactment() {
	new_test_ext().execute_with(|| {
		assert_ok!(TokenholderGovernance::submit_proposal(
			RuntimeOrigin::signed(ALICE),
			Proposal::SetGovernanceKey(GOV_KEY_PROPOSAL)
		));
		TokenholderGovernance::on_initialize(
			<frame_system::Pallet<Test>>::block_number() +
				<mock::Test as Config>::VotingPeriod::get(),
		);
		assert!(!Proposals::<Test>::contains_key(
			<frame_system::Pallet<Test>>::block_number() +
				<mock::Test as Config>::VotingPeriod::get()
		));
		assert!(!Backers::<Test>::contains_key(Proposal::SetGovernanceKey(GOV_KEY_PROPOSAL)));
		assert!(GovKeyUpdateAwaitingEnactment::<Test>::get().is_none());
		assert_eq!(
			last_event::<Test>(),
			crate::mock::RuntimeEvent::TokenholderGovernance(crate::Event::ProposalRejected {
				proposal: Proposal::SetGovernanceKey(GOV_KEY_PROPOSAL)
			}),
		);
	});
}

#[test]
fn replace_proposal_during_enactment_period() {
	new_test_ext().execute_with(|| {
		fn create_and_back_proposal(proposal: Proposal<Test>) {
			assert_ok!(TokenholderGovernance::submit_proposal(
				RuntimeOrigin::signed(ALICE),
				proposal
			));
			assert_ok!(TokenholderGovernance::back_proposal(RuntimeOrigin::signed(BOB), proposal));
			assert_ok!(TokenholderGovernance::back_proposal(
				RuntimeOrigin::signed(CHARLES),
				proposal
			));
		}
		go_to_block(5);
		create_and_back_proposal(Proposal::SetGovernanceKey(GOV_KEY_PROPOSAL));
		go_to_block(15);
		assert_eq!(GovKeyUpdateAwaitingEnactment::<Test>::get().unwrap().1, GOV_KEY_PROPOSAL);
		create_and_back_proposal(Proposal::SetGovernanceKey(ANOTHER_GOV_KEY_PROPOSAL));
		go_to_block(25);
		assert_eq!(
			GovKeyUpdateAwaitingEnactment::<Test>::get().unwrap().1,
			ANOTHER_GOV_KEY_PROPOSAL
		);
	});
}
