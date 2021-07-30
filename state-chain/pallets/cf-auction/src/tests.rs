mod test {
	use crate::mock::*;
	use crate::*;
	use frame_support::{assert_noop, assert_ok};

	fn last_event() -> mock::Event {
		frame_system::Pallet::<Test>::events()
			.pop()
			.expect("Event expected")
			.event
	}

	#[test]
	fn genesis() {
		new_test_ext().execute_with(|| {
			// We should have our genesis validators, which would have been provided by
			// `BidderProvider`
			assert_matches!(AuctionPallet::phase(), AuctionPhase::WaitingForBids(winners, min_bid)
				if winners == vec![MAX_BID.0, JOE_BID.0, LOW_BID.0] && min_bid == LOW_BID.1
			);
		});
	}
	#[test]
	fn run_through_phases() {
		new_test_ext().execute_with(|| {
			// Check we are in the bidders phase
			assert_matches!(AuctionPallet::phase(), AuctionPhase::WaitingForBids(..));
			// Now move to the next phase, this should be the BidsTaken phase
			assert_matches!(AuctionPallet::process(), Ok(AuctionPhase::BidsTaken(bidders)) if bidders == vec![LOW_BID, JOE_BID, MAX_BID]);
			// Read storage to confirm has been changed to BidsTaken
			assert_matches!(AuctionPallet::current_phase(), AuctionPhase::BidsTaken(bidders) if bidders == vec![LOW_BID, JOE_BID, MAX_BID]);
			// Having moved into the BidsTaken phase we should have our list of bidders filtered
			// Expecting the phase to change, a set of winners, the bidder list and a bond value set
			// to our min bid
			assert_matches!(AuctionPallet::process(), Ok(AuctionPhase::WinnersSelected(winners, min_bid))
				if winners == vec![MAX_BID.0, JOE_BID.0, LOW_BID.0] && min_bid == LOW_BID.1
			);
			assert_matches!(AuctionPallet::current_phase(), AuctionPhase::WinnersSelected(winners, min_bid)
				if winners == vec![MAX_BID.0, JOE_BID.0, LOW_BID.0] && min_bid == LOW_BID.1
			);
			assert_eq!(
				last_event(),
				mock::Event::pallet_cf_auction(crate::Event::AuctionCompleted(0, vec![MAX_BID.0, JOE_BID.0, LOW_BID.0])),
			);
			// Just leaves us to confirm this auction, if we try to process this we will get an error
			// until is confirmed
			assert_matches!(AuctionPallet::process(), Err(AuctionError::NotConfirmed));
			// Confirm the auction
			Test::set_awaiting_confirmation(false);
			// and finally we complete the process, clearing the bidders
			assert_matches!(AuctionPallet::process(), Ok(AuctionPhase::WaitingForBids(..)));
			assert_matches!(AuctionPallet::current_phase(), AuctionPhase::WaitingForBids(..));
		});
	}

	#[test]
	fn changing_range() {
		new_test_ext().execute_with(|| {
			// Assert our minimum is set to 2
			assert_eq!(<Test as Config>::MinAuctionSize::get(), 2);
			// Check we are throwing up an error when we send anything less than the minimum of 2
			assert_noop!(
				AuctionPallet::set_auction_size_range(Origin::root(), (0, 0)),
				Error::<Test>::InvalidRange
			);
			assert_noop!(
				AuctionPallet::set_auction_size_range(Origin::root(), (1, 2)),
				Error::<Test>::InvalidRange
			);
			// This should now work
			assert_ok!(AuctionPallet::set_auction_size_range(
				Origin::root(),
				(2, 100)
			));
			// Confirm we have an event
			assert_eq!(
				last_event(),
				mock::Event::pallet_cf_auction(crate::Event::AuctionRangeChanged(
					(MIN_AUCTION_SIZE, MAX_AUCTION_SIZE),
					(2, 100)
				)),
			);
			//
			// We throw up an error if we try to set it to the current
			assert_noop!(
				AuctionPallet::set_auction_size_range(Origin::root(), (2, 100)),
				Error::<Test>::InvalidRange
			);
		});
	}

	#[test]
	fn kill_them_all() {
		new_test_ext().execute_with(|| {
			// Create a test set of bidders
			BIDDER_SET.with(|l| *l.borrow_mut() = vec![LOW_BID, JOE_BID]);

			let auction_range = (2, 100);
			assert_ok!(AuctionPallet::set_auction_range(auction_range));
			assert_matches!(AuctionPallet::process(), Ok(AuctionPhase::BidsTaken(_)));
			assert_matches!(
				AuctionPallet::process(),
				Ok(AuctionPhase::WinnersSelected(..))
			);
			assert!(Test::awaiting_confirmation());
			assert_matches!(AuctionPallet::phase(), AuctionPhase::WinnersSelected(winners, min_bid)
				if !winners.is_empty() && min_bid > 0
			);
			// Kill it
			AuctionPallet::abort();
			assert_eq!(AuctionPallet::phase(), AuctionPhase::default());
		});
	}
}
