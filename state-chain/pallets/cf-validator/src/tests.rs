mod test {
	use crate::*;
	use crate::{mock::*, Error};
	use cf_traits::mocks::vault_rotation::clear_confirmation;
	use frame_support::{assert_noop, assert_ok};
	use sp_runtime::traits::{BadOrigin, Zero};

	const ALICE: u64 = 100;

	fn last_event() -> mock::Event {
		frame_system::Pallet::<Test>::events()
			.pop()
			.expect("Event expected")
			.event
	}

	fn assert_winners() -> Vec<ValidatorId> {
		assert_matches!(AuctionPallet::phase(), AuctionPhase::ValidatorsSelected(winners, _) => {
			winners
		})
	}

	#[test]
	fn you_have_to_be_priviledged() {
		new_test_ext().execute_with(|| {
			// Run through the sudo extrinsics to be sure they are what they are
			assert_noop!(
				ValidatorPallet::set_blocks_for_epoch(Origin::signed(ALICE), Zero::zero()),
				BadOrigin
			);
			assert_noop!(
				ValidatorPallet::force_rotation(Origin::signed(ALICE)),
				BadOrigin
			);
		});
	}

	#[test]
	fn changing_epoch() {
		new_test_ext().execute_with(|| {
			// Confirm we have a minimum epoch of 1 block
			assert_eq!(<Test as Config>::MinEpoch::get(), 1);
			// Throw up an error if we supply anything less than this
			assert_noop!(
				ValidatorPallet::set_blocks_for_epoch(Origin::root(), 0),
				Error::<Test>::InvalidEpoch
			);
			// This should work as 2 > 1
			assert_ok!(ValidatorPallet::set_blocks_for_epoch(Origin::root(), 2));
			// Confirm we have an event for the change from 0 to 2
			assert_eq!(
				last_event(),
				mock::Event::pallet_cf_validator(crate::Event::EpochDurationChanged(0, 2)),
			);
			// We throw up an error if we try to set it to the current
			assert_noop!(
				ValidatorPallet::set_blocks_for_epoch(Origin::root(), 2),
				Error::<Test>::InvalidEpoch
			);
		});
	}

	#[test]
	fn should_end_session() {
		new_test_ext().execute_with(|| {
			let set_size = 10;
			assert_ok!(AuctionPallet::set_active_range((2, set_size)));
			// Set block length of epoch to 10
			let epoch = 10;
			assert_ok!(ValidatorPallet::set_blocks_for_epoch(Origin::root(), epoch));
			// If we are in the bidder phase we should check if we have a force auction or
			// epoch has expired
			// Test force rotation
			assert_ok!(ValidatorPallet::force_rotation(Origin::root()));
			// Test we are in the bidder phase
			assert_matches!(AuctionPallet::phase(), AuctionPhase::WaitingForBids);
			// Move forward by 1 block, we have a block already
			run_to_block(2);
			assert_matches!(AuctionPallet::phase(), AuctionPhase::ValidatorsSelected(..));
			// Confirm the auction
			clear_confirmation();
			// Move forward by 1 block
			run_to_block(3);
			assert_matches!(AuctionPallet::phase(), AuctionPhase::WaitingForBids);
			// Move forward by 1 block, we should sit in the non-auction phase 'WaitingForBids'
			run_to_block(5);
			assert_matches!(AuctionPallet::phase(), AuctionPhase::WaitingForBids);
			// Epoch is block 10 so let's test an epoch cycle to provoke an auction
			// This should be the same state
			run_to_block(9);
			assert_matches!(AuctionPallet::phase(), AuctionPhase::WaitingForBids);
			run_to_block(10);
			// We should have started another auction
			assert_matches!(AuctionPallet::phase(), AuctionPhase::ValidatorsSelected(..));
			// Let's check we can't alter the state of the pallet during this period
			assert_noop!(
				ValidatorPallet::force_rotation(Origin::root()),
				Error::<Test>::AuctionInProgress
			);
			assert_noop!(
				ValidatorPallet::set_blocks_for_epoch(Origin::root(), 10),
				Error::<Test>::AuctionInProgress
			);
			// Finally back to the start again
			// Confirm the auction
			clear_confirmation();
			run_to_block(11);
			assert_matches!(AuctionPallet::phase(), AuctionPhase::WaitingForBids);
		});
	}

	#[test]
	fn rotation() {
		// We expect from our `DummyAuction` that we will have our bidders which are then
		// ran through an auction and that the winners of this auction become the validating set
		new_test_ext().execute_with(|| {
			let set_size = 10;
			assert_ok!(AuctionPallet::set_active_range((2, set_size)));
			// Set block length of epoch to 10
			let epoch = 10;
			assert_ok!(ValidatorPallet::set_blocks_for_epoch(Origin::root(), epoch));
			// At genesis we have 0 valdiators
			assert_eq!(mock::current_validators().len(), 0);
			// ---------- Run Auction
			// Confirm we are in the waiting state
			assert_matches!(AuctionPallet::phase(), AuctionPhase::WaitingForBids);
			// Move forward 2 blocks
			run_to_block(2);
			assert_matches!(AuctionPallet::phase(), AuctionPhase::WaitingForBids);
			// There are no validators as we are nice and fresh
			assert!(<ValidatorPallet as EpochInfo>::current_validators().is_empty());
			assert!(<ValidatorPallet as EpochInfo>::next_validators().is_empty());
			// Run to the epoch
			run_to_block(10);
			// We should have now completed an auction have a set of winners to pass as validators
			let winners = assert_winners();
			assert!(<ValidatorPallet as EpochInfo>::current_validators().is_empty());
			// and the winners are
			assert!(!<ValidatorPallet as EpochInfo>::next_validators().is_empty());
			// run more block to make them validators
			run_to_block(11);
			// Continue with our current validator set, as we had none should be empty
			// TODO add genesis validators to mock
			assert!(<ValidatorPallet as EpochInfo>::current_validators().is_empty());
			// We do now see our winners lined up to be the next set of validators
			assert_eq!(<ValidatorPallet as EpochInfo>::next_validators(), winners);
			// Complete the cycle
			run_to_block(12);
			// As we haven't confirmed the auction we would still be in the same phase
			assert_matches!(AuctionPallet::phase(), AuctionPhase::ValidatorsSelected(..));
			run_to_block(13);
			// and still...
			assert_matches!(AuctionPallet::phase(), AuctionPhase::ValidatorsSelected(..));
			// Confirm the auction
			clear_confirmation();
			run_to_block(14);
			assert_matches!(AuctionPallet::phase(), AuctionPhase::WaitingForBids);
			assert_eq!(<ValidatorPallet as EpochInfo>::epoch_index(), 1);
			// We do now see our winners as the set of validators
			assert_eq!(
				<ValidatorPallet as EpochInfo>::current_validators(),
				winners
			);
			// Our old winners remain
			assert_eq!(<ValidatorPallet as EpochInfo>::next_validators(), winners);
			// Force an auction at the next block
			assert_ok!(ValidatorPallet::force_rotation(Origin::root()));
			run_to_block(15);
			// A new auction starts
			// We should still see the old winners validating
			assert_eq!(
				<ValidatorPallet as EpochInfo>::current_validators(),
				winners
			);
			// Our new winners are
			// We should still see the old winners validating
			let winners = assert_winners();
			assert_eq!(<ValidatorPallet as EpochInfo>::next_validators(), winners);
			// Confirm the auction
			clear_confirmation();
			run_to_block(16);
			// Finalised auction, waiting for bids again
			assert_matches!(AuctionPallet::phase(), AuctionPhase::WaitingForBids);
			assert_eq!(<ValidatorPallet as EpochInfo>::epoch_index(), 2);
			// We have the new set of validators
			assert_eq!(
				<ValidatorPallet as EpochInfo>::current_validators(),
				winners
			);
		});
	}

	#[test]
	fn genesis() {
		new_test_ext().execute_with(|| {
			// We should have a set of 0 validators on genesis with a minimum bid of 0 set
			assert_eq!(
				current_validators().len(),
				0,
				"We shouldn't have a set of validators at genesis"
			);
			assert_eq!(min_bid(), 0, "We should have a minimum bid of zero");
		});
	}
}
