use crate::{mock::*, *};
use cf_test_utilities::last_event;
use cf_traits::mocks::keygen_exclusion::MockKeygenExclusion;
use frame_support::{assert_noop, assert_ok};

#[test]
fn should_provide_winning_set() {
	new_test_ext().execute_with(|| {
		generate_bids(NUMBER_OF_BIDDERS, BIDDER_GROUP_A);

		let auction_outcome =
			<AuctionPallet as Auctioneer<Test>>::resolve_auction().expect("the auction should run");

		assert_eq!((auction_outcome.winners.clone(), auction_outcome.bond), expected_winning_set());

		assert_eq!(
			last_event::<Test>(),
			mock::Event::AuctionPallet(crate::Event::AuctionCompleted(
				expected_winning_set().0,
				expected_winning_set().1
			)),
		);

		generate_bids(NUMBER_OF_BIDDERS, BIDDER_GROUP_B);
		let AuctionOutcome { winners, bond, .. } =
			AuctionPallet::resolve_auction().expect("the auction should run");

		assert_eq!(
			(winners, bond),
			expected_winning_set(),
			"running subsequent auction with new bidders should new winners"
		);
	});
}

#[test]
fn changing_range() {
	new_test_ext().execute_with(|| {
		// Assert our minimum is set to 2
		assert_eq!(<Test as Config>::MinValidators::get(), MIN_VALIDATOR_SIZE);
		// Check we are throwing up an error when we send anything less than the minimum of 1
		assert_noop!(
			AuctionPallet::set_active_validator_range(Origin::root(), (0, 0)),
			Error::<Test>::InvalidAuctionParameters
		);
		assert_noop!(
			AuctionPallet::set_active_validator_range(Origin::root(), (0, 1)),
			Error::<Test>::InvalidAuctionParameters
		);
		// This should now work
		assert_ok!(AuctionPallet::set_active_validator_range(Origin::root(), (2, 100)));
		// Confirm we have an event
		assert!(matches!(
			last_event::<Test>(),
			mock::Event::AuctionPallet(crate::Event::AuctionParametersChanged(..)),
		));
		assert_ok!(AuctionPallet::set_active_validator_range(Origin::root(), (2, 100)));
		assert_ok!(AuctionPallet::set_active_validator_range(Origin::root(), (3, 3)));
	});
}

// An auction has failed with a set of bad validators being reported to the pallet
// The subsequent auction will not include these validators
#[test]
fn should_exclude_bad_validators_in_next_auction() {
	new_test_ext().execute_with(|| {
		// Generate bids with half of these being reported as bad validators
		let number_of_bidders = 10;
		generate_bids(number_of_bidders, BIDDER_GROUP_A);

		// Split the good from the bad
		let (good_bidders, bad_bidders): (Vec<ValidatorId>, Vec<ValidatorId>) =
			MockBidderProvider::get_bidders()
				.iter()
				.map(|(id, _)| *id)
				.partition(|id| *id % 2 == 0);

		// Set bad bidders offline
		for bad_bidder in &bad_bidders {
			MockOnline::set_online(bad_bidder, false);
		}

		// Confirm we just have the good bidders in our new auction result
		assert_eq!(
			<AuctionPallet as Auctioneer<Test>>::resolve_auction()
				.expect("we should have an auction")
				.winners,
			good_bidders
				.iter()
				.take(MAX_VALIDATOR_SIZE as usize)
				.cloned()
				.collect::<Vec<_>>(),
		);
	});
}

#[test]
fn should_exclude_excluded_from_keygen_set() {
	new_test_ext().execute_with(|| {
		// Generate bids with half of these being reported as bad validators
		let number_of_bidders = 10;
		generate_bids(number_of_bidders, BIDDER_GROUP_A);

		// Split the good from the bad
		let (good_bidders, bad_bidders): (Vec<ValidatorId>, Vec<ValidatorId>) =
			MockBidderProvider::get_bidders()
				.iter()
				.map(|(id, _)| *id)
				.partition(|id| *id % 2 == 0);

		MockKeygenExclusion::<Test>::set(bad_bidders);

		// Confirm we just have the good bidders in our new auction result
		assert_eq!(
			<AuctionPallet as Auctioneer<Test>>::resolve_auction()
				.expect("we should have an auction")
				.winners,
			good_bidders
				.iter()
				.take(MAX_VALIDATOR_SIZE as usize)
				.cloned()
				.collect::<Vec<_>>(),
		);
	});
}
