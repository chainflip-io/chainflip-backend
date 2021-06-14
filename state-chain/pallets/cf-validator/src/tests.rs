mod test {
	use crate::*;
	use crate::{Error, mock::*};
	use sp_runtime::traits::{BadOrigin, Zero};
	use frame_support::{assert_ok, assert_noop};

	const ALICE: u64 = 100;

	fn last_event() -> mock::Event {
		frame_system::Pallet::<Test>::events().pop().expect("Event expected").event
	}

	fn assert_winners() -> Vec<ValidatorId> {
		assert_matches!(AuctionPallet::phase(), AuctionPhase::WinnersSelected(winners, _) => {
			winners
		})
	}

	fn assert_sort(v1: &mut Vec<u64>, v2: &mut Vec<u64>) {
		v1.sort();
		v2.sort();
		assert_eq!(v1, v2)
	}

	#[test]
	fn you_have_to_be_priviledged() {
		new_test_ext().execute_with(|| {
			// Run through the sudo extrinsics to be sure they are what they are
			assert_noop!(ValidatorManager::set_blocks_for_epoch(Origin::signed(ALICE), Zero::zero()), BadOrigin);
			assert_noop!(ValidatorManager::force_rotation(Origin::signed(ALICE)), BadOrigin);
		});
	}

	#[test]
	fn changing_epoch() {
		new_test_ext().execute_with(|| {
			// Confirm we have a minimum epoch of 1 block
			assert_eq!(<Test as Config>::MinEpoch::get(), 1);
			// Throw up an error if we supply anything less than this
			assert_noop!(ValidatorManager::set_blocks_for_epoch(Origin::root(), 0), Error::<Test>::InvalidEpoch);
			// This should work as 2 > 1
			assert_ok!(ValidatorManager::set_blocks_for_epoch(Origin::root(), 2));
			// Confirm we have an event for the change from 0 to 2
			assert_eq!(
				last_event(),
				mock::Event::pallet_cf_validator(crate::Event::EpochDurationChanged(EPOCH_BLOCKS, 2)),
			);
			// We throw up an error if we try to set it to the current
			assert_noop!(ValidatorManager::set_blocks_for_epoch(Origin::root(), 2), Error::<Test>::InvalidEpoch);
		});
	}

	#[test]
	fn should_end_session() {
		new_test_ext().execute_with(|| {
			let set_size = 10;
			assert_ok!(AuctionPallet::set_auction_range((2, set_size)));
			// Set block length of epoch to 10
			let epoch = 10;
			assert_ok!(ValidatorManager::set_blocks_for_epoch(Origin::root(), epoch));
			// If we are in the bidder phase we should check if we have a force auction or
			// epoch has expired
			// Test force rotation
			assert_ok!(ValidatorManager::force_rotation(Origin::root()));
			// Test we are in the bidder phase
			assert_matches!(AuctionPallet::phase(), AuctionPhase::WaitingForBids(_, _));
			// Move forward by 1 block, we have a block already
			run_to_block(2);
			assert_matches!(AuctionPallet::phase(), AuctionPhase::WinnersSelected(_, _));
			// Confirm the auction
			CONFIRM.with(|l| { *l.borrow_mut() = true });
			// Move forward by 1 block
			run_to_block(3);
			assert_matches!(AuctionPallet::phase(), AuctionPhase::WaitingForBids(_, _));
			// Move forward by 1 block, we should sit in the non-auction phase 'WaitingForBids'
			run_to_block(5);
			assert_matches!(AuctionPallet::phase(), AuctionPhase::WaitingForBids(_, _));
			// Epoch is block 10 so let's test an epoch cycle to provoke an auction
			// This should be the same state
			run_to_block(9);
			assert_matches!(AuctionPallet::phase(), AuctionPhase::WaitingForBids(_, _));
			run_to_block(10);
			// We should have started another auction
			assert_matches!(AuctionPallet::phase(), AuctionPhase::WinnersSelected(_, _));
			// Let's check we can't alter the state of the pallet during this period
			assert_noop!(ValidatorManager::force_rotation(Origin::root()), Error::<Test>::AuctionInProgress);
			assert_noop!(ValidatorManager::set_blocks_for_epoch(Origin::root(), 10), Error::<Test>::AuctionInProgress);
			// Finally back to the start again
			run_to_block(11);
			assert_matches!(AuctionPallet::phase(), AuctionPhase::WaitingForBids(_, _));
		});
	}

	#[test]
	fn rotation() {
		// We expect from our `DummyAuction` that we will have our bidders which are then
		// ran through an auction and that the winners of this auction become the validating set
		new_test_ext().execute_with(|| {
			// Run past genesis and the force auction
			// Provide a confirmation when we run the auction
			CONFIRM.with(|l| { *l.borrow_mut() = true });
			run_to_block(3);
			CONFIRM.with(|l| { *l.borrow_mut() = false });
			let set_size = 10;
			assert_ok!(AuctionPallet::set_auction_range((2, set_size)));
			// Set block length of epoch to 10
			let epoch = 10;
			assert_ok!(ValidatorManager::set_blocks_for_epoch(Origin::root(), epoch));
			// Our genesis validators, hello.
			assert_sort(&mut <ValidatorManager as EpochInfo>::current_validators(),
					   &mut mock::genesis_validators());
			// ---------- Run Auction
			// Confirm we are in the waiting state
			assert_matches!(AuctionPallet::phase(), AuctionPhase::WaitingForBids(_, _));
			// Move forward 2 blocks
			run_to_block(5);
			assert_matches!(AuctionPallet::phase(), AuctionPhase::WaitingForBids(_, _));
			// We have 2 validators, the genesis set sitting in next
			assert_sort(&mut <ValidatorManager as EpochInfo>::next_validators(),
					   &mut mock::genesis_validators());
			// Run to the epoch
			run_to_block(10);
			// We should have now completed an auction have a set of winners to pass as validators
			let mut winners = assert_winners();
			// Our genesis validators are still validating the network
			assert_sort(&mut <ValidatorManager as EpochInfo>::current_validators(),
						&mut mock::genesis_validators());
			// and the winners are
			assert_sort(&mut <ValidatorManager as EpochInfo>::next_validators(), &mut winners);
			// run more block to make them validators
			run_to_block(11);
			// rotation won't happen until we have a confirmed auction, so the current set are still
			// the genesis bunch
			assert_sort(&mut <ValidatorManager as EpochInfo>::current_validators(),
						&mut mock::genesis_validators());
			// Confirm we are still in the same phase
			assert_matches!(AuctionPallet::phase(), AuctionPhase::WinnersSelected(_, _));
			run_to_block(12);
			// and still...
			assert_matches!(AuctionPallet::phase(), AuctionPhase::WinnersSelected(_, _));
			// Confirm the auction
			CONFIRM.with(|l| { *l.borrow_mut() = true });
			run_to_block(13);
			// A rotation has occurred
			assert_matches!(AuctionPallet::phase(), AuctionPhase::WaitingForBids(_, _));
			// Confirm our new epoch, we had one at genesis remember
			assert_eq!(<ValidatorManager as EpochInfo>::epoch_index(), 2);
			// We do now see our winners as the set of validators
			assert_sort(&mut <ValidatorManager as EpochInfo>::current_validators(), &mut winners);
			// Our old winners remain
			assert_sort(&mut <ValidatorManager as EpochInfo>::next_validators(), &mut winners);
			// Force an auction at the next block
			assert_ok!(ValidatorManager::force_rotation(Origin::root()));
			run_to_block(14);
			// A new auction starts
			// We should still see the old winners validating
			assert_sort(&mut <ValidatorManager as EpochInfo>::current_validators(), &mut winners);
			// Our new winners are
			// We should still see the old winners validating
			let mut winners = assert_winners();
			assert_sort(&mut <ValidatorManager as EpochInfo>::next_validators(), &mut winners);
			run_to_block(15);
			// Finalised auction, waiting for bids again
			assert_matches!(AuctionPallet::phase(), AuctionPhase::WaitingForBids(_, _));
			assert_eq!(<ValidatorManager as EpochInfo>::epoch_index(), 3);
			// We have the new set of validators
			assert_sort(&mut <ValidatorManager as EpochInfo>::current_validators(), &mut winners);
		});
	}

	#[test]
	fn genesis() {
		// As we are forcing an auction on genesis we should see an auction ran over block 1 and 2
		// Confirm we are in the waiting state
		new_test_ext().execute_with(|| {
			// Provide a confirmation when we run the auction
			CONFIRM.with(|l| { *l.borrow_mut() = true });
			run_to_block(2);
			// We should have ran through an auction and have a set of winners and a min bid
			assert_matches!(AuctionPallet::phase(), AuctionPhase::WinnersSelected(mut winners, min_bid) => {
				GENESIS_VALIDATORS.with(|genesis_validators| {
					let genesis_validators = &mut *genesis_validators.borrow_mut();
					genesis_validators.sort();
					winners.sort();
					assert_eq!(*genesis_validators, winners);
					assert_eq!(min_bid, 1);
				});
			});
			// Move to block 3 and we now have the new set, or rather the genesis set
			// Let's check we have our set validating
			run_to_block(3);
			assert_matches!(AuctionPallet::phase(), AuctionPhase::WaitingForBids(mut winners, min_bid) => {
				CURRENT_VALIDATORS.with(|current_validators| {
					let current_validators = &mut *current_validators.borrow_mut();
					current_validators.sort();
					winners.sort();
					assert_eq!(*current_validators, winners);
					assert_eq!(min_bid, 1);
				});
			});
			// The state continues as expected
			run_to_block(10);
			assert_matches!(AuctionPallet::phase(), AuctionPhase::WaitingForBids(..));
		});
	}
}
