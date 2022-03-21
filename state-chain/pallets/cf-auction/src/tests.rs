use crate::{mock::*, *};
use cf_traits::mocks::chainflip_account::MockChainflipAccount;
use frame_support::{assert_noop, assert_ok};

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
			last_event(),
			mock::Event::AuctionPallet(crate::Event::AuctionCompleted(expected_winning_set().0)),
		);

		<AuctionPallet as Auctioneer>::update_validator_status(&auction_result.winners);

		generate_bids(NUMBER_OF_BIDDERS, BIDDER_GROUP_B);
		let AuctionResult { winners, minimum_active_bid, .. } = run_complete_auction();

		assert_eq!(
			(winners, minimum_active_bid),
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
		let auction_result = run_complete_auction();
		let validate_states = |nodes: Vec<ValidatorId>, state: ChainflipAccountState| {
			for node in nodes {
				assert_eq!(MockChainflipAccount::get(&node).state, state);
			}
		};

		let validate_bidder_groups = |result: AuctionResult<ValidatorId, Amount>| {
			let number_of_bidders = MockBidderProvider::get_bidders().len() as u32;
			let (validators_size, backup_validators_size, passive_nodes_size) =
				expected_group_sizes(number_of_bidders);

			assert_eq!(validators_size, result.winners.len() as u32);

			assert_eq!(backup_validators_size, AuctionPallet::backup_group_size());

			assert_eq!(
				passive_nodes_size,
				AuctionPallet::remaining_bidders().len() as u32 -
					AuctionPallet::backup_group_size(),
				"expected passive node size is not expected"
			);

			validate_states(result.winners, ChainflipAccountState::Validator);

			let backup_validators = AuctionPallet::remaining_bidders()
				.iter()
				.take(AuctionPallet::backup_group_size() as usize)
				.map(|(validator_id, _)| *validator_id)
				.collect();

			validate_states(backup_validators, ChainflipAccountState::Backup);

			let passive_nodes = AuctionPallet::remaining_bidders()
				.iter()
				.skip(AuctionPallet::backup_group_size() as usize)
				.take(usize::MAX)
				.map(|(validator_id, _)| *validator_id)
				.collect();

			validate_states(passive_nodes, ChainflipAccountState::Passive);
		};

		// Validate groups at genesis
		validate_bidder_groups(auction_result);

		// Run a few auctions and validate groups
		let auction_bidders = [
			MAX_VALIDATOR_SIZE - 1,
			MAX_VALIDATOR_SIZE + 1,
			MAX_VALIDATOR_SIZE * 4 / 3,
			MAX_VALIDATOR_SIZE + MAX_VALIDATOR_SIZE / BACKUP_VALIDATOR_RATIO + 1,
		];
		for bidders in auction_bidders.iter() {
			generate_bids(*bidders, BIDDER_GROUP_A);
			validate_bidder_groups(run_complete_auction());
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
		run_complete_auction();

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

		assert_eq!(
			MockChainflipAccount::get(top_passive_node).state,
			ChainflipAccountState::Backup,
			"top passive node is now a backup validator"
		);

		assert_eq!(
			MockChainflipAccount::get(bottom_backup_validator).state,
			ChainflipAccountState::Passive,
			"bottom backup validator is now passive node"
		);

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
		run_complete_auction();

		let backup_validators = current_backup_validators();

		let (top_backup_validator_id, _) = backup_validators.first().unwrap();
		let new_bid = AuctionPallet::highest_passive_node_bid() - 1;

		HandleStakes::<Test>::stake_updated(top_backup_validator_id, new_bid);

		assert_eq!(
			MockChainflipAccount::get(top_backup_validator_id).state,
			ChainflipAccountState::Passive,
			"backup validator should be demoted to passive node"
		);

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
		run_complete_auction();
		// Place bid below lowest backup validator bid but above highest passive node
		// bid.  Should see lowest backup validator bid change but the state of the backup
		// validator would not change
		let backup_validators = current_backup_validators();

		let new_bid = AuctionPallet::lowest_backup_validator_bid() - 1;
		// Take the top and update bid
		let (top_backup_validator_id, _) = backup_validators.first().unwrap();
		HandleStakes::<Test>::stake_updated(top_backup_validator_id, new_bid);

		assert_eq!(
			MockChainflipAccount::get(top_backup_validator_id).state,
			ChainflipAccountState::Backup,
			"bid changed and state remains as backup validator"
		);

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
		run_complete_auction();
		// Place bid above highest passive node bid but below lowest backup validator
		// bid Should see highest passive node bid change but the state of the passive
		// node would not change
		let passive_nodes = current_passive_nodes();

		let new_bid = AuctionPallet::highest_passive_node_bid() + 1;
		// Take the top and update bid
		let (bottom_passive_node, _) = passive_nodes.last().unwrap();
		HandleStakes::<Test>::stake_updated(bottom_passive_node, new_bid);

		assert_eq!(
			MockChainflipAccount::get(bottom_passive_node).state,
			ChainflipAccountState::Passive,
			"should remain as passive node"
		);

		assert_eq!(
			AuctionPallet::highest_passive_node_bid(),
			new_bid,
			"the new highest bid for the passive node group"
		);
	});
}

#[test]
fn should_adjust_groups_in_emergency() {
	new_test_ext().execute_with(|| {
		let number_of_bidders = 150u32;
		let max_validators = 100u32;
		// Create some bidders
		generate_bids(number_of_bidders, BIDDER_GROUP_A);
		// Create a bigger group of validators, 100.
		AuctionPallet::set_active_range((MIN_VALIDATOR_SIZE, max_validators)).unwrap();
		// Run auction generate the groups
		run_complete_auction();

		// Request an emergency rotation
		MockEmergencyRotation::request_emergency_rotation();
		// Take down half the validators, holy moses!
		// This will mean we would have max_validators / 2 or 50 and after the first
		// auction we would have 1/3 BVs of max_validators or 33 giving us a total set of
		// bidders of 83.  However, in an emergency rotation we want to ensure we have
		// a maximum of 30% BVs in the active set of rather 30% of 33 or no more than
		// 9(rounded down int math) BVs.  This would mean when we come to the next active set we
		// would have 50 of the original active set plus no more than 9 BVs or 50 + 9 = 59.
		let mut bids = MockBidderProvider::get_bidders();
		// Sort and take the top half out `max_validators / 2`
		bids.sort_unstable_by_key(|k| k.1);
		bids.reverse();
		// Set our new set of bidders
		let bidders_in_emergency_network: Vec<_> =
			bids.iter().skip((max_validators / 2) as usize).cloned().collect();

		// Check the states of each
		let number_of_backup_validators = bidders_in_emergency_network
			.iter()
			.filter(|(validator_id, _)| {
				MockChainflipAccount::get(validator_id).state == ChainflipAccountState::Backup
			})
			.count() as u32;

		let number_of_validators = bidders_in_emergency_network
			.iter()
			.filter(|(validator_id, _)| {
				MockChainflipAccount::get(validator_id).state == ChainflipAccountState::Validator
			})
			.count() as u32;

		// Confirming the maths is right
		// We should have half our validators
		assert_eq!(number_of_validators, max_validators / 2);
		// and the remaining BVs or 100/3
		assert_eq!(number_of_backup_validators, max_validators / 3);

		set_bidders(bidders_in_emergency_network);

		// Let's now run the emergency auction
		// We have a set of 100 bidders, 50 validators, 33 backup validators and 17 passive
		// nodes If this wasn't an emergency rotation we would see the same distribution after
		// an auction but as we have requested an emergency rotation we should see 50 plus 33 *
		// 30% as validators or rather the winners.
		let auction_result = run_complete_auction();

		assert_eq!(
			auction_result.winners.len() as u32,
			(PercentageOfBackupValidatorsInEmergency::get() * number_of_backup_validators) / 100 +
				number_of_validators
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
			<AuctionPallet as Auctioneer>::resolve_auction()
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
