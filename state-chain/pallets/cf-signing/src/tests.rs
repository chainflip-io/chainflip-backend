use crate::{self as pallet_cf_signing, mock::*, Error};
use frame_support::{assert_ok, assert_noop};
use frame_support::instances::Instance0;

struct MockCfe;

pub const SIGNATURE: &'static str = "Wow!";

impl MockCfe {
	fn respond() {
		for event_record in System::events() {
			Self::process_event(event_record.event);
		}
	}

	fn process_event(event: Event) {
		match event {
			Event::pallet_cf_signing_Instance0(
				pallet_cf_signing::Event::ThresholdSignatureRequest(
					req_id,
					key_id,
					signers,
					payload,
				),
			) => {
				assert_eq!(key_id, DOGE_KEY_ID);
				assert_eq!(signers, vec![RANDOM_NOMINEE]);
				assert_eq!(payload, DOGE_PAYLOAD);

				assert_ok!(DogeSigning::signature_success(Origin::root(), req_id, SIGNATURE.to_string()));
			}
			_ => panic!("Unexpected event"),
		};
	}
}

#[test]
fn happy_path() {
	new_test_ext().execute_with(|| {
		let request_id = DogeSigning::request_signature(DogeSigningContext {
			message: "Amazing!".to_string(),
		});
		assert!(DogeSigning::pending_request(request_id).is_some());
		assert_noop!(
			DogeSigning::signature_success(
				Origin::root(),
				request_id + 1,
				"MaliciousSignature".to_string()
			),
			Error::<Test, Instance0>::InvalidRequestId
		);

		MockCfe::respond();

		assert!(DogeSigning::pending_request(request_id).is_none());
		assert_eq!(
			MockCallback::<DogeSigningContext>::get_stored_callback(),
			Some("So Amazing! Such Wow!".to_string())
		);
	});
}
