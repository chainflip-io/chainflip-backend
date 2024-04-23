#![cfg(test)]
use codec::{Decode, Encode};
use scale_decode::DecodeAsType;
use scale_info::{PortableRegistry, TypeInfo};

use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo)]
enum RuntimeEvent {
	System(SystemEvent),
	Other(OtherEvent),
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo)]
enum SystemEvent {
	ExtrinsicSuccess { dispatch_info: String },
	ExtrinsicFailed { dispatch_error: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo)]
enum OtherEvent {
	UnitVariant,
	ValueVariant(u32),
	TupleVariant(u32, String, bool),
	StructVariant { field: u32 },
	ArrayVariant(Vec<u32>),
	EthereumVariant(InstanceLike<Ethereum>),
	BitcoinVariant(InstanceLike<Bitcoin>),
	BytesArrayVariant([u8; 32]),
	ByteVecVariant(Vec<u8>),
}

trait Chain {
	type Asset;
}

mod eth {
	use super::*;
	#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo)]
	pub enum Asset {
		Eth,
		Flip,
		Usdc,
	}
}

mod btc {
	use super::*;
	#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo)]
	pub enum Asset {
		Btc,
	}
}

#[derive(Default, Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo)]
struct Ethereum;
impl Chain for Ethereum {
	type Asset = eth::Asset;
}

#[derive(Default, Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo)]
struct Bitcoin;
impl Chain for Bitcoin {
	type Asset = btc::Asset;
}

#[derive(Default, Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo)]
struct InstanceLike<T: Chain> {
	inner: T::Asset,
}

fn make_type_resolver<T: TypeInfo + 'static>() -> (u32, PortableRegistry) {
	let m = scale_info::MetaType::new::<T>();
	let mut registry = scale_info::Registry::new();
	let type_id = registry.register_type(&m).id;
	(type_id, PortableRegistry::from(registry))
}

macro_rules! make_test {
    ( [ $t:ty ], $( $v:expr ),+ $(,)? ) => {
        #[test]
        fn test_snapshots() {
            let (type_id, registry) = make_type_resolver::<$t>();

            $(
                let scale_encoded = $v.encode();
                let decoded_json = <ScaleDecodedToJson as DecodeAsType>::decode_as_type(
                    &mut &scale_encoded[..],
                    &type_id,
                    &registry,
                )
                .unwrap();
                insta::assert_json_snapshot!(decoded_json);
            )+
        }
    };
}

make_test! {
	[RuntimeEvent],
	RuntimeEvent::System(SystemEvent::ExtrinsicSuccess { dispatch_info: "info".to_owned() }),
	RuntimeEvent::System(SystemEvent::ExtrinsicFailed { dispatch_error: "error".to_owned() }),
	RuntimeEvent::Other(OtherEvent::UnitVariant),
	RuntimeEvent::Other(OtherEvent::ValueVariant(42)),
	RuntimeEvent::Other(OtherEvent::TupleVariant(42, "hello".to_owned(), true)),
	RuntimeEvent::Other(OtherEvent::StructVariant { field: 42 }),
	RuntimeEvent::Other(OtherEvent::ArrayVariant(vec![1, 2, 3])),
	RuntimeEvent::Other(OtherEvent::EthereumVariant(InstanceLike { inner: eth::Asset::Usdc })),
	RuntimeEvent::Other(OtherEvent::BitcoinVariant(InstanceLike { inner: btc::Asset::Btc })),
	RuntimeEvent::Other(OtherEvent::BytesArrayVariant([0xcf; 32])),
	RuntimeEvent::Other(OtherEvent::ByteVecVariant(vec![0xcf; 42])),
}

#[test]
fn event_check_for_success() {
	let (type_id, registry) = make_type_resolver::<RuntimeEvent>();

	let scale_encoded =
		RuntimeEvent::System(SystemEvent::ExtrinsicSuccess { dispatch_info: "info".to_owned() })
			.encode();
	let decoded_json = <ScaleDecodedToJson as DecodeAsType>::decode_as_type(
		&mut &scale_encoded[..],
		&type_id,
		&registry,
	)
	.unwrap();

	fn is_succes(obj: impl AsRef<serde_json::Value>) -> bool {
		!obj.as_ref()["System"]["ExtrinsicSuccess"].is_null()
	}

	assert!(is_succes(decoded_json));
}
