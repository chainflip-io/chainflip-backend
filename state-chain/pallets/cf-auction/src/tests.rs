use crate::{mock::*, *};
use cf_test_utilities::last_event;
use cf_traits::mocks::keygen_exclusion::MockKeygenExclusion;
use frame_support::assert_ok;

#[test]
fn should_provide_winning_set() {
	new_test_ext().execute_with(|| {
		generate_bids(NUMBER_OF_BIDDERS, BIDDER_GROUP_A);

		let auction_result =
			<AuctionPallet as Auctioneer>::resolve_auction().expect("the auction should run");

		assert_eq!(
			(auction_result.winners.clone(), auction_result.minimum_active_bid),
			expected_winning_set()
		);

		assert_eq!(
			last_event::<Test>(),
			mock::Event::AuctionPallet(crate::Event::AuctionCompleted(expected_winning_set().0)),
		);

		generate_bids(NUMBER_OF_BIDDERS, BIDDER_GROUP_B);
		let AuctionResult { winners, minimum_active_bid, .. } =
			AuctionPallet::resolve_auction().expect("the auction should run");

		assert_eq!(
			(winners, minimum_active_bid),
			expected_winning_set(),
			"running subsequent auction with new bidders should new winners"
		);
	});
}

fn expected_group_sizes(number_of_bidders: u32) -> (u32, u32, u32) {
	let expected_number_of_authorities = min(MAX_AUTHORITY_SIZE, number_of_bidders);
	let expected_number_of_backup_nodes = min(
		expected_number_of_authorities / BACKUP_NODE_RATIO,
		number_of_bidders.saturating_sub(expected_number_of_authorities),
	);
	let expected_number_of_passive_nodes = number_of_bidders
		.saturating_sub(expected_number_of_backup_nodes)
		.saturating_sub(expected_number_of_authorities);
	(
		expected_number_of_authorities,
		expected_number_of_backup_nodes,
		expected_number_of_passive_nodes,
	)
}

#[test]
fn should_create_correct_size_of_groups() {
	new_test_ext().execute_with(|| {
		generate_bids(NUMBER_OF_BIDDERS, BIDDER_GROUP_A);
		let auction_result = AuctionPallet::resolve_auction().expect("the auction should run");

		let validate_bidder_groups = |result: AuctionResult<ValidatorId, Amount>| {
			let number_of_bidders = MockBidderProvider::get_bidders().len() as u32;
			let (authority_set_size, backup_nodes_size, passive_nodes_size) =
				expected_group_sizes(number_of_bidders);

			assert_eq!(authority_set_size, result.winners.len() as u32);

			assert_eq!(backup_nodes_size, AuctionPallet::backup_group_size());

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
			MAX_AUTHORITY_SIZE - 1,
			MAX_AUTHORITY_SIZE + 1,
			MAX_AUTHORITY_SIZE * 4 / 3,
			MAX_AUTHORITY_SIZE + MAX_AUTHORITY_SIZE / BACKUP_NODE_RATIO + 1,
		];
		for number_of_bidders in numbers_of_auction_bidders.iter() {
			generate_bids(*number_of_bidders, BIDDER_GROUP_A);

			validate_bidder_groups(
				AuctionPallet::resolve_auction().expect("the auction should run"),
			);
		}
	});
}

fn current_backup_nodes() -> Vec<RemainingBid<ValidatorId, Amount>> {
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

		let backup_nodes = current_backup_nodes();
		let passive_nodes = current_passive_nodes();

		let (bottom_backup_node, lowest_backup_node_bid) = backup_nodes.last().unwrap();
		let (top_passive_node, highest_passive_node_bid) = passive_nodes.first().unwrap();

		assert_eq!(*lowest_backup_node_bid, AuctionPallet::lowest_backup_node_bid());

		assert_eq!(*highest_passive_node_bid, AuctionPallet::highest_passive_node_bid());
		let new_bid = lowest_backup_node_bid + 1;

		// Promote a passive node to the backup set
		HandleStakes::<Test>::stake_updated(top_passive_node, new_bid);

		// Reset with the new bid
		let top_of_the_passive_nodes = (*top_passive_node, new_bid);

		let backup_nodes = current_backup_nodes();
		let passive_nodes = current_passive_nodes();

		let new_bottom_of_the_backup_nodes = backup_nodes.last().unwrap();
		let new_top_of_the_passive_nodes = passive_nodes.first().unwrap();

		assert_eq!(&top_of_the_passive_nodes, new_bottom_of_the_backup_nodes);

		assert_eq!(*new_top_of_the_passive_nodes, (*bottom_backup_node, *lowest_backup_node_bid));

		assert_eq!(AuctionPallet::lowest_backup_node_bid(), new_bid);
	});
}

#[test]
fn should_demote_backup_node_on_poor_stake() {
	new_test_ext().execute_with(|| {
		generate_bids(NUMBER_OF_BIDDERS, BIDDER_GROUP_A);
		// do the genesis
		AuctionPallet::resolve_auction().expect("the auction should run");
		AuctionPallet::update_backup_and_passive_states();

		let backup_nodes = current_backup_nodes();

		let (top_backup_node_id, _) = backup_nodes.first().unwrap();
		let new_bid = AuctionPallet::highest_passive_node_bid() - 1;

		HandleStakes::<Test>::stake_updated(top_backup_node_id, new_bid);

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
fn should_establish_a_new_lowest_backup_node_bid() {
	new_test_ext().execute_with(|| {
		generate_bids(NUMBER_OF_BIDDERS, BIDDER_GROUP_A);
		AuctionPallet::resolve_auction().expect("the auction should run");
		AuctionPallet::update_backup_and_passive_states();
		// Place bid below lowest backup node bid but above highest passive node
		// bid.  Should see lowest backup node bid change but the state of the backup
		// node would not change
		let backup_nodes = current_backup_nodes();

		let new_bid = AuctionPallet::lowest_backup_node_bid() - 1;
		// Take the top and update bid to one less than the lowest bid. e.g.
		let (top_backup_node_id, _) = backup_nodes.first().unwrap();
		HandleStakes::<Test>::stake_updated(top_backup_node_id, new_bid);

		assert_eq!(
			AuctionPallet::lowest_backup_node_bid(),
			new_bid,
			"the new lower bid is now the lowest bid for the backup node group"
		);
	});
}

#[test]
fn should_establish_a_highest_passive_node_bid() {
	new_test_ext().execute_with(|| {
		generate_bids(NUMBER_OF_BIDDERS, BIDDER_GROUP_A);
		AuctionPallet::resolve_auction().expect("the auction should run");
		AuctionPallet::update_backup_and_passive_states();
		// Place bid above highest passive node bid but below lowest backup node
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
		// This should now work
		assert_ok!(AuctionPallet::set_current_authority_set_size_range(Origin::root(), (2, 100)));
		// Confirm we have an event
		assert_eq!(
			last_event::<Test>(),
			mock::Event::AuctionPallet(crate::Event::AuthoritySetSizeRangeChanged(
				(MIN_AUTHORITY_SIZE, MAX_AUTHORITY_SIZE),
				(2, 100)
			)),
		);
		assert_ok!(AuctionPallet::set_current_authority_set_size_range(Origin::root(), (2, 100)));
		assert_ok!(AuctionPallet::set_current_authority_set_size_range(Origin::root(), (3, 3)));
	});
}

// An auction has failed with a set of bad authorities being reported to the pallet
// The subsequent auction will not include these authorities
#[test]
fn should_exclude_bad_authorities_in_next_auction() {
	new_test_ext().execute_with(|| {
		// Generate bids with half of these being reported as bad authorities
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
			<AuctionPallet as Auctioneer>::resolve_auction()
				.expect("we should have an auction")
				.winners,
			good_bidders
				.iter()
				.take(MAX_AUTHORITY_SIZE as usize)
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
			<AuctionPallet as Auctioneer>::resolve_auction()
				.expect("we should have an auction")
				.winners,
			good_bidders
				.iter()
				.take(MAX_AUTHORITY_SIZE as usize)
				.cloned()
				.collect::<Vec<_>>(),
		);
	});
}
