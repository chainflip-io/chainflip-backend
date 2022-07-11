use crate::{mock::*, *};

const GOV_KEY_PROPOSAL: [u8; 32] = [1u8; 32];
const COMM_KEY_PROPOSAL: [u8; 32] = [1u8; 32];

#[test]
fn can_submit_a_proposal() {
    new_test_ext().execute_with(|| {
        assert_ok!(TokenholderGovernance::submit_proposal(Origin::signed(ALICE), Proposal::SetGovernanceKey(GOV_KEY_PROPOSAL)));
        assert!(Proposals::<T>::exists(<frame_system::Pallet<T>>::block_number() + VotingPeriod::<T>::get()));
    });
}