use crate::{self as pallet_cf_transaction_broadcast, Error, PendingRequests, mock::*};
use frame_support::{assert_noop, assert_ok};
use frame_system::RawOrigin;
use frame_support::instances::Instance0;

struct MockCfe;

impl MockCfe {
	fn respond() {
		for event_record in System::events() {
			Self::process_event(event_record.event);
		}
	}

	fn process_event(event: Event) {
		match event {
			_ => panic!("Unexpected event"),
		};
	}
}
