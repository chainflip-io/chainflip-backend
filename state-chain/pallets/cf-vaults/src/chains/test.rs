mod test {
	use crate::chains::ethereum::{EthSigningTxRequest, EthSigningTxResponse, Error};
	use crate::chains::mock::*;
	use crate::chains::*;
	use crate::rotation::ChainVault;
	use frame_support::{assert_noop, assert_ok};

	fn last_event() -> mock::Event {
		frame_system::Pallet::<MockRuntime>::events()
			.pop()
			.expect("Event expected")
			.event
	}

	#[test]
	fn try_starting_a_vault_rotation() {
		new_test_ext().execute_with(|| {
			assert_ok!(EthereumPallet::try_start_vault_rotation(
				0,
				vec![],
				vec![ALICE, BOB, CHARLIE]
			));
			let signing_request = EthSigningTxRequest {
				payload: EthereumPallet::encode_set_agg_key_with_agg_key(vec![]).unwrap(),
				validators: vec![ALICE, BOB, CHARLIE],
			};
			assert_eq!(
				last_event(),
				mock::Event::ethereum(ethereum::Event::EthSignTxRequestEvent(0, signing_request))
			);
		});
	}

	#[test]
	fn witness_eth_signing_tx_response() {
		new_test_ext().execute_with(|| {
			assert_ok!(EthereumPallet::witness_eth_signing_tx_response(
				Origin::signed(ALICE),
				0,
				EthSigningTxResponse::Success(vec![])
			));

			assert_noop!(EthereumPallet::witness_eth_signing_tx_response(
				Origin::signed(ALICE),
				0,
				EthSigningTxResponse::Error(vec![1, 2, 3])
			), Error::<MockRuntime>::EthSigningTxResponseFailed);
		});
	}
}
