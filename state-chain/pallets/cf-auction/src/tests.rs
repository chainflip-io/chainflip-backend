mod test {
	use crate::*;
	use crate::{mock::*};
	use frame_support::{assert_ok, assert_noop};

	fn last_event() -> mock::Event {
		frame_system::Pallet::<Test>::events().pop().expect("Event expected").event
	}

	#[test]
	fn run_through_phases() {
		new_test_ext().execute_with(|| {
			// Create a test set of bidders
			let invalid_bid = (1, 0);
			let low_bid = (2, 2);
			let joe_bid = (3, 100);
			let max_bid = (4, 101);
			BIDDER_SET.with(|l| {
				*l.borrow_mut() = vec![invalid_bid, low_bid, joe_bid, max_bid]
			});

			let auction_range = (2, 100);
			// Check we are in the bidders phase
			assert_eq!(AuctionPallet::current_phase(), AuctionPhase::default());
			// Now move to the next phase, this should be the BidsTaken phase
			assert_matches!(AuctionPallet::process(),
				Ok(AuctionPhase::BidsTaken(bidders)) if bidders == vec![low_bid, joe_bid, max_bid]);
			// Read storage to confirm has been changed to BidsTaken
			assert_matches!(AuctionPallet::current_phase(),
				AuctionPhase::BidsTaken(bidders) if bidders == vec![low_bid, joe_bid, max_bid]);
			// Having moved into the BidsTaken phase we should have our list of bidders filtered
			// And again to the next phase, however we should see an error as we haven't set our
			// auction size as an `AuctionError::Empty`
			assert_eq!(AuctionPallet::process(), Err(AuctionError::Empty));
			// In order to move forward we will need to set our auction set size
			// First test the call failing, range would have a 0 value or have equal values for min and max
			assert_eq!(AuctionPallet::set_auction_range((0, 0)), Err(AuctionError::InvalidRange));
			assert_eq!(AuctionPallet::set_auction_range((1, 1)), Err(AuctionError::InvalidRange));
			assert_ok!(AuctionPallet::set_auction_range(auction_range));
			// Check storage for auction range
			assert_eq!(AuctionPallet::auction_size_range(), auction_range);
			// With that sorted we would move on to completing the auction
			// Expecting the phase to change, a set of winners, the bidder list and a bond value set
			// to our min bid
			assert_matches!(AuctionPallet::process(), Ok(AuctionPhase::WinnersSelected(winners, min_bid))
				if winners == vec![max_bid.0, joe_bid.0, low_bid.0] && min_bid == low_bid.1
			);
			assert_matches!(AuctionPallet::current_phase(), AuctionPhase::WinnersSelected(winners, min_bid)
				if winners == vec![max_bid.0, joe_bid.0, low_bid.0] && min_bid == low_bid.1
			);
			// Just leaves us to confirm this auction, if we try to process this we will get an error
			// until is confirmed
			assert_matches!(AuctionPallet::process(), Err(AuctionError::NotConfirmed));
			// Confirm the auction
			CONFIRM.with(|l| { *l.borrow_mut() = true });
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
			assert_noop!(AuctionPallet::set_auction_size_range(Origin::root(), (0, 0)),
						Error::<Test>::InvalidRange);
			assert_noop!(AuctionPallet::set_auction_size_range(Origin::root(), (1, 2)),
						Error::<Test>::InvalidRange);
			// This should now work
			assert_ok!(AuctionPallet::set_auction_size_range(Origin::root(), (2, 100)));
			// Confirm we have an event
			assert_eq!(
				last_event(),
				mock::Event::pallet_cf_auction(crate::Event::AuctionRangeChanged((0, 0), (2, 100))),
			);
			//
			// We throw up an error if we try to set it to the current
			assert_noop!(AuctionPallet::set_auction_size_range(Origin::root(), (2, 100)),
						Error::<Test>::InvalidRange);
		});
	}

	#[test]
	fn kill_them_all() {
		new_test_ext().execute_with(|| {
			// Create a test set of bidders
			BIDDER_SET.with(|l| {
				*l.borrow_mut() = vec![(2, 2), (3, 100)]
			});

			let auction_range = (2, 100);
			CONFIRM.with(|l| { *l.borrow_mut() = true });
			assert_ok!(AuctionPallet::set_auction_range(auction_range));
			assert!(!AuctionPallet::auction_to_confirm());
			assert_matches!(AuctionPallet::process(), Ok(AuctionPhase::BidsTaken(_)));
			assert_matches!(AuctionPallet::process(), Ok(AuctionPhase::WinnersSelected(..)));
			assert_matches!(AuctionPallet::phase(), AuctionPhase::WinnersSelected(winners, min_bid)
				if !winners.is_empty() && min_bid > 0
			);
			assert!(AuctionPallet::auction_to_confirm());
			// Kill it
			AuctionPallet::abort();
			assert_eq!(AuctionPallet::phase(), AuctionPhase::default());
		});
	}
}
