use codec::{Decode, Encode};
use scale_info::{prelude::string::String, PortableRegistry};
use serde::{Deserialize, Serialize};
use sp_std::vec::Vec;

use crate::RuntimeEvent;

#[derive(Encode, Decode, Clone, Copy, Eq, PartialEq, Serialize, Deserialize, Debug)]
pub enum EventDecoderError {
	FailedToDecodeFromScaleBytes,
	FailedToDecodeFromString,
	FailedToConvertIntoJson,
}

/// A struct that provides interface for decoding RuntimeEvents.
pub struct RuntimeEventDecoder {
	registry: PortableRegistry,
	type_id: u32,
}

impl RuntimeEventDecoder {
	/// Creates and returns an instance of a PortableRegistry, used for decoding Runtime Events.
	pub fn new() -> Self {
		// We can get the 'portable' type info using scale_info.
		let meta = scale_info::MetaType::new::<RuntimeEvent>();
		let mut registry = scale_info::Registry::new();
		let id = registry.register_type(&meta).id;

		Self { registry: PortableRegistry::from(registry), type_id: id }
	}

	pub fn decode_event_to_json(&self, data: Vec<u8>) -> Result<String, EventDecoderError> {
		scale_value::scale::decode_as_type(&mut &*data, &self.type_id, &self.registry)
			.map_err(|_| EventDecoderError::FailedToDecodeFromScaleBytes)
			.and_then(|value| {
				serde_json::to_string(&value)
					.map_err(|_| EventDecoderError::FailedToConvertIntoJson)
			})
	}
}

impl Default for RuntimeEventDecoder {
	fn default() -> Self {
		Self::new()
	}
}

#[cfg(test)]
pub mod test {
	use super::*;
	use cf_chains::{address::EncodedAddress, CcmChannelMetadata};
	use cf_primitives::{AccountId, Asset};
	use sp_runtime::BoundedVec;

	use crate::{EthereumInstance, Runtime};

	fn test_event_encoding(registry: &RuntimeEventDecoder, event: RuntimeEvent) {
		let encoded = event.encode();
		let to_string_json = registry.decode_event_to_json(encoded).unwrap();
		println!("value to_string Json: \n{}\n", to_string_json);
	}

	#[test]
	fn can_decode_runtime_events() {
		let registry = RuntimeEventDecoder::new();

		test_event_encoding(
			&registry,
			RuntimeEvent::System(frame_system::Event::<Runtime>::Remarked {
				sender: AccountId::from([0xF0; 32]),
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
	}
}
