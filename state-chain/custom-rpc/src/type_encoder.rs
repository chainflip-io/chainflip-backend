/// This file contains a "decoder" for Types that derives from Scale's Type Info.
/// You can use this decoder to decode encoded types into scale_value::Value,
/// which can then be Serialized.
use scale_info::{PortableRegistry, TypeInfo};
use std::vec::Vec;

/// A struct that provides interface for decoding RuntimeEvents.
pub struct TypeDecoder {
	registry: PortableRegistry,
	type_id: u32,
}

impl TypeDecoder {
	/// Creates and returns an instance of a PortableRegistry, used for decoding Runtime Events.
	pub fn new<T: TypeInfo + 'static>() -> Self {
		// We can get the 'portable' type info using scale_info.
		let meta = scale_info::MetaType::new::<T>();
		let mut registry = scale_info::Registry::new();
		let id = registry.register_type(&meta).id;

		Self { registry: PortableRegistry::from(registry), type_id: id }
	}

	pub fn decode_data(&self, data: Vec<u8>) -> scale_value::Value {
		scale_value::scale::decode_as_type(&mut &*data.clone(), &self.type_id, &self.registry)
			.map(|value| value.remove_context())
			.unwrap_or_else(|err| {
				log::error!(
					"Failed to decode Runtime error. Error: {}, Raw data: \n{:?}",
					err,
					data.clone()
				);
				Self::unknown_type(data)
			})
	}

	fn unknown_type(bytes: Vec<u8>) -> scale_value::Value {
		scale_value::Value::named_composite([(
			"UnknownData",
			scale_value::Value::string(hex::encode(bytes)),
		)])
	}
}

#[cfg(test)]
pub mod test {
	use super::*;
	use cf_chains::{address::EncodedAddress, CcmChannelMetadata};
	use cf_primitives::{AccountId, Asset};
	use codec::Encode;
	use sp_runtime::BoundedVec;
	use state_chain_runtime::{EthereumInstance, Runtime, RuntimeEvent};

	fn test_event_encoding(registry: &TypeDecoder, event: RuntimeEvent, output: String) {
		let encoded = event.encode();
		assert_eq!(registry.decode_data(encoded).to_string(), output);
	}

	#[test]
	fn can_decode_runtime_events() {
		let registry = TypeDecoder::new::<RuntimeEvent>();

		test_event_encoding(
			&registry,
			RuntimeEvent::System(frame_system::Event::<Runtime>::Remarked {
				sender: AccountId::from([0xF0; 32]),
				hash: [0xFF; 32].into(),
			}),
			"System (Remarked { sender: ((240, 240, 240, 240, 240, 240, 240, 240, 240, 240, 240, 240, 240, 240, 240, 240, 240, 240, 240, 240, 240, 240, 240, 240, 240, 240, 240, 240, 240, 240, 240, 240)), hash: ((255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255)) })".to_owned()
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
			"EthereumBroadcaster (BroadcastSuccess { broadcast_id: 123, transaction_out_id: { s: (187, 187, 187, 187, 187, 187, 187, 187, 187, 187, 187, 187, 187, 187, 187, 187, 187, 187, 187, 187, 187, 187, 187, 187, 187, 187, 187, 187, 187, 187, 187, 187), k_times_g_address: (170, 170, 170, 170, 170, 170, 170, 170, 170, 170, 170, 170, 170, 170, 170, 170, 170, 170, 170, 170) }, transaction_ref: ((204, 204, 204, 204, 204, 204, 204, 204, 204, 204, 204, 204, 204, 204, 204, 204, 204, 204, 204, 204, 204, 204, 204, 204, 204, 204, 204, 204, 204, 204, 204, 204)) })".to_owned()
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
				channel_opening_fee: 1_000u128,
			}),
			"Swapping (SwapDepositAddressReady { deposit_address: Eth ((221, 221, 221, 221, 221, 221, 221, 221, 221, 221, 221, 221, 221, 221, 221, 221, 221, 221, 221, 221)), destination_address: Eth ((221, 221, 221, 221, 221, 221, 221, 221, 221, 221, 221, 221, 221, 221, 221, 221, 221, 221, 221, 221)), source_asset: Flip (), destination_asset: Usdc (), channel_id: 55, broker_commission_rate: 100, channel_metadata: Some ({ message: ((0, 1, 2, 3, 4)), gas_budget: 1000000, cf_parameters: ((16, 17, 18, 19, 20)) }), source_chain_expiry_block: 1000, boost_fee: 9, channel_opening_fee: 1000 })".to_owned(),
		);
	}

	#[test]
	fn can_return_unknown_events() {
		assert_eq!(
			TypeDecoder::new::<RuntimeEvent>()
				.decode_data(vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06])
				.to_string(),
			"{ UnknownData: \"010203040506\" }".to_owned()
		);
	}
}
