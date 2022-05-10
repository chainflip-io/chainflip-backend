use crate::{mock::*, *};
use cf_test_utilities::last_event;
use cf_traits::mocks::keygen_exclusion::MockKeygenExclusion;
use frame_support::{assert_noop, assert_ok};

#[test]
fn should_provide_winning_set() {
	new_test_ext().execute_with(|| {
		MockBidderProvider::set_bids(&(1..=10).map(|id| (id, 100)).collect::<Vec<_>>());

		let AuctionOutcome { winners, bond, .. } =
			<AuctionPallet as Auctioneer<Test>>::resolve_auction().expect("the auction should run");

		assert!(!winners.is_empty() && winners.iter().all(|id| *id < 10));
		assert_eq!(bond, 100);

		assert_eq!(
			last_event::<Test>(),
			mock::Event::AuctionPallet(crate::Event::AuctionCompleted(winners, bond)),
		);

		MockBidderProvider::set_bids(&(11..=20).map(|id| (id, 80)).collect::<Vec<_>>());
		let AuctionOutcome { winners, bond, .. } =
			AuctionPallet::resolve_auction().expect("the auction should run");

		assert!(!winners.is_empty() && winners.iter().all(|id| *id > 10));
		assert_eq!(bond, 80);
	});
}

#[test]
fn auction_params_must_be_valid_when_set() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			AuctionPallet::set_auction_parameters(
				Origin::root(),
				DynamicSetSizeParameters::default()
			),
			Error::<Test>::InvalidAuctionParameters
		);

		assert_ok!(AuctionPallet::set_auction_parameters(
			Origin::root(),
			DynamicSetSizeParameters {
				min_size: 3,
				max_size: 10,
				max_contraction: 10,
				max_expansion: 10,
			}
		));
		// Confirm we have an event
		assert!(matches!(
			last_event::<Test>(),
			mock::Event::AuctionPallet(crate::Event::AuctionParametersChanged(..)),
		));
	});
}

#[test]
fn should_exclude_excluded_from_keygen_set() {
	new_test_ext().execute_with(|| {
		MockBidderProvider::set_bids(&(0..10).map(|id| (id, 100)).collect::<Vec<_>>());

		// Designate a set of validators to be excluded from keygen.
		let bad_bidders = MockBidderProvider::get_bidders()
			.iter()
			.take(4)
			.map(|(id, _)| *id)
			.collect::<BTreeSet<_>>();

		MockKeygenExclusion::<Test>::set(bad_bidders.iter().cloned().collect());

		// Confirm we just have the good bidders in our new auction result
		assert!(<AuctionPallet as Auctioneer<Test>>::resolve_auction()
			.expect("we should have an auction")
			.winners
			.iter()
			.cloned()
			.collect::<BTreeSet<_>>()
			.is_disjoint(&bad_bidders));
	});
}
