mod test {
	use crate::mock::*;
	use crate::*;
	use cf_traits::mocks::vault_rotation::clear_confirmation;
	use cf_traits::Bid;
	use frame_support::{assert_noop, assert_ok};

	fn last_event() -> mock::Event {
		frame_system::Pallet::<Test>::events()
			.pop()
			.expect("Event expected")
			.event
	}

	// The last is invalid as it has a bid of 0
	fn expected_bidding() -> Vec<Bid<ValidatorId, Amount>> {
		let mut bidders = TestBidderProvider::get_bidders();
		bidders.pop();
		bidders
	}

	// The set we would expect
	fn expected_validating_set() -> (Vec<ValidatorId>, Amount) {
		let mut bidders = TestBidderProvider::get_bidders();
		bidders.truncate(MAX_VALIDATOR_SIZE as usize);
		(
			bidders
				.iter()
				.map(|(validator_id, _)| *validator_id)
				.collect(),
			bidders.last().unwrap().1,
		)
	}

	#[test]
	fn genesis() {
		new_test_ext().execute_with(|| {
			// We should have our genesis validators, which would have been provided by
			// `BidderProvider`
			assert_matches!(AuctionPallet::phase(), AuctionPhase::WaitingForBids(validators, minimum_active_bid)
				if (validators.clone(), minimum_active_bid) == expected_validating_set()
			);
		});
	}
	#[test]
	fn run_through_phases() {
		new_test_ext().execute_with(|| {
			// Check we are in the bidders phase
			assert_matches!(AuctionPallet::phase(), AuctionPhase::WaitingForBids(..));
			// Now move to the next phase, this should be the BidsTaken phase
			assert_matches!(AuctionPallet::process(), Ok(AuctionPhase::BidsTaken(bids))
				if bids == expected_bidding());
			// Read storage to confirm has been changed to BidsTaken
			assert_matches!(AuctionPallet::current_phase(), AuctionPhase::BidsTaken(bids)
				if bids == expected_bidding());
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
					0,
					expected_validating_set().0
				)),
			);
			// Just leaves us to confirm this auction, if we try to process this we will get an error
			// until is confirmed
			assert_matches!(AuctionPallet::process(), Err(AuctionError::NotConfirmed));
			// Confirm the auction
			clear_confirmation();
			// and finally we complete the process, clearing the bidders
			assert_matches!(
				AuctionPallet::process(),
				Ok(AuctionPhase::WaitingForBids(..))
			);
		});
	}

	fn run_auction(number_of_bids: u32) {
		generate_bids(number_of_bids);

		let _ = AuctionPallet::process()
			.and(AuctionPallet::process().and_then(|_| {
				clear_confirmation();
				AuctionPallet::process()
			}))
			.unwrap();
	}

	#[test]
	fn should_create_correct_size_of_groups() {
		let expected_group_sizes = |number_of_bidders: u32| {
			let expected_number_of_validators = min(MAX_VALIDATOR_SIZE, number_of_bidders);
			let expected_number_of_backup_validators =
				expected_number_of_validators / BACKUP_VALIDATOR_RATIO;
			let expected_number_of_passive_nodes = number_of_bidders
				.saturating_sub(expected_number_of_backup_validators)
				.saturating_sub(expected_number_of_validators);
			(
				expected_number_of_validators,
				expected_number_of_backup_validators,
				expected_number_of_passive_nodes,
			)
		};

		new_test_ext().execute_with(|| {
			let validate_states = |nodes: Vec<ValidatorId>, state: ChainflipAccountState| {
				for node in nodes {
					assert_eq!(MockChainflipAccount::get(&node).state, state);
				}
			};

			let validate_bidder_groups = || {
				let number_of_bidders = expected_bidding().len() as u32;
				let (validators_size, backup_validators_size, passive_nodes_size) =
					expected_group_sizes(number_of_bidders);

				match AuctionPallet::current_phase() {
					AuctionPhase::WaitingForBids(validators, _) => {
						assert_eq!(validators_size, validators.len() as u32);
						assert_eq!(backup_validators_size, AuctionPallet::backup_group_size());
						assert_eq!(
							passive_nodes_size,
							AuctionPallet::remaining_bidders().len() as u32
								- AuctionPallet::backup_group_size()
						);
						validate_states(validators, ChainflipAccountState::Validator);

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
					_ => unreachable!("wrong phase"),
				}
			};

			// Validate groups at genesis
			validate_bidder_groups();

			// Run a few auctions and validate groups
			let auction_bidders = [MAX_VALIDATOR_SIZE - 1, MAX_VALIDATOR_SIZE, 100, 200, 1000];
			for bidders in auction_bidders.iter() {
				run_auction(*bidders);
				validate_bidder_groups();
			}

			//We should have our validators states set correctly based on the backup group size parameter
			// match AuctionPallet::current_phase() {
			// 	AuctionPhase::WaitingForBids(validators, _) => {
			// 		let backup_group_size = AuctionPallet::backup_group_size() as usize;
			// 		let remaining = AuctionPallet::remaining_bidders();
			//
			// 		let first_backup_validator = remaining[0];
			// 		let last_backup_validator = remaining[backup_group_size - 1];
			// 		let first_passive = remaining[backup_group_size];
			// 		let last_passive = remaining.last().unwrap();
			// 		assert_eq!(
			// 			MockChainflipAccount::get(&validators[0]).state,
			// 			ChainflipAccountState::Validator
			// 		);
			// 		assert_eq!(
			// 			MockChainflipAccount::get(&validators[(MAX_VALIDATOR_SIZE - 1) as usize])
			// 				.state,
			// 			ChainflipAccountState::Validator
			// 		);
			// 		assert_eq!(
			// 			MockChainflipAccount::get(&first_backup_validator.0).state,
			// 			ChainflipAccountState::Backup
			// 		);
			// 		assert_eq!(
			// 			MockChainflipAccount::get(&last_backup_validator.0).state,
			// 			ChainflipAccountState::Backup
			// 		);
			// 		assert_eq!(
			// 			MockChainflipAccount::get(&first_passive.0).state,
			// 			ChainflipAccountState::Passive
			// 		);
			// 		assert_eq!(
			// 			MockChainflipAccount::get(&last_passive.0).state,
			// 			ChainflipAccountState::Passive
			// 		);
			//
			// 		let minimum_backup_bid = remaining[backup_group_size - 1].1;
			// 		// Update stakes via `HandleStakes`
			// 		// Take the top passive and increase their stake to minimum backup bid plus 1
			// 		// The last backup validator should now be a passive validator
			// 		// and our passive validator is now a backup validator
			// 		HandleStakes::<Test>::stake_updated(first_passive.0, minimum_backup_bid + 1);
			// 		assert_eq!(
			// 			MockChainflipAccount::get(&first_passive.0).state,
			// 			ChainflipAccountState::Backup
			// 		);
			// 		assert_eq!(
			// 			MockChainflipAccount::get(&last_backup_validator.0).state,
			// 			ChainflipAccountState::Passive
			// 		);
			// 		// Update who is who
			// 		let first_passive = remaining[backup_group_size];
			// 		// A backup validator has claimed stake and is now a passive validator
			// 		// and the first passive has been promoted to backup
			// 		HandleStakes::<Test>::stake_updated(first_backup_validator.0, 1);
			// 		assert_eq!(
			// 			MockChainflipAccount::get(&first_backup_validator.0).state,
			// 			ChainflipAccountState::Passive
			// 		);
			// 		assert_eq!(
			// 			MockChainflipAccount::get(&first_passive.0).state,
			// 			ChainflipAccountState::Backup
			// 		);
			// 	}
			// 	_ => {
			// 		panic!("Wrong phase")
			// 	}
			// }
		});
	}

	#[test]
	fn should_update_state_when_stake_shifts_node_into_new_group() {
		new_test_ext().execute_with(|| match AuctionPallet::current_phase() {
			AuctionPhase::WaitingForBids(..) => {
				let backup_validators: Vec<_> = AuctionPallet::remaining_bidders()
					.iter()
					.take(AuctionPallet::backup_group_size() as usize)
					.map(|bid| *bid)
					.collect();

				let passive_nodes: Vec<_> = AuctionPallet::remaining_bidders()
					.iter()
					.skip(AuctionPallet::backup_group_size() as usize)
					.take(usize::MAX)
					.map(|bid| *bid)
					.collect();

				let bottom_of_the_backup_validators = backup_validators.last().unwrap();
				let top_of_the_passive_nodes = passive_nodes.first().unwrap();
				let new_bid = AuctionPallet::lowest_backup_validator_bid() + 1;
				// Promote a passive node to the backup set
				HandleStakes::<Test>::stake_updated(
					top_of_the_passive_nodes.0,
					new_bid,
				);

				assert_eq!(
					MockChainflipAccount::get(&top_of_the_passive_nodes.0).state,
					ChainflipAccountState::Backup
				);

				assert_eq!(
					MockChainflipAccount::get(&bottom_of_the_backup_validators.0).state,
					ChainflipAccountState::Passive
				);

				let backup_validators: Vec<_> = AuctionPallet::remaining_bidders()
					.iter()
					.take(AuctionPallet::backup_group_size() as usize)
					.map(|bid| *bid)
					.collect();

				let passive_nodes: Vec<_> = AuctionPallet::remaining_bidders()
					.iter()
					.skip(AuctionPallet::backup_group_size() as usize)
					.take(usize::MAX)
					.map(|bid| *bid)
					.collect();

				assert_eq!(top_of_the_passive_nodes, backup_validators.last().unwrap());
				assert_eq!(bottom_of_the_backup_validators, passive_nodes.first().unwrap());

				assert_eq!(AuctionPallet::lowest_backup_validator_bid(), new_bid);
			}
			_ => unreachable!("wrong phase"),
		});
	}
	#[test]
	fn changing_range() {
		new_test_ext().execute_with(|| {
			// Assert our minimum is set to 2
			assert_eq!(<Test as Config>::MinValidators::get(), 2);
			// Check we are throwing up an error when we send anything less than the minimum of 2
			assert_noop!(
				AuctionPallet::set_active_validator_range(Origin::root(), (0, 0)),
				Error::<Test>::InvalidRange
			);
			assert_noop!(
				AuctionPallet::set_active_validator_range(Origin::root(), (1, 2)),
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
			generate_bids(2);
			let auction_range = (2, 100);
			assert_ok!(AuctionPallet::set_active_range(auction_range));
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
}
