use super::*;
use cf_chains::{address::EncodedAddress, CcmChannelMetadata};
use cf_primitives::Asset;
use codec::Encode;
use sp_runtime::BoundedVec;
use state_chain_runtime::{EthereumInstance, RuntimeEvent};

fn test_event_encoding(
	registry: &state_chain_runtime::chainflip::RuntimeEventDecoder,
	event: RuntimeEvent,
) {
	let encoded = event.encode().to_vec();
	println!("Encoded: \n{:?}\n", hex::encode(encoded.clone()));
	let to_string = registry.decode_event_to_string(encoded.clone()).unwrap();
	let to_string_json = registry.decode_event_to_string_json(encoded).unwrap();
	let from_string = registry.decode_event_from_string(to_string.clone()).unwrap();

	assert_eq!(to_string, from_string.to_string());
	println!("value to_string: \n{}\n", to_string);
	println!("value to_string Json: \n{}\n", to_string_json);
	println!("value from string: \n{}\n", from_string);
}

#[test]
fn can_decode_runtime_events() {
	super::genesis::with_test_defaults().build().execute_with(|| {
		let registry = state_chain_runtime::chainflip::RuntimeEventDecoder::new();

		test_event_encoding(
			&registry,
			RuntimeEvent::System(frame_system::Event::<Runtime>::Remarked {
				sender: AccountId::from(ALICE),
				hash: [0xFF; 32].into(),
			}),
		);

		test_event_encoding(
			&registry,
			RuntimeEvent::EthereumBroadcaster(pallet_cf_broadcast::Event::<
				Runtime,
				EthereumInstance,
			>::BroadcastSuccess {
				broadcast_id: 123u32,
				transaction_out_id: cf_chains::evm::SchnorrVerificationComponents {
					s: [0xBB; 32],
					k_times_g_address: [0xAA; 20],
				},
				transaction_ref: [0xCC; 32].into(),
			}),
		);

		test_event_encoding(
			&registry,
			RuntimeEvent::Swapping(pallet_cf_swapping::Event::<Runtime>::SwapDepositAddressReady {
				deposit_address: EncodedAddress::Eth([0xDD; 20]),
				destination_address: EncodedAddress::Eth([0xDD; 20]),
				source_asset: Asset::Flip,
				destination_asset: Asset::Usdc,
				channel_id: 55u64,
				broker_commission_rate: 100u16,
				channel_metadata: Some(CcmChannelMetadata {
					message: BoundedVec::try_from(vec![0x00, 0x01, 0x02, 0x03, 0x04]).unwrap(),
					gas_budget: 1_000_000u128,
					cf_parameters: BoundedVec::try_from(vec![0x10, 0x11, 0x12, 0x13, 0x14])
						.unwrap(),
				}),
				source_chain_expiry_block: 1_000u64,
				boost_fee: 9u16,
			}),
		);
	});
}
