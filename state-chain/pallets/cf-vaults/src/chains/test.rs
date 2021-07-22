mod test {
	use frame_support::{assert_noop, assert_ok};
	use crate::chains::mock::*;
	use crate::chains::*;
	use crate::rotation::ChainVault;
	use crate::chains::ethereum::EthSigningTxRequest;

	fn last_event() -> mock::Event {
		frame_system::Pallet::<MockRuntime>::events()
			.pop()
			.expect("Event expected")
			.event
	}

	#[test]
	fn try_starting_a_vault_rotation() {
		new_test_ext().execute_with(|| {
			assert_ok!(EthereumPallet::try_start_vault_rotation(0, vec![], vec![]));
			let signing_request = EthSigningTxRequest {
				payload: vec![],
				validators: vec![]
			};
			assert_eq!(last_event(), mock::Event::ethereum(ethereum::Event::EthSignTxRequestEvent(0, signing_request)));
		});
		// Try with index and should fail
		// Create an index - pub(super) next maybe
	}
}
