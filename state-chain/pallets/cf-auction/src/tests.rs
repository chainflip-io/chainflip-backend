mod test {
	use crate::mock::*;
	use crate::*;
	use cf_traits::mocks::vault_rotation::{clear_confirmation, Mock as MockVaultRotator};
	use frame_support::{assert_noop, assert_ok};

	#[test]
	fn we_have_a_set_of_winners_at_genesis() {
		new_test_ext().execute_with(|| {
			let (winners, minimum_active_bid) = expected_validating_set();
			assert_eq!(
				AuctionPallet::auction_result(),
				Some(AuctionResult {
					winners,
					minimum_active_bid,
				})
			);
		});
	}

	#[test]
	fn run_through_phases() {
		new_test_ext().execute_with(|| {
			// We would have the genesis state with group 1 of the bidders
			let (old_winners, old_minimum_active_bid) = expected_validating_set();
			assert_eq!(
				AuctionPallet::auction_result(),
				Some(AuctionResult {
					winners: old_winners.clone(),
					minimum_active_bid: old_minimum_active_bid,
				})
			);
			generate_bids(NUMBER_OF_BIDDERS, BIDDER_GROUP_B);
			// Check we are in the bidders phase
			assert_matches!(AuctionPallet::phase(), AuctionPhase::WaitingForBids);
			// Now move to the next phase, this should be the BidsTaken phase
			assert_matches!(AuctionPallet::process(), Ok(AuctionPhase::BidsTaken(bids))
				if bids == MockBidderProvider::get_bidders());
			// Read storage to confirm has been changed to BidsTaken
			assert_matches!(AuctionPallet::current_phase(), AuctionPhase::BidsTaken(bids)
				if bids == MockBidderProvider::get_bidders());
			// Having moved into the BidsTaken phase we should have our list of bidders filtered
			// Expecting the phase to change, a set of winners, the bidder list and a bond value set
			// to our min bid
			assert_matches!(AuctionPallet::process(), Ok(AuctionPhase::ValidatorsSelected(validators, minimum_active_bid))
				if (validators.clone(), minimum_active_bid) == expected_validating_set()
			);
			assert_matches!(AuctionPallet::current_phase(), AuctionPhase::ValidatorsSelected(validators, minimum_active_bid)
				if (validators.clone(), minimum_active_bid) == expected_validating_set()
			);
			assert_eq!(
				last_event(),
				mock::Event::pallet_cf_auction(crate::Event::AuctionCompleted(
					1,
					expected_validating_set().0
				)),
			);
			// Just leaves us to confirm this auction, if we try to process this we will get an error
			// until is confirmed
			assert_matches!(AuctionPallet::process(), Err(AuctionError::NotConfirmed));
			// Confirm the auction
			clear_confirmation();
			// and finally we complete the process, a list of confirmed validators
			let (new_winners, new_minimum_active_bid) = expected_validating_set();
			assert_matches!(AuctionPallet::process(), Ok(AuctionPhase::ConfirmedValidators(validators, minimum_active_bid))
				if (validators.clone(), minimum_active_bid) == (new_winners.clone(), new_minimum_active_bid)
			);

			assert_eq!(
				AuctionPallet::auction_result(),
				Some(AuctionResult {
					winners: new_winners.clone(),
					minimum_active_bid: new_minimum_active_bid
				})
			);

			assert_ne!(old_winners, new_winners);
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
			run_auction();
			let validate_states = |nodes: Vec<ValidatorId>, state: ChainflipAccountState| {
				for node in nodes {
					assert_eq!(MockChainflipAccount::get(&node).state, state);
				}
			};

			let validate_bidder_groups = || {
				let number_of_bidders = MockBidderProvider::get_bidders().len() as u32;
				let (validators_size, backup_validators_size, passive_nodes_size) =
					expected_group_sizes(number_of_bidders);

				if let Some(result) = AuctionPallet::auction_result() {
					assert_eq!(validators_size, result.winners.len() as u32);
					assert_eq!(backup_validators_size, AuctionPallet::backup_group_size());
					assert_eq!(
						passive_nodes_size,
						AuctionPallet::remaining_bidders().len() as u32
							- AuctionPallet::backup_group_size()
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
				}
			};

			// Validate groups at genesis
			validate_bidder_groups();

			// Run a few auctions and validate groups
			let auction_bidders = [
				MAX_VALIDATOR_SIZE - 1,
				MAX_VALIDATOR_SIZE + 1,
				MAX_VALIDATOR_SIZE * 4 / 3,
				MAX_VALIDATOR_SIZE + MAX_VALIDATOR_SIZE / BACKUP_VALIDATOR_RATIO + 1,
			];
			for bidders in auction_bidders.iter() {
				generate_bids(*bidders, BIDDER_GROUP_A);
				run_auction();
				validate_bidder_groups();
			}
		});
	}

	fn current_backup_validators() -> Vec<RemainingBid<ValidatorId, Amount>> {
		AuctionPallet::remaining_bidders()
			.iter()
			.take(AuctionPallet::backup_group_size() as usize)
			.map(|bid| *bid)
			.collect()
	}

	fn current_passive_nodes() -> Vec<RemainingBid<ValidatorId, Amount>> {
		AuctionPallet::remaining_bidders()
			.iter()
			.skip(AuctionPallet::backup_group_size() as usize)
			.take(usize::MAX)
			.map(|bid| *bid)
			.collect()
	}

	#[test]
	fn should_promote_passive_node_if_stake_qualifies_for_backup() {
		new_test_ext().execute_with(|| {
			generate_bids(NUMBER_OF_BIDDERS, BIDDER_GROUP_A);
			run_auction();

			match AuctionPallet::current_phase() {
				AuctionPhase::WaitingForBids => {
					let backup_validators = current_backup_validators();
					let passive_nodes = current_passive_nodes();

					let (bottom_backup_validator, lowest_backup_validator_bid) =
						backup_validators.last().unwrap();
					let (top_passive_node, highest_passive_node_bid) =
						passive_nodes.first().unwrap();
					assert_eq!(
						*lowest_backup_validator_bid,
						AuctionPallet::lowest_backup_validator_bid()
					);
					assert_eq!(
						*highest_passive_node_bid,
						AuctionPallet::highest_passive_node_bid()
					);
					let new_bid = lowest_backup_validator_bid + 1;

					// Promote a passive node to the backup set
					HandleStakes::<Test>::stake_updated(top_passive_node, new_bid);

					assert_eq!(
						MockChainflipAccount::get(top_passive_node).state,
						ChainflipAccountState::Backup
					);

					assert_eq!(
						MockChainflipAccount::get(&bottom_backup_validator).state,
						ChainflipAccountState::Passive
					);

					// Reset with the new bid
					let top_of_the_passive_nodes = (*top_passive_node, new_bid);

					let backup_validators = current_backup_validators();
					let passive_nodes = current_passive_nodes();

					let new_bottom_of_the_backup_validators = backup_validators.last().unwrap();
					let new_top_of_the_passive_nodes = passive_nodes.first().unwrap();

					assert_eq!(
						&top_of_the_passive_nodes,
						new_bottom_of_the_backup_validators
					);
					assert_eq!(
						*new_top_of_the_passive_nodes,
						(*bottom_backup_validator, *lowest_backup_validator_bid)
					);

					assert_eq!(AuctionPallet::lowest_backup_validator_bid(), new_bid);
				}
				_ => unreachable!("wrong phase"),
			}
		});
	}

	#[test]
	fn should_demote_backup_validator_on_poor_stake() {
		new_test_ext().execute_with(|| {
			generate_bids(NUMBER_OF_BIDDERS, BIDDER_GROUP_A);
			run_auction();

			match AuctionPallet::current_phase() {
				AuctionPhase::WaitingForBids => {
					let backup_validators = current_backup_validators();

					let (top_backup_validator_id, _) = backup_validators.first().unwrap();
					let new_bid = AuctionPallet::highest_passive_node_bid() - 1;

					HandleStakes::<Test>::stake_updated(top_backup_validator_id, new_bid);

					assert_eq!(
						MockChainflipAccount::get(top_backup_validator_id).state,
						ChainflipAccountState::Passive
					);

					// The top passive node would move upto backup set and the highest passive bid
					// would be recalculated
					assert_eq!(AuctionPallet::highest_passive_node_bid(), new_bid);
				}
				_ => unreachable!("wrong phase"),
			}
		});
	}

	#[test]
	fn should_establish_a_new_lowest_backup_validator_bid() {
		new_test_ext().execute_with(|| {
			generate_bids(NUMBER_OF_BIDDERS, BIDDER_GROUP_A);
			run_auction();
			match AuctionPallet::current_phase() {
				AuctionPhase::WaitingForBids => {
					// Place bid below lowest backup validator bid but above highest passive node bid
					// Should see lowest backup validator bid change but the state of the backup
					// validator would not change
					let backup_validators = current_backup_validators();

					let new_bid = AuctionPallet::lowest_backup_validator_bid() - 1;
					// Take the top and update bid
					let (top_backup_validator_id, _) = backup_validators.first().unwrap();
					HandleStakes::<Test>::stake_updated(top_backup_validator_id, new_bid);

					assert_eq!(
						MockChainflipAccount::get(top_backup_validator_id).state,
						ChainflipAccountState::Backup
					);

					assert_eq!(AuctionPallet::lowest_backup_validator_bid(), new_bid);
				}
				_ => unreachable!("wrong phase"),
			}
		});
	}

	#[test]
	fn should_establish_a_highest_passive_node_bid() {
		new_test_ext().execute_with(|| {
			generate_bids(NUMBER_OF_BIDDERS, BIDDER_GROUP_A);
			run_auction();
			match AuctionPallet::current_phase() {
				AuctionPhase::WaitingForBids => {
					// Place bid above highest passive node bid but below lowest backup validator bid
					// Should see highest passive node bid change but the state of the passive node
					// would not change
					let passive_nodes = current_passive_nodes();

					let new_bid = AuctionPallet::highest_passive_node_bid() + 1;
					// Take the top and update bid
					let (bottom_passive_node, _) = passive_nodes.last().unwrap();
					HandleStakes::<Test>::stake_updated(bottom_passive_node, new_bid);

					assert_eq!(
						MockChainflipAccount::get(bottom_passive_node).state,
						ChainflipAccountState::Passive
					);

					assert_eq!(AuctionPallet::highest_passive_node_bid(), new_bid);
				}
				_ => unreachable!("wrong phase"),
			}
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
			run_auction();
			// Request an emergency rotation
			MockEmergencyRotation::request_emergency_rotation();
			// Take down half the validators, holy moses!
			// This will mean we would have max_validators / 2 or 50 and after the first
			// auction we would have 1/3 BVs of max_validators or 33 giving us a total set of
			// bidders of 83.  However, in an emergency rotation we want to ensure we have
			// a maximum of 30% BVs in the active set of rather 30% of 83 or no more than
			// 24(rounded down int math) BVs.  This would mean when we come to the next active set we would have
			// 50 of the original active set plus no more than 25 BVs or 50 + 25 = 75.
			let mut bids = MockBidderProvider::get_bidders();
			// Sort and take the top half out `max_validators / 2`
			bids.sort_unstable_by_key(|k| k.1);
			bids.reverse();
			// Set our new set of bidders
			let bidders_in_emergency_network: Vec<_> = bids
				.iter()
				.skip((max_validators / 2) as usize)
				.cloned()
				.collect();

			// Check the states of each
			let number_of_backup_validators = bidders_in_emergency_network
				.iter()
				.filter(|(validator_id, _)| {
					MockChainflipAccount::get(&validator_id).state == ChainflipAccountState::Backup
				})
				.count() as u32;

			let number_of_validators = bidders_in_emergency_network
				.iter()
				.filter(|(validator_id, _)| {
					MockChainflipAccount::get(&validator_id).state
						== ChainflipAccountState::Validator
				})
				.count() as u32;

			// Confirming the maths is right
			// We should have half our validators
			assert_eq!(number_of_validators, max_validators / 2);
			// and the remaining BVs or 100/3
			assert_eq!(number_of_backup_validators, max_validators / 3);

			let number_of_emergency_bidders = bidders_in_emergency_network.len();
			set_bidders(bidders_in_emergency_network);

			// Let's now run the emergency auction
			// We have a set of 100 bidders, 50 validators, 33 backup validators and 17 passive nodes
			// If this wasn't an emergency rotation we would see the same distribution after an auction
			// but as we have requested an emergency rotation we should see 50 plus (50 + 33) * 30% as
			// validators or rather the winners.
			run_auction();

			let auction_result = AuctionPallet::auction_result().expect("an auction result please");
			assert_eq!(
				auction_result.winners.len() as u32,
				(PercentageOfBackupValidatorsInEmergency::get()
					* (number_of_validators + number_of_backup_validators))
					/ 100 + number_of_validators
			);

			// This would leave a 1/3 or less of backup validators of our emergency bidding set.
			// In this case this would be 100 bidders minus the winners in the auction
			assert_eq!(
				AuctionPallet::backup_group_size() as usize,
				number_of_emergency_bidders - auction_result.winners.len()
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
			run_auction();
			// Request an emergency rotation
			MockEmergencyRotation::request_emergency_rotation();
			// Take down half the validators, holy moses!
			// This will mean we would have max_validators / 2 or 50 and after the first
			// auction we would have 1/3 BVs of max_validators or 33 giving us a total set of
			// bidders of 83.  However, in an emergency rotation we want to ensure we have
			// a maximum of 30% BVs in the active set of rather 30% of 33 or no more than
			// 9(rounded down int math) BVs.  This would mean when we come to the next active set we would have
			// 50 of the original active set plus no more than 9 BVs or 50 + 9 = 59.
			let mut bids = MockBidderProvider::get_bidders();
			// Sort and take the top half out `max_validators / 2`
			bids.sort_unstable_by_key(|k| k.1);
			bids.reverse();
			// Set our new set of bidders
			let bidders_in_emergency_network: Vec<_> = bids
				.iter()
				.skip((max_validators / 2) as usize)
				.cloned()
				.collect();

			// Check the states of each
			let number_of_backup_validators = bidders_in_emergency_network
				.iter()
				.filter(|(validator_id, _)| {
					MockChainflipAccount::get(&validator_id).state == ChainflipAccountState::Backup
				})
				.count() as u32;

			let number_of_validators = bidders_in_emergency_network
				.iter()
				.filter(|(validator_id, _)| {
					MockChainflipAccount::get(&validator_id).state
						== ChainflipAccountState::Validator
				})
				.count() as u32;

			// Confirming the maths is right
			// We should have half our validators
			assert_eq!(number_of_validators, max_validators / 2);
			// and the remaining BVs or 100/3
			assert_eq!(number_of_backup_validators, max_validators / 3);

			set_bidders(bidders_in_emergency_network);

			// Let's now run the emergency auction
			// We have a set of 100 bidders, 50 validators, 33 backup validators and 17 passive nodes
			// If this wasn't an emergency rotation we would see the same distribution after an auction
			// but as we have requested an emergency rotation we should see 50 plus 33 * 30% as
			// validators or rather the winners.
			run_auction();

			let auction_result = AuctionPallet::auction_result().expect("an auction result please");
			assert_eq!(
				auction_result.winners.len() as u32,
				(PercentageOfBackupValidatorsInEmergency::get() * number_of_backup_validators)
					/ 100 + number_of_validators
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
			assert_ok!(AuctionPallet::set_active_validator_range(
				Origin::root(),
				(2, 100)
			));
			// Confirm we have an event
			assert_eq!(
				last_event(),
				mock::Event::pallet_cf_auction(crate::Event::ActiveValidatorRangeChanged(
					(MIN_VALIDATOR_SIZE, MAX_VALIDATOR_SIZE),
					(2, 100)
				)),
			);
			//
			// We throw up an error if we try to set it to the current
			assert_noop!(
				AuctionPallet::set_active_validator_range(Origin::root(), (2, 100)),
				Error::<Test>::InvalidRange
			);
		});
	}

	#[test]
	fn kill_them_all() {
		new_test_ext().execute_with(|| {
			// Create a test set of bidders
			generate_bids(2, BIDDER_GROUP_A);
			assert_matches!(AuctionPallet::process(), Ok(AuctionPhase::BidsTaken(..)));
			assert_matches!(
				AuctionPallet::process(),
				Ok(AuctionPhase::ValidatorsSelected(..))
			);

			assert_matches!(AuctionPallet::phase(), AuctionPhase::ValidatorsSelected(validators, minimum_active_bid)
				if !validators.is_empty() && minimum_active_bid > 0
			);
			// Kill it
			AuctionPallet::abort();
			assert_eq!(AuctionPallet::phase(), AuctionPhase::default());
		});
	}

	#[test]
	fn should_abort_on_error_in_starting_vault_rotation() {
		new_test_ext().execute_with(|| {
			assert_matches!(AuctionPallet::process(), Ok(AuctionPhase::BidsTaken(bids))
				if bids == MockBidderProvider::get_bidders());
			// Signal we want to error on vault rotation
			MockVaultRotator::error_on_start_vault_rotation();
			assert_matches!(AuctionPallet::process(), Err(..));
		});
	}
}
