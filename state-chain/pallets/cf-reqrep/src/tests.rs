use crate::{self as pallet_reqrep, mock::*};
use frame_support::assert_ok;
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
			Event::pallet_reqrep_Instance0(pallet_reqrep::Event::Request(id, req)) => {
				assert_eq!(req, ping_pong::Ping);
				assert_ok!(PingPongRequestResponse::reply(RawOrigin::Signed(0).into(), id, ping_pong::Pong));
			},
			_ => panic!("Unexpected event"),
		};
	}
}

#[test]
fn ping_pong() {
	new_test_ext().execute_with(|| {
		PingPongRequestResponse::request(ping_pong::Ping);
		MockCfe::respond();
	});
}
