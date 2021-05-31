mod test {
	use crate::*;
	use crate::{mock::*};
	use frame_support::{assert_ok};

	#[test]
	fn run_through_phases() {
		new_test_ext().execute_with(|| {
			// Create a test set of bidders
			let invalid_bid = (1, 0);
			let min_bid = (2, 2);
			let joe_bid = (3, 100);
			let max_bid = (4, 101);
			BIDDER_SET.with(|l| {
				*l.borrow_mut() = vec![invalid_bid, min_bid, joe_bid, max_bid]
			});

			let auction_range = (1, 100);
			// Check we are in the bidders phase
			assert_eq!(AuctionPallet::current_phase(), AuctionPhase::Bidders);
			// Now move to the next phase, this should be the auction phase
			assert_eq!(AuctionPallet::process(), Ok(AuctionPhase::Bidders));
			// Read storage to confirm has been changed to Auction
			assert_eq!(AuctionPallet::current_phase(), AuctionPhase::Auction);
			// Having moved into the auction phase we should have our list of bidders filtered
			// Check storage is what we assume
			assert_eq!(AuctionPallet::bidders(), vec![min_bid, joe_bid, max_bid]);
			// We should however have no outstanding winners or bond stored
			assert!(AuctionPallet::winners().is_empty());
			assert_eq!(AuctionPallet::minimum_bid(), 0);
			// And again to the next phase, however we should see an error as we haven't set our
			// auction size as an `AuctionError::Empty`
			assert_eq!(AuctionPallet::process(), Err(AuctionError::Empty));
			// In order to move forward we will need to set our auction set size
			// First test the call failing, range would have a 0 value or have equal values for min and max
			assert_eq!(AuctionPallet::set_auction_size((0, 0)), Err(AuctionError::InvalidRange));
			assert_eq!(AuctionPallet::set_auction_size((1, 1)), Err(AuctionError::InvalidRange));
			assert_ok!(AuctionPallet::set_auction_size(auction_range));
			// Check storage for auction range
			assert_eq!(AuctionPallet::auction_size_range(), auction_range);
			// With that sorted we would move on to completing the auction
			// Expecting the phase to change, a set of winners, the bidder list and a bond value set
			// to our min bid
			assert_eq!(AuctionPallet::process(), Ok(AuctionPhase::Auction));
			assert_eq!(AuctionPallet::current_phase(), AuctionPhase::Completed);
			assert_eq!(AuctionPallet::bidders(), vec![min_bid, joe_bid, max_bid]);
			assert_eq!(AuctionPallet::winners(), vec![max_bid.0, joe_bid.0, min_bid.0]);
			assert_eq!(AuctionPallet::minimum_bid(), min_bid.1);

			// and finally we complete the process, clearing the bidders
			assert_eq!(AuctionPallet::process(), Ok(AuctionPhase::Completed));
			assert_eq!(AuctionPallet::current_phase(), AuctionPhase::Bidders);
			assert!(AuctionPallet::bidders().is_empty());
		});
	}
}
