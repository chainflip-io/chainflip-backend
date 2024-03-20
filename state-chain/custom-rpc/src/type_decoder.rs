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
		scale_value::Value::unnamed_variant(
			"UnknownType",
			[scale_value::Value::string(hex::encode(bytes))],
		)
	}
}

#[cfg(test)]
pub mod test {
	use super::*;
	use cf_chains::{address::EncodedAddress, ForeignChain};
	use cf_primitives::{
		chains::assets::{any, btc, dot, eth},
		AccountId,
	};
	use codec::{Decode, Encode};
	use sp_core::H256;
	use sp_runtime::{DispatchError, ModuleError};

	#[derive(Debug, PartialEq, Encode, Decode, TypeInfo)]
	enum TestEvent {
		Inner(InnerEvent),
	}

	#[derive(Debug, PartialEq, Encode, Decode, TypeInfo)]
	enum InnerEvent {
		Unit,
		Primitives { num: u32, balance: u128, text: String, byte_array: [u8; 32] },
		SubstrateTypes { account_id: AccountId, hash: H256 },
		EncodedAddress { eth: EncodedAddress, dot: EncodedAddress, btc: EncodedAddress },
		ForeignChain { eth: ForeignChain, dot: ForeignChain, btc: ForeignChain, arb: ForeignChain },
		Asset { any: any::Asset, eth: eth::Asset, btc: btc::Asset, dot: dot::Asset },
		DispatchError { module: DispatchError, others: DispatchError },
	}

	#[derive(Debug, PartialEq, Encode, Decode, TypeInfo)]
	struct IncompatibleEvent(String);

	macro_rules! test_event_encoding {
		( $( $name:ident : $event:expr ),+ $(,)? ) => {
			$(
				#[test]
				fn $name() {
					let value = TypeDecoder::new::<TestEvent>().decode_data($event.encode());
					insta::assert_json_snapshot!(value);
				}
			)+
		};
	}

	test_event_encoding! {
		unknown_event: IncompatibleEvent("what?".to_owned()),
		unit_event: TestEvent::Inner(InnerEvent::Unit),
		primitives: TestEvent::Inner(InnerEvent::Primitives {
			num: 123u32,
			balance: 1_234_567_890_123u128,
			text: "Hello".to_owned(),
			byte_array: [0x02; 32],
		}),
		substrate_types: TestEvent::Inner(InnerEvent::SubstrateTypes {
			account_id: AccountId::from([0x01; 32]),
			hash: H256::from([0x02; 32]),
		}),
		external_address_types: TestEvent::Inner(InnerEvent::EncodedAddress {
			eth: EncodedAddress::Eth([0x03; 20]),
			dot: EncodedAddress::Dot([0x04; 32]),
			btc: EncodedAddress::Btc(b"bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq".to_vec()),
		}),
		chains: TestEvent::Inner(InnerEvent::ForeignChain {
			eth: ForeignChain::Ethereum,
			dot: ForeignChain::Polkadot,
			btc: ForeignChain::Bitcoin,
			arb: ForeignChain::Arbitrum,
		}),
		assets: TestEvent::Inner(InnerEvent::Asset {
			any: any::Asset::Flip,
			eth: eth::Asset::Eth,
			btc: btc::Asset::Btc,
			dot: dot::Asset::Dot,
		}),
		dispatch_error: TestEvent::Inner(InnerEvent::DispatchError {
			module: DispatchError::Module(ModuleError{
				index: 2u8,
				error: [0u8, 1u8, 2u8, 3u8],
				message: Some("CustomPalletError")
			}),
			others: DispatchError::Other("Error Message")
		}),
	}
}
