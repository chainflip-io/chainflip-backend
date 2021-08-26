use crate::{mock::*, BroadcastId, Error, Event as BroadcastEvent, PayloadFor, PendingBroadcasts};
use frame_support::{assert_noop, instances::Instance0};
use frame_system::RawOrigin;

struct MockCfe;

impl MockCfe {
	fn respond() {
		for event_record in System::events() {
			Self::process_event(event_record.event);
		}
	}

	fn process_event(event: Event) {
		match event {
			Event::pallet_cf_transaction_broadcast_Instance0(broadcast_event) => {
				match broadcast_event {
					BroadcastEvent::ThresholdSignatureRequest(id, nominees, payload) => {
						Self::handle_threshold_sig_request(id, nominees, payload)
					}
					BroadcastEvent::TransactionSigningRequest(id, nominee, unsigned_tx) => {
						Self::handle_transaction_signature_request(id, nominee, unsigned_tx)
					}
					BroadcastEvent::ReadyForBroadcast(id, _signed_tx) => {
						// TODO
					},
					BroadcastEvent::__Ignore(_, _) => unimplemented!(),
				}
			}
			_ => panic!("Unexpected event"),
		};
	}

	// Asserts the payload is as expected and returns a super-secure signature.
	fn handle_threshold_sig_request(
		id: BroadcastId,
		signers: Vec<u64>,
		payload: PayloadFor<Test, Instance0>,
	) {
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
		TransactionBroadcast::transaction_ready(RawOrigin::Signed(0).into(), id, MockSignedTx)
			.unwrap();
	}
}

fn broadcast_state(id: BroadcastId) -> Option<MockBroadcast> {
	PendingBroadcasts::<Test, Instance0>::get(id)
}

#[test]
fn test_broadcast_flow() {
	new_test_ext().execute_with(|| {
		let bc = MockBroadcast::New;
		// Construct the payload and request threshold sig.
		assert_eq!(1, TransactionBroadcast::initiate_broadcast(bc));
		assert_eq!(broadcast_state(1), Some(MockBroadcast::PayloadConstructed));
		// CFE posts the signature back on-chain once the threshold sig has been constructed.
		// This triggers a new request to sign the actual tx.
		MockCfe::respond();
		assert_eq!(
			broadcast_state(1),
			Some(MockBroadcast::ThresholdSigReceived(
				b"signed-by-cfe".to_vec()
			))
		);
		// The CFE returns the complete and ready-to-broadcast tx.
		MockCfe::respond();
		assert_eq!(broadcast_state(1), Some(MockBroadcast::Complete));
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
