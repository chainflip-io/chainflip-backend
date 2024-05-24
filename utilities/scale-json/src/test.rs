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
	Unit,
	Value(u32),
	Tuple(u32, String, bool),
	Struct {
		num_u8: u8,
		num_u16: u16,
		num_u32: u32,
		num_u64: u64,
		num_u128: u128,
		num_i8: i8,
		num_i16: i16,
		num_i32: i32,
		num_i64: i64,
		num_i128: i128,
	},
	Array(Vec<u32>),
	Ethereum(InstanceLike<Ethereum>),
	Bitcoin(InstanceLike<Bitcoin>),
	BytesArray([u8; 32]),
	ByteVec(Vec<u8>),
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

macro_rules! insta_assert_json_pretty {
	($($arg:tt)*) => {
		insta::_assert_snapshot_base!(
			transform=|v| serde_json::to_string_pretty(v).unwrap(),
			$($arg)*
		)
	};
}

macro_rules! make_tests {
	( [ $t:ty ], $( $name:ident: $v:expr ),+ $(,)? ) => {
		$(
			#[test]
			fn $name() {
				let (type_id, registry) = make_type_resolver::<$t>();

				let scale_encoded = $v.encode();
				let decoded_json = <ScaleDecodedToJson as DecodeAsType>::decode_as_type(
					&mut &scale_encoded[..],
					type_id,
					&registry,
				)
				.unwrap();

				// NOTE: Cannot use insta::assert_json_snapshot! here because the implementation relies on serde-json
				// serialization and insta uses its own internal serializer.
				insta_assert_json_pretty!(decoded_json.as_ref());
			}
		)+
	};
}

mod insta_tests {
	use super::*;

	make_tests! {
		[RuntimeEvent],
		extrinsic_success: RuntimeEvent::System(SystemEvent::ExtrinsicSuccess { dispatch_info: "info".to_owned() }),
		extrinsic_failure: RuntimeEvent::System(SystemEvent::ExtrinsicFailed { dispatch_error: "error".to_owned() }),
		unit: RuntimeEvent::Other(OtherEvent::Unit),
		value: RuntimeEvent::Other(OtherEvent::Value(42)),
		tuple: RuntimeEvent::Other(OtherEvent::Tuple(42, "hello".to_owned(), true)),
		struct_variant: RuntimeEvent::Other(OtherEvent::Struct {
			num_u8: 1,
			num_u16: 2,
			num_u32: 3,
			num_u64: 4,
			num_u128: 5,
			num_i8: -1,
			num_i16: -2,
			num_i32: -3,
			num_i64: -4,
			num_i128: -5,
		}),
		array: RuntimeEvent::Other(OtherEvent::Array(vec![1, 2, 3])),
		array_empty: RuntimeEvent::Other(OtherEvent::Array(vec![])),
		eth_instance: RuntimeEvent::Other(OtherEvent::Ethereum(InstanceLike { inner: eth::Asset::Usdc })),
		btc_instance: RuntimeEvent::Other(OtherEvent::Bitcoin(InstanceLike { inner: btc::Asset::Btc })),
		byte_array: RuntimeEvent::Other(OtherEvent::BytesArray([0xcf; 32])),
		byte_vec: RuntimeEvent::Other(OtherEvent::ByteVec(vec![0xcf; 42])),
		byte_vec_empty: RuntimeEvent::Other(OtherEvent::ByteVec(vec![])),
	}
}

#[test]
fn event_check_for_success() {
	let (type_id, registry) = make_type_resolver::<RuntimeEvent>();

	let scale_encoded =
		RuntimeEvent::System(SystemEvent::ExtrinsicSuccess { dispatch_info: "info".to_owned() })
			.encode();
	let decoded_json = <ScaleDecodedToJson as DecodeAsType>::decode_as_type(
		&mut &scale_encoded[..],
		type_id,
		&registry,
	)
	.unwrap();

	fn is_succes(obj: impl AsRef<serde_json::Value>) -> bool {
		!obj.as_ref()["System"]["ExtrinsicSuccess"].is_null()
	}

	assert!(is_succes(decoded_json));
}
