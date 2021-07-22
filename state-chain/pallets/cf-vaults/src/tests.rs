mod test {
	use crate::mock::*;
	use crate::*;
	use frame_support::{assert_noop, assert_ok};

	// Notes for test plan

	fn last_event() -> mock::Event {
		frame_system::Pallet::<MockRuntime>::events()
			.pop()
			.expect("Event expected")
			.event
	}

	#[test]
	fn try_index() {

		// Try with index and should fail
		// Create an index - pub(super) next maybe
		VaultsPallet::abort_rotation();
	}

	#[test]
	fn initiate_key_generation_request() {
		new_test_ext().execute_with(|| {
			// on_completed with an empty set of validators
			// 1.result error "EmptyValidatorSet"
			// 2.with set
			// Returns ok
			// creates new index, confirm it has increased "request_idx"
			// with this new index we have a new entry in "vault_rotations" with the keygen request object
			// Event emitted KeygenRequestEvent
		});
	}

	#[test]
	fn provide_keygen_response() {
		new_test_ext().execute_with(|| {
			// on_completed with an empty set of validators
			// 2.with set
			// Returns ok
			// Get new request_idx
			// Create KeygenResponse and call extrinsic
			// Try with invalid request idx, error
			// Success with public key
		});
	}
}
