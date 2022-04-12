use crate::{mock::*, *};
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
			last_event(),
			mock::Event::AuctionPallet(crate::Event::AuctionCompleted(expected_winning_set().0)),
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

fn expected_group_sizes(number_of_bidders: u32) -> (u32, u32, u32) {
	let expected_number_of_validators = min(MAX_VALIDATOR_SIZE, number_of_bidders);
	let expected_number_of_backup_validators = min(
		expected_number_of_validators / BACKUP_VALIDATOR_RATIO,
		number_of_bidders.saturating_sub(expected_number_of_validators),
	);
	let expected_number_of_passive_nodes = number_of_bidders
		.saturating_sub(expected_number_of_backup_validators)
		.saturating_sub(expected_number_of_validators);
	(
		expected_number_of_validators,
		expected_number_of_backup_validators,
		expected_number_of_passive_nodes,
	)
}

#[test]
fn should_create_correct_size_of_groups() {
	new_test_ext().execute_with(|| {
		generate_bids(NUMBER_OF_BIDDERS, BIDDER_GROUP_A);
		let auction_result = AuctionPallet::resolve_auction().expect("the auction should run");

		let validate_bidder_groups = |outcome: AuctionOutcome<Test>| {
			let number_of_bidders = MockBidderProvider::get_bidders().len() as u32;
			let (validators_size, backup_validators_size, passive_nodes_size) =
				expected_group_sizes(number_of_bidders);

			assert_eq!(validators_size, outcome.winners.len() as u32);

			assert_eq!(backup_validators_size, AuctionPallet::backup_group_size());

			assert_eq!(
				passive_nodes_size,
				AuctionPallet::remaining_bidders().len() as u32 -
					AuctionPallet::backup_group_size(),
				"expected passive node size is not expected"
			);
		};

		// Validate groups at genesis
		validate_bidder_groups(auction_result);

		// Run a few auctions and validate groups
		let numbers_of_auction_bidders = [
			MAX_VALIDATOR_SIZE - 1,
			MAX_VALIDATOR_SIZE + 1,
			MAX_VALIDATOR_SIZE * 4 / 3,
			MAX_VALIDATOR_SIZE + MAX_VALIDATOR_SIZE / BACKUP_VALIDATOR_RATIO + 1,
		];
		for number_of_bidders in numbers_of_auction_bidders.iter() {
			generate_bids(*number_of_bidders, BIDDER_GROUP_A);

			validate_bidder_groups(
				AuctionPallet::resolve_auction().expect("the auction should run"),
			);
		}
	});
}

fn current_backup_validators() -> Vec<RemainingBid<ValidatorId, Amount>> {
	AuctionPallet::remaining_bidders()
		.iter()
		.take(AuctionPallet::backup_group_size() as usize)
		.copied()
		.collect()
}

fn current_passive_nodes() -> Vec<RemainingBid<ValidatorId, Amount>> {
	AuctionPallet::remaining_bidders()
		.iter()
		.skip(AuctionPallet::backup_group_size() as usize)
		.take(usize::MAX)
		.copied()
		.collect()
}

#[test]
fn should_promote_passive_node_if_stake_qualifies_for_backup() {
	new_test_ext().execute_with(|| {
		generate_bids(NUMBER_OF_BIDDERS, BIDDER_GROUP_A);
		AuctionPallet::resolve_auction().expect("the auction should run");
		AuctionPallet::update_backup_and_passive_states();

		let backup_validators = current_backup_validators();
		let passive_nodes = current_passive_nodes();

		let (bottom_backup_validator, lowest_backup_validator_bid) =
			backup_validators.last().unwrap();
		let (top_passive_node, highest_passive_node_bid) = passive_nodes.first().unwrap();

		assert_eq!(*lowest_backup_validator_bid, AuctionPallet::lowest_backup_validator_bid());

		assert_eq!(*highest_passive_node_bid, AuctionPallet::highest_passive_node_bid());
		let new_bid = lowest_backup_validator_bid + 1;

		// Promote a passive node to the backup set
		HandleStakes::<Test>::stake_updated(top_passive_node, new_bid);

		// Reset with the new bid
		let top_of_the_passive_nodes = (*top_passive_node, new_bid);

		let backup_validators = current_backup_validators();
		let passive_nodes = current_passive_nodes();

		let new_bottom_of_the_backup_validators = backup_validators.last().unwrap();
		let new_top_of_the_passive_nodes = passive_nodes.first().unwrap();

		assert_eq!(&top_of_the_passive_nodes, new_bottom_of_the_backup_validators);

		assert_eq!(
			*new_top_of_the_passive_nodes,
			(*bottom_backup_validator, *lowest_backup_validator_bid)
		);

		assert_eq!(AuctionPallet::lowest_backup_validator_bid(), new_bid);
	});
}

#[test]
fn should_demote_backup_validator_on_poor_stake() {
	new_test_ext().execute_with(|| {
		generate_bids(NUMBER_OF_BIDDERS, BIDDER_GROUP_A);
		// do the genesis
		AuctionPallet::resolve_auction().expect("the auction should run");
		AuctionPallet::update_backup_and_passive_states();

		let backup_validators = current_backup_validators();

		let (top_backup_validator_id, _) = backup_validators.first().unwrap();
		let new_bid = AuctionPallet::highest_passive_node_bid() - 1;

		HandleStakes::<Test>::stake_updated(top_backup_validator_id, new_bid);

		// The top passive node would move upto backup set and the highest passive bid
		// would be recalculated
		assert_eq!(
			AuctionPallet::highest_passive_node_bid(),
			new_bid,
			"highest passive node bid should be the new bid"
		);
	});
}

#[test]
fn should_establish_a_new_lowest_backup_validator_bid() {
	new_test_ext().execute_with(|| {
		generate_bids(NUMBER_OF_BIDDERS, BIDDER_GROUP_A);
		AuctionPallet::resolve_auction().expect("the auction should run");
		AuctionPallet::update_backup_and_passive_states();
		// Place bid below lowest backup validator bid but above highest passive node
		// bid.  Should see lowest backup validator bid change but the state of the backup
		// validator would not change
		let backup_validators = current_backup_validators();

		let new_bid = AuctionPallet::lowest_backup_validator_bid() - 1;
		// Take the top and update bid to one less than the lowest bid. e.g.
		let (top_backup_validator_id, _) = backup_validators.first().unwrap();
		HandleStakes::<Test>::stake_updated(top_backup_validator_id, new_bid);

		assert_eq!(
			AuctionPallet::lowest_backup_validator_bid(),
			new_bid,
			"the new lower bid is now the lowest bid for the backup validator group"
		);
	});
}

#[test]
fn should_establish_a_highest_passive_node_bid() {
	new_test_ext().execute_with(|| {
		generate_bids(NUMBER_OF_BIDDERS, BIDDER_GROUP_A);
		AuctionPallet::resolve_auction().expect("the auction should run");
		AuctionPallet::update_backup_and_passive_states();
		// Place bid above highest passive node bid but below lowest backup validator
		// bid Should see highest passive node bid change but the state of the passive
		// node would not change
		let passive_nodes = current_passive_nodes();

		let new_bid = AuctionPallet::highest_passive_node_bid() + 1;
		// Take the top and update bid
		let (bottom_passive_node, _) = passive_nodes.last().unwrap();
		HandleStakes::<Test>::stake_updated(bottom_passive_node, new_bid);

		assert_eq!(
			AuctionPallet::highest_passive_node_bid(),
			new_bid,
			"the new highest bid for the passive node group"
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
			Error::<Test>::InvalidRange
		);
		assert_noop!(
			AuctionPallet::set_active_validator_range(Origin::root(), (0, 1)),
			Error::<Test>::InvalidRange
		);
		// This should now work
		assert_ok!(AuctionPallet::set_active_validator_range(Origin::root(), (2, 100)));
		// Confirm we have an event
		assert_eq!(
			last_event(),
			mock::Event::AuctionPallet(crate::Event::ActiveValidatorRangeChanged(
				(MIN_VALIDATOR_SIZE, MAX_VALIDATOR_SIZE),
				(2, 100)
			)),
		);
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
