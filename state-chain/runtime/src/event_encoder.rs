use codec::{Decode, Encode};
use scale_info::{prelude::string::String, PortableRegistry};
// use scale_value::{Value, ValueDef};
use serde::{Deserialize, Serialize};

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

	pub fn decode_event_to_string(&self, data: Vec<u8>) -> Result<String, EventDecoderError> {
		scale_value::scale::decode_as_type(&mut &*data, &self.type_id, &self.registry)
			.map_err(|_| EventDecoderError::FailedToDecodeFromScaleBytes)
			.map(|value| value.to_string())
	}

	pub fn decode_event_to_string_json(&self, data: Vec<u8>) -> Result<String, EventDecoderError> {
		let value = scale_value::scale::decode_as_type(&mut &*data, &self.type_id, &self.registry)
			.map_err(|_| EventDecoderError::FailedToDecodeFromScaleBytes)?;
		value
			.serialize(scale_value::serde::ValueSerializer)
			.map_err(|_| EventDecoderError::FailedToConvertIntoJson)
			.map(|value| value.to_string())
	}

	pub fn decode_event_from_string(
		&self,
		data: String,
	) -> Result<scale_value::Value<()>, EventDecoderError> {
		scale_value::stringify::from_str(&data)
			.0
			.map_err(|_| EventDecoderError::FailedToDecodeFromString)
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

// <------- Custom to-string implementation for Value --------->
// pub fn encode_value_to_json(value: Value) -> String {
// 	format!("{{\n{}\n}}", value.value)
// }

// pub fn encode_value_def(value: ValueDef<T>) -> String {
// 	match value {
// 		ValueDef::Composite(c) => c.fmt(f),
// 		ValueDef::Variant(v) => v.fmt(f),
// 		ValueDef::BitSequence(b) => fmt_bitsequence(b, f),
// 		ValueDef::Primitive(p) => p.fmt(f),
// 	}
// }

// < ------------ Implementation of Scale-value libray ---------->
// impl<T> Display for Composite<T> {
//     fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
//         match self {
//             Composite::Named(vals) => {
//                 f.write_str("{ ")?;
//                 for (idx, (name, val)) in vals.iter().enumerate() {
//                     if idx != 0 {
//                         f.write_str(", ")?;
//                     }
//                     if is_ident(name) {
//                         f.write_str(name)?;
//                     } else {
//                         fmt_string(name, f)?;
//                     }
//                     f.write_str(": ")?;
//                     val.fmt(f)?;
//                 }
//                 f.write_str(" }")?;
//             }
//             Composite::Unnamed(vals) => {
//                 f.write_char('(')?;
//                 for (idx, val) in vals.iter().enumerate() {
//                     if idx != 0 {
//                         f.write_str(", ")?;
//                     }
//                     val.fmt(f)?;
//                 }
//                 f.write_char(')')?;
//             }
//         }
//         Ok(())
//     }
// }

// impl<T> Display for Variant<T> {
//     fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
//         if is_ident(&self.name) {
//             f.write_str(&self.name)?;
//         } else {
//             // If the variant name isn't a valid ident, we parse it into
//             // a special "v" prefixed string to allow arbitrary content while
//             // keeping it easy to parse variant names with minimal lookahead.
//             // Most use cases should never see or care about this.
//             f.write_char('v')?;
//             fmt_string(&self.name, f)?;
//         }
//         f.write_char(' ')?;
//         self.values.fmt(f)
//     }
// }

// impl Display for Primitive {
//     fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
//         match self {
//             Primitive::Bool(true) => f.write_str("true"),
//             Primitive::Bool(false) => f.write_str("false"),
//             Primitive::Char(c) => fmt_char(*c, f),
//             Primitive::I128(n) => n.fmt(f),
//             Primitive::U128(n) => n.fmt(f),
//             Primitive::String(s) => fmt_string(s, f),
//             // We don't currently have a sane way to parse into these or
//             // format out of them:
//             Primitive::U256(_) | Primitive::I256(_) => Err(core::fmt::Error),
//         }
//     }
// }

// fn fmt_string(s: &str, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
//     f.write_char('"')?;
//     for char in s.chars() {
//         match string_helpers::to_escape_code(char) {
//             Some(escaped) => {
//                 f.write_char('\\')?;
//                 f.write_char(escaped)?
//             }
//             None => f.write_char(char)?,
//         }
//     }
//     f.write_char('"')
// }

// fn fmt_char(c: char, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
//     f.write_char('\'')?;
//     match string_helpers::to_escape_code(c) {
//         Some(escaped) => {
//             f.write_char('\\')?;
//             f.write_char(escaped)?
//         }
//         None => f.write_char(c)?,
//     }
//     f.write_char('\'')
// }

// fn fmt_bitsequence(b: &BitSequence, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
//     f.write_char('<')?;
//     for bit in b.iter() {
//         match bit {
//             true => f.write_char('1')?,
//             false => f.write_char('0')?,
//         }
//     }
//     f.write_char('>')
// }

// /// Is the string provided a valid ident (as per from_string::parse_ident).
// fn is_ident(s: &str) -> bool {
//     let mut chars = s.chars();

//     // First char must be a letter (false if no chars)
//     let Some(fst) = chars.next() else { return false };
//     if !fst.is_alphabetic() {
//         return false;
//     }

//     // Other chars must be letter, number or underscore
//     for c in chars {
//         if !c.is_alphanumeric() && c != '_' {
//             return false;
//         }
//     }
//     true
// }
