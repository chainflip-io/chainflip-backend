mod tests {
	use crate::{mock::*, Error, *};
	use cf_traits::{mocks::vault_rotation::clear_confirmation, IsOutgoing};
	use frame_support::{assert_noop, assert_ok};
	use sp_runtime::traits::{BadOrigin, Zero};

	const ALICE: u64 = 100;
	const BOB: u64 = 101;

	fn last_event() -> mock::Event {
		frame_system::Pallet::<Test>::events().pop().expect("Event expected").event
	}

	fn assert_winners() -> Vec<ValidatorId> {
		if let AuctionPhase::ValidatorsSelected(winners, _) = AuctionPallet::phase() {
			return winners
		}
		panic!("Expected `ValidatorsSelected` auction phase, got {:?}", AuctionPallet::phase());
	}

	#[test]
	fn you_have_to_be_priviledged() {
		new_test_ext().execute_with(|| {
			// Run through the sudo extrinsics to be sure they are what they are
			assert_noop!(
				ValidatorPallet::set_blocks_for_epoch(Origin::signed(ALICE), Zero::zero()),
				BadOrigin
			);
			assert_noop!(ValidatorPallet::force_rotation(Origin::signed(ALICE)), BadOrigin);
		});
	}

	#[test]
	fn changing_epoch() {
		new_test_ext().execute_with(|| {
			// Confirm we have a minimum epoch of 1 block
			assert_eq!(<Test as Config>::MinEpoch::get(), 1);
			// Throw up an error if we supply anything less than this
			assert_noop!(
				ValidatorPallet::set_blocks_for_epoch(Origin::root(), 0),
				Error::<Test>::InvalidEpoch
			);
			// This should work as 2 > 1
			assert_ok!(ValidatorPallet::set_blocks_for_epoch(Origin::root(), 2));
			// Confirm we have an event for the change from 0 to 2
			assert_eq!(
				last_event(),
				mock::Event::ValidatorPallet(crate::Event::EpochDurationChanged(0, 2)),
			);
			// We throw up an error if we try to set it to the current
			assert_noop!(
				ValidatorPallet::set_blocks_for_epoch(Origin::root(), 2),
				Error::<Test>::InvalidEpoch
			);
		});
	}

	#[test]
	fn should_end_session() {
		new_test_ext().execute_with(|| {
			let set_size = 10;
			assert_ok!(AuctionPallet::set_active_range((2, set_size)));
			// Set block length of epoch to 10
			let epoch = 10;
			assert_ok!(ValidatorPallet::set_blocks_for_epoch(Origin::root(), epoch));
			// If we are in the bidder phase we should check if we have a force auction or
			// epoch has expired
			// Test force rotation
			assert_ok!(ValidatorPallet::force_rotation(Origin::root()));
			// Test we are in the bidder phase
			assert_matches!(AuctionPallet::phase(), AuctionPhase::WaitingForBids);
			// Move forward by 1 block, we have a block already
			run_to_block(2);
			assert_matches!(AuctionPallet::phase(), AuctionPhase::ValidatorsSelected(..));
			// Confirm the auction
			clear_confirmation();
			// Move forward by 1 block
			run_to_block(3);
			assert_matches!(AuctionPallet::phase(), AuctionPhase::WaitingForBids);
			// Move forward by 1 block, we should sit in the non-auction phase 'WaitingForBids'
			run_to_block(5);
			assert_matches!(AuctionPallet::phase(), AuctionPhase::WaitingForBids);
			// Epoch is block 10 so let's test an epoch cycle to provoke an auction
			// This should be the same state
			run_to_block(9);
			assert_matches!(AuctionPallet::phase(), AuctionPhase::WaitingForBids);
			let next_epoch = ValidatorPallet::current_epoch_started_at() + epoch;
			run_to_block(next_epoch);
			// We should have started another auction
			assert_matches!(AuctionPallet::phase(), AuctionPhase::ValidatorsSelected(..));
			assert_noop!(
				ValidatorPallet::set_blocks_for_epoch(Origin::root(), 10),
				Error::<Test>::AuctionInProgress
			);
			// Finally back to the start again
			// Confirm the auction
			clear_confirmation();
			run_to_block(next_epoch + 1);
			assert_matches!(AuctionPallet::phase(), AuctionPhase::WaitingForBids);
		});
	}

	#[test]
	fn rotation() {
		// We expect from our `DummyAuction` that we will have our bidders which are then
		// ran through an auction and that the winners of this auction become the validating set
		new_test_ext().execute_with(|| {
			let set_size = 10;
			assert_ok!(AuctionPallet::set_active_range((2, set_size)));
			// Set block length of epoch to 10
			let epoch = 10;
			assert_ok!(ValidatorPallet::set_blocks_for_epoch(Origin::root(), epoch));
			// At genesis we should have our dummy validators
			assert_eq!(mock::current_validators().len(), DUMMY_GENESIS_VALIDATORS.len());
			// ---------- Run Auction
			// Confirm we are in the waiting state
			assert_matches!(AuctionPallet::phase(), AuctionPhase::WaitingForBids);
			// Run to the epoch
			run_to_block(10);
			// We should have now completed an auction have a set of winners to pass as validators
			let winners = assert_winners();
			assert_eq!(
				<ValidatorPallet as EpochInfo>::current_validators(),
				&DUMMY_GENESIS_VALIDATORS[..]
			);
			// run more block to make them validators
			run_to_block(11);
			// Continue with our current validator set, as we had none should still be the genesis
			// set
			assert_eq!(
				<ValidatorPallet as EpochInfo>::current_validators(),
				&DUMMY_GENESIS_VALIDATORS[..]
			);
			// Complete the cycle
			run_to_block(12);
			// As we haven't confirmed the auction we would still be in the same phase
			assert_matches!(AuctionPallet::phase(), AuctionPhase::ValidatorsSelected(..));
			run_to_block(13);
			// and still...
			assert_matches!(AuctionPallet::phase(), AuctionPhase::ValidatorsSelected(..));
			// Confirm the auction
			clear_confirmation();
			run_to_block(14);
			assert_matches!(AuctionPallet::phase(), AuctionPhase::WaitingForBids);
			assert_eq!(<ValidatorPallet as EpochInfo>::epoch_index(), 2);
			// We do now see our winners as the set of validators
			assert_eq!(<ValidatorPallet as EpochInfo>::current_validators(), winners);
			// Force an auction at the next block
			assert_ok!(ValidatorPallet::force_rotation(Origin::root()));
			run_to_block(15);
			// A new auction starts
			// We should still see the old winners validating
			assert_eq!(<ValidatorPallet as EpochInfo>::current_validators(), winners);
			// Our new winners are
			// We should still see the old winners validating
			let winners = assert_winners();
			// Confirm the auction
			clear_confirmation();
			run_to_block(16);

			let outgoing_validators = outgoing_validators();
			for outgoer in &outgoing_validators {
				assert!(MockIsOutgoing::is_outgoing(outgoer));
			}
			// Finalised auction, waiting for bids again
			assert_matches!(AuctionPallet::phase(), AuctionPhase::WaitingForBids);
			assert_eq!(<ValidatorPallet as EpochInfo>::epoch_index(), 3);
			// We have the new set of validators
			assert_eq!(<ValidatorPallet as EpochInfo>::current_validators(), winners);
			// The pallet has the same set of validators as those in the session pallet(those that
			// are securing the network)
			assert_eq!(<ValidatorPallet as EpochInfo>::current_validators(), Session::validators());
		});
	}

	#[test]
	fn should_repeat_auction_after_aborted_auction() {
		new_test_ext().execute_with(|| {
			let set_size = 10;
			assert_ok!(AuctionPallet::set_active_range((2, set_size)));
			// Set block length of epoch to 100
			let epoch = 100;
			assert_ok!(ValidatorPallet::set_blocks_for_epoch(Origin::root(), epoch));
			// Confirm we are in the waiting state
			assert_matches!(AuctionPallet::phase(), AuctionPhase::WaitingForBids);
			// Run to epoch, auction will start
			run_to_block(epoch);
			assert_eq!(AuctionPallet::current_auction_index(), 1, "should see a new auction");
			// Validators are selected
			assert_matches!(AuctionPallet::phase(), AuctionPhase::ValidatorsSelected(..));
			// Abort the current auction
			<AuctionPallet as Auctioneer>::abort();
			// Back to initial state
			assert_matches!(AuctionPallet::phase(), AuctionPhase::WaitingForBids);
			// Move to next block, a new auction starts
			run_to_block(epoch + 1);
			assert_eq!(AuctionPallet::current_auction_index(), 2, "should be at the next auction");
			// Another set of validators selected
			assert_matches!(AuctionPallet::phase(), AuctionPhase::ValidatorsSelected(..));
		});
	}

	#[test]
	fn should_repeat_auction_after_forcing_auction_and_then_aborted_auction() {
		new_test_ext().execute_with(|| {
			let set_size = 10;
			assert_ok!(AuctionPallet::set_active_range((2, set_size)));
			// Set block length of epoch to 100
			let epoch = 100;
			assert_ok!(ValidatorPallet::set_blocks_for_epoch(Origin::root(), epoch));
			// Confirm we are in the waiting state
			assert_matches!(AuctionPallet::phase(), AuctionPhase::WaitingForBids);
			// Force a rotation, auction will start
			assert_ok!(ValidatorPallet::force_rotation(Origin::root()));
			run_to_block(System::block_number() + 1);
			assert_eq!(AuctionPallet::current_auction_index(), 1, "should see a new auction");
			// Validators are selected
			assert_matches!(AuctionPallet::phase(), AuctionPhase::ValidatorsSelected(..));
			// Abort the current auction
			<AuctionPallet as Auctioneer>::abort();
			// Back to initial state
			assert_matches!(AuctionPallet::phase(), AuctionPhase::WaitingForBids);
			// Move to next block, a new auction starts
			run_to_block(System::block_number() + 2);
			assert_eq!(AuctionPallet::current_auction_index(), 2, "should be at the next auction");
			// Another set of validators selected
			assert_matches!(AuctionPallet::phase(), AuctionPhase::ValidatorsSelected(..));
		});
	}

	#[test]
	fn genesis() {
		new_test_ext().execute_with(|| {
			// We should have a set of validators on genesis with a minimum bid of 0 set
			assert_eq!(
				current_validators().len(),
				DUMMY_GENESIS_VALIDATORS.len(),
				"We shouldn have a set of validators at genesis"
			);
			assert_eq!(min_bid(), 0, "We should have a minimum bid of zero");
			assert_eq!(
				ValidatorPallet::current_epoch(),
				1,
				"the first epoch should be the first epoch"
			);
		});
	}

	#[test]
	fn send_cfe_version() {
		new_test_ext().execute_with(|| {
			// We initially submit version
			let validator = DUMMY_GENESIS_VALIDATORS[0];

			let version = SemVer { major: 4, ..Default::default() };
			assert_ok!(ValidatorPallet::cfe_version(Origin::signed(validator), version.clone(),));

			assert_eq!(
				last_event(),
				mock::Event::ValidatorPallet(crate::Event::CFEVersionUpdated(
					validator,
					SemVer::default(),
					version.clone()
				)),
				"should emit event on updated version"
			);

			assert_eq!(
				version.clone(),
				ValidatorPallet::validator_cfe_version(validator),
				"version should be stored"
			);

			// We submit a new version
			let new_version = SemVer { major: 5, ..Default::default() };
			assert_ok!(ValidatorPallet::cfe_version(
				Origin::signed(validator),
				new_version.clone()
			));

			assert_eq!(
				last_event(),
				mock::Event::ValidatorPallet(crate::Event::CFEVersionUpdated(
					validator,
					version.clone(),
					new_version.clone()
				)),
				"should emit event on updated version"
			);

			assert_eq!(
				new_version,
				ValidatorPallet::validator_cfe_version(validator),
				"new version should be stored"
			);

			// When we submit the same version we should see no `CFEVersionUpdated` event
			frame_system::Pallet::<Test>::reset_events();
			assert_ok!(ValidatorPallet::cfe_version(
				Origin::signed(validator),
				new_version.clone()
			));

			assert_eq!(
				0,
				frame_system::Pallet::<Test>::events().len(),
				"We should have no events of an update"
			);

			assert_eq!(
				new_version,
				ValidatorPallet::validator_cfe_version(validator),
				"we should be still on the same new version"
			);
		});
	}

	#[test]
	fn register_peer_id() {
		new_test_ext().execute_with(|| {
			use sp_core::{Encode, Pair};

			let alice_peer_keypair = sp_core::ed25519::Pair::from_legacy_string("alice", None);
			let alice_peer_public_key = alice_peer_keypair.public();

			// Don't allow invalid signatures
			assert_noop!(
				ValidatorPallet::register_peer_id(
					Origin::signed(ALICE),
					alice_peer_public_key,
					0,
					0,
					alice_peer_keypair.sign(&BOB.encode()[..]),
				),
				Error::<Test>::InvalidAccountPeerMappingSignature
			);

			// Non-overlaping peer ids and valid signatures
			assert_ok!(ValidatorPallet::register_peer_id(
				Origin::signed(ALICE),
				alice_peer_public_key,
				40044,
				10,
				alice_peer_keypair.sign(&ALICE.encode()[..]),
			));
			assert_eq!(
				last_event(),
				mock::Event::ValidatorPallet(crate::Event::PeerIdRegistered(
					ALICE,
					alice_peer_public_key,
					40044,
					10
				)),
				"should emit event on register peer id"
			);
			assert_eq!(ValidatorPallet::mapped_peer(&alice_peer_public_key), Some(()));
			assert_eq!(
				ValidatorPallet::validator_peer_id(&ALICE),
				Some((ALICE, alice_peer_public_key, 40044, 10))
			);

			// New mappings to overlapping peer id are disallowed
			assert_noop!(
				ValidatorPallet::register_peer_id(
					Origin::signed(BOB),
					alice_peer_public_key,
					0,
					0,
					alice_peer_keypair.sign(&BOB.encode()[..]),
				),
				Error::<Test>::AccountPeerMappingOverlap
			);

			// New validator mapping works
			let bob_peer_keypair = sp_core::ed25519::Pair::from_legacy_string("bob", None);
			let bob_peer_public_key = bob_peer_keypair.public();
			assert_ok!(ValidatorPallet::register_peer_id(
				Origin::signed(BOB),
				bob_peer_public_key,
				40043,
				11,
				bob_peer_keypair.sign(&BOB.encode()[..]),
			),);
			assert_eq!(
				last_event(),
				mock::Event::ValidatorPallet(crate::Event::PeerIdRegistered(
					BOB,
					bob_peer_public_key,
					40043,
					11
				)),
				"should emit event on register peer id"
			);
			assert_eq!(ValidatorPallet::mapped_peer(&bob_peer_public_key), Some(()));
			assert_eq!(
				ValidatorPallet::validator_peer_id(&BOB),
				Some((BOB, bob_peer_public_key, 40043, 11))
			);

			// Changing existing mapping to overlapping peer id is disallowed
			assert_noop!(
				ValidatorPallet::register_peer_id(
					Origin::signed(BOB),
					alice_peer_public_key,
					0,
					0,
					alice_peer_keypair.sign(&BOB.encode()[..]),
				),
				Error::<Test>::AccountPeerMappingOverlap
			);

			let bob_peer_keypair = sp_core::ed25519::Pair::from_legacy_string("bob2", None);
			let bob_peer_public_key = bob_peer_keypair.public();

			// Changing to new peer id works
			assert_ok!(ValidatorPallet::register_peer_id(
				Origin::signed(BOB),
				bob_peer_public_key,
				40043,
				11,
				bob_peer_keypair.sign(&BOB.encode()[..]),
			));
			assert_eq!(
				last_event(),
				mock::Event::ValidatorPallet(crate::Event::PeerIdRegistered(
					BOB,
					bob_peer_public_key,
					40043,
					11
				)),
				"should emit event on register peer id"
			);
			assert_eq!(ValidatorPallet::mapped_peer(&bob_peer_public_key), Some(()));
			assert_eq!(
				ValidatorPallet::validator_peer_id(&BOB),
				Some((BOB, bob_peer_public_key, 40043, 11))
			);
		});
	}

	#[test]
	fn should_predict_next_epoch() {
		new_test_ext().execute_with(|| {
			let epoch = 100;
			assert_ok!(ValidatorPallet::set_blocks_for_epoch(Origin::root(), epoch));
			assert_eq!(
				<ValidatorPallet as EpochInfo>::next_expected_epoch(),
				epoch,
				"Our first epoch would be at block {}",
				epoch
			);
			assert_ok!(ValidatorPallet::force_rotation(Origin::root()));
			// We are on block 1, move forward 2 blocks
			run_to_block(1 + 2);
			clear_confirmation();
			// We are on block 3 move forward 1 block
			run_to_block(3 + 1);
			// Our next epoch is expected epoch(100) + 4
			assert_eq!(
				<ValidatorPallet as EpochInfo>::next_expected_epoch(),
				epoch + 4,
				"Our next expected epoch would be at block {} + 4",
				epoch
			);
		});
	}
}
