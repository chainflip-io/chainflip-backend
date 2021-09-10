use crate::{
	mock::*, BroadcastId, BroadcastState, Error, Event as BroadcastEvent, PayloadFor,
	PendingBroadcasts,
};
use frame_support::{assert_noop, instances::Instance0};
use frame_system::RawOrigin;

const KEY_ID: u64 = 42;

struct MockCfe;

impl MockCfe {
	fn respond() {
		let events = System::events();
		System::reset_events();
		for event_record in events {
			Self::process_event(event_record.event);
		}
	}

	fn process_event(event: Event) {
		match event {
			Event::pallet_cf_broadcast_Instance0(broadcast_event) => {
				match broadcast_event {
					BroadcastEvent::ThresholdSignatureRequest(id, key_id, nominees, payload) => {
						Self::handle_threshold_sig_request(id, key_id, nominees, payload)
					}
					BroadcastEvent::TransactionSigningRequest(id, nominee, unsigned_tx) => {
						Self::handle_transaction_signature_request(id, nominee, unsigned_tx)
					}
					BroadcastEvent::BroadcastRequest(id, _signed_tx) => {
						Self::handle_broadcast_request(id)
					}
					BroadcastEvent::BroadcastComplete(_) => {
						// TODO
					}
					BroadcastEvent::__Ignore(_, _) => unimplemented!(),
				}
			}
			_ => panic!("Unexpected event"),
		};
	}

	// Asserts the payload is as expected and returns a super-secure signature.
	fn handle_threshold_sig_request(
		id: BroadcastId,
		key_id: u64,
		signers: Vec<u64>,
		payload: PayloadFor<Test, Instance0>,
	) {
		assert_eq!(key_id, KEY_ID);
		assert_eq!(payload, b"payload");
		assert_eq!(signers, vec![RANDOM_NOMINEE]);
		TransactionBroadcast::signature_ready(
			RawOrigin::Root.into(),
			id,
			b"signed-by-cfe".to_vec(),
		)
		.unwrap();
	}

	// Accepts an unsigned tx, making sure the nominee
	fn handle_transaction_signature_request(
		id: BroadcastId,
		nominee: u64,
		_unsigned_tx: MockUnsignedTx,
	) {
		assert_eq!(nominee, RANDOM_NOMINEE);
		TransactionBroadcast::transaction_ready(RawOrigin::Signed(nominee).into(), id, MockSignedTx)
			.unwrap();
	}

	fn handle_broadcast_request(id: BroadcastId) {
		TransactionBroadcast::broadcast_success(
			RawOrigin::Root.into(),
			id,
			b"0x-tx-hash".to_vec()
		).unwrap();
	}
}

fn broadcast_state(state: BroadcastState, id: BroadcastId) -> Option<MockBroadcast> {
	PendingBroadcasts::<Test, Instance0>::get(state, id)
}

#[test]
fn test_broadcast_happy_path() {
	new_test_ext().execute_with(|| {
		// Construct the payload and request threshold sig.
		assert_eq!(
			1,
			TransactionBroadcast::initiate_broadcast(MockBroadcast::New, KEY_ID).unwrap()
		);
		assert_eq!(
			broadcast_state(BroadcastState::AwaitingThreshold, 1),
			Some(MockBroadcast::New)
		);
		// CFE posts the signature back on-chain once the threshold sig has been constructed.
		// This triggers a new request to sign the actual tx.
		MockCfe::respond();
		assert_eq!(
			broadcast_state(BroadcastState::AwaitingSignature, 1),
			Some(MockBroadcast::ThresholdSigReceived(
				b"signed-by-cfe".to_vec()
			))
		);
		// The CFE returns the complete and ready-to-broadcast tx.
		MockCfe::respond();
		// This triggers transaction verification and a broadcast request.
		assert_eq!(
			broadcast_state(BroadcastState::AwaitingBroadcast, 1),
			Some(MockBroadcast::ThresholdSigReceived(
				b"signed-by-cfe".to_vec()
			))
		);
		// The CFE will respond that the transaction is complete.
		MockCfe::respond();
		assert_eq!(
			broadcast_state(BroadcastState::Complete, 1),
			Some(MockBroadcast::Complete)
		);
	})
}

#[test]
fn test_invalid_id_is_noop() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			TransactionBroadcast::signature_ready(RawOrigin::Root.into(), 0, b"".to_vec(),),
			Error::<Test, Instance0>::InvalidBroadcastId
		);
		assert_noop!(
			TransactionBroadcast::transaction_ready(RawOrigin::Signed(0).into(), 0, MockSignedTx,),
			Error::<Test, Instance0>::InvalidBroadcastId
		);
	})
}
