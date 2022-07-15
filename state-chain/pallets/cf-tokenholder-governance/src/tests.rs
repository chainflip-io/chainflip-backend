use frame_support::{assert_noop, assert_ok};

use crate::{mock::*, *};

const GOV_KEY_PROPOSAL: [u8; 32] = [1u8; 32];
const COMM_KEY_PROPOSAL: [u8; 32] = [1u8; 32];

/// This is a more complex test scenraio in which we prove the following things:
/// - Submit a proposal
/// - Back a proposal
/// - Proof an proposal can not be backed several times
/// - ...
#[test]
fn update_gov_key_via_onchain_proposal() {
    new_test_ext().execute_with(|| {
        VotingPeriod::<Test>::set(10);
        EnactmentDelay::<Test>::set(20);
        assert_ok!(TokenholderGovernance::submit_proposal(Origin::signed(ALICE), Proposal::SetGovernanceKey(GOV_KEY_PROPOSAL)));
        assert!(Proposals::<Test>::contains_key(<frame_system::Pallet<Test>>::block_number() + VotingPeriod::<Test>::get()));
        // Try to back the same proposal with the inital proposer
        assert_noop!(TokenholderGovernance::back_proposal(Origin::signed(ALICE), Proposal::SetGovernanceKey(GOV_KEY_PROPOSAL)), Error::<Test>::AlreadyBacked);
        // Back the proposal to ensure threshold
        assert_ok!(TokenholderGovernance::back_proposal(Origin::signed(BOB), Proposal::SetGovernanceKey(GOV_KEY_PROPOSAL)));
        assert_ok!(TokenholderGovernance::back_proposal(Origin::signed(CHARLES), Proposal::SetGovernanceKey(GOV_KEY_PROPOSAL)));
        // Jump to the block in which we expect the proposal
        TokenholderGovernance::on_initialize(<frame_system::Pallet<Test>>::block_number() + VotingPeriod::<Test>::get());
        // Expect the proposal to be moved to the enactment stage
        assert!(GovKeyUpdateAwaitingEnactment::<Test>::get().is_some());
        TokenholderGovernance::on_initialize(<frame_system::Pallet<Test>>::block_number() + EnactmentDelay::<Test>::get());
        assert!(GovKeyUpdateAwaitingEnactment::<Test>::get().is_none());
    });
}