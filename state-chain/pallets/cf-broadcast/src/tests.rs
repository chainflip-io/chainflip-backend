use crate::{
	mock::*, AwaitingBroadcast, AwaitingSignature, BroadcastFailure, BroadcastId, Error, RetryQueue,
	Event as BroadcastEvent, Instance0
};
use frame_support::{assert_noop, assert_ok, traits::Hooks};
use frame_system::RawOrigin;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Scenario {
	HappyPath,
	UnhappyPath(BroadcastFailure),
}

thread_local! {
	pub static COMPLETED_BROADCASTS: std::cell::RefCell<Vec<BroadcastId>> = Default::default();
	pub static FAILED_BROADCASTS: std::cell::RefCell<Vec<BroadcastId>> = Default::default();
}

struct MockCfe;

impl MockCfe {
	fn respond(scenario: Scenario) {
		let events = System::events();
		System::reset_events();
		for event_record in events {
			Self::process_event(event_record.event, scenario.clone());
		}
	}

	fn process_event(event: Event, scenario: Scenario) {
		match event {
			Event::pallet_cf_broadcast_Instance0(broadcast_event) => match broadcast_event {
				BroadcastEvent::TransactionSigningRequest(id, nominee, unsigned_tx) => {
					Self::handle_transaction_signature_request(id, nominee, unsigned_tx);
				}
				BroadcastEvent::BroadcastRequest(id, _signed_tx) => {
					Self::handle_broadcast_request(id, scenario);
				}
				BroadcastEvent::BroadcastComplete(id) => {
					COMPLETED_BROADCASTS.with(|cell| cell.borrow_mut().push(id));
				}
				BroadcastEvent::RetryScheduled(_, _) => {
					// Informational only. No action required by the CFE.
				},
				BroadcastEvent::BroadcastFailed(id, _, _) => {
					FAILED_BROADCASTS.with(|cell| cell.borrow_mut().push(id));
				},
				BroadcastEvent::__Ignore(_, _) => unimplemented!(),
			},
			_ => panic!("Unexpected event"),
		};
	}

	// Accepts an unsigned tx, making sure the nominee has been assigned.
	fn handle_transaction_signature_request(
		id: BroadcastId,
		nominee: u64,
		_unsigned_tx: MockUnsignedTx,
	) {
		assert_eq!(nominee, RANDOM_NOMINEE);
		// Invalid signer refused.
		assert_noop!(
			DogeBroadcast::transaction_ready(
				RawOrigin::Signed(nominee + 1).into(),
				id,
				MockSignedTx,
			),
			Error::<Test, Instance0>::InvalidSigner
		);
		// Only the nominee can return the signed tx.
		assert_ok!(DogeBroadcast::transaction_ready(
			RawOrigin::Signed(nominee).into(),
			id,
			MockSignedTx,
		));
	}

	// Simulate different outcomes.
	fn handle_broadcast_request(id: BroadcastId, scenario: Scenario) {
		assert_ok!(match scenario {
			Scenario::HappyPath => DogeBroadcast::broadcast_success(Origin::root(), id, [0xcf; 4]),
			Scenario::UnhappyPath(failure) => {
				DogeBroadcast::broadcast_failure(Origin::root(), id, failure, [0xcf; 4])
			}
		});
	}
}

#[test]
fn test_broadcast_happy_path() {
	new_test_ext().execute_with(|| {
		const BROADCAST_ID: BroadcastId = 1;

		// Initiate broadcast
		assert_ok!(DogeBroadcast::start_sign_and_broadcast(
			Origin::root(),
			MockUnsignedTx
		));
		assert!(AwaitingSignature::<Test, Instance0>::get(BROADCAST_ID).is_some());

		// CFE responds with a signed transaction. This moves us to the broadcast stage.
		MockCfe::respond(Scenario::HappyPath);
		assert!(AwaitingSignature::<Test, Instance0>::get(BROADCAST_ID).is_none());
		assert!(AwaitingBroadcast::<Test, Instance0>::get(BROADCAST_ID).is_some());

		// CFE responds again with confirmation of a successful broadcast.
		MockCfe::respond(Scenario::HappyPath);
		assert!(AwaitingSignature::<Test, Instance0>::get(BROADCAST_ID).is_none());
		assert!(AwaitingBroadcast::<Test, Instance0>::get(BROADCAST_ID).is_none());

		// CFE logs the completed broadcast.
		MockCfe::respond(Scenario::HappyPath);
		assert_eq!(
			COMPLETED_BROADCASTS.with(|cell| *cell.borrow().first().unwrap()),
			BROADCAST_ID
		);
	})
}

#[test]
fn test_broadcast_rejected() {
	new_test_ext().execute_with(|| {
		const BROADCAST_ID: BroadcastId = 1;

		// Initiate broadcast
		assert_ok!(DogeBroadcast::start_sign_and_broadcast(
			Origin::root(),
			MockUnsignedTx
		));
		assert!(AwaitingSignature::<Test, Instance0>::get(BROADCAST_ID).unwrap().attempt == 0);

		// CFE responds with a signed transaction. This moves us to the broadcast stage.
		MockCfe::respond(Scenario::HappyPath);
		assert!(AwaitingSignature::<Test, Instance0>::get(BROADCAST_ID).is_none());
		assert!(AwaitingBroadcast::<Test, Instance0>::get(BROADCAST_ID).is_some());

		// CFE responds that the transaction was rejected.
		MockCfe::respond(Scenario::UnhappyPath(BroadcastFailure::TransactionRejected));
		assert!(AwaitingSignature::<Test, Instance0>::get(BROADCAST_ID).is_none());
		assert!(AwaitingBroadcast::<Test, Instance0>::get(BROADCAST_ID).is_none());
		assert_eq!(RetryQueue::<Test, Instance0>::decode_len().unwrap_or_default(), 1);

		// The `on_initialize` hook is called and triggers a new broadcast attempt.
		DogeBroadcast::on_initialize(0);
		assert_eq!(RetryQueue::<Test, Instance0>::decode_len().unwrap_or_default(), 0);
		assert!(AwaitingSignature::<Test, Instance0>::get(BROADCAST_ID + 1).unwrap().attempt == 1);
	})
}

#[test]
fn test_broadcast_failed() {
	new_test_ext().execute_with(|| {
		const BROADCAST_ID: BroadcastId = 1;

		// Initiate broadcast
		assert_ok!(DogeBroadcast::start_sign_and_broadcast(
			Origin::root(),
			MockUnsignedTx
		));
		assert!(
			AwaitingSignature::<Test, Instance0>::get(BROADCAST_ID)
				.unwrap()
				.attempt == 0
		);

		// CFE responds with a signed transaction. This moves us to the broadcast stage.
		MockCfe::respond(Scenario::HappyPath);
		assert!(AwaitingSignature::<Test, Instance0>::get(BROADCAST_ID).is_none());
		assert!(AwaitingBroadcast::<Test, Instance0>::get(BROADCAST_ID).is_some());

		// CFE responds that the transaction failed.
		MockCfe::respond(Scenario::UnhappyPath(BroadcastFailure::TransactionFailed));
		assert!(AwaitingSignature::<Test, Instance0>::get(BROADCAST_ID).is_none());
		assert!(AwaitingBroadcast::<Test, Instance0>::get(BROADCAST_ID).is_none());

		// We don't retry.
		assert_eq!(
			RetryQueue::<Test, Instance0>::decode_len().unwrap_or_default(),
			0
		);
		// The broadcast has failed.
		MockCfe::respond(Scenario::UnhappyPath(BroadcastFailure::TransactionFailed));
		assert_eq!(
			FAILED_BROADCASTS.with(|cell| *cell.borrow().first().unwrap()),
			BROADCAST_ID
		);
	})
}

#[test]
fn test_invalid_id_is_noop() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			DogeBroadcast::transaction_ready(RawOrigin::Signed(0).into(), 0, MockSignedTx),
			Error::<Test, Instance0>::InvalidBroadcastId
		);
		assert_noop!(
			DogeBroadcast::broadcast_success(Origin::root(), 0, [0u8; 4]),
			Error::<Test, Instance0>::InvalidBroadcastId
		);
		assert_noop!(
			DogeBroadcast::broadcast_failure(
				Origin::root(),
				0,
				BroadcastFailure::TransactionFailed,
				[0u8; 4]
			),
			Error::<Test, Instance0>::InvalidBroadcastId
		);
	})
}
