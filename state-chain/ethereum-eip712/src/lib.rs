#![cfg_attr(not(feature = "std"), no_std)]

use codec::Encode;
use ethabi::ethereum_types::{U128, U256};
use scale_info::{
	prelude::{
		format,
		string::{String, ToString},
	},
	Field, MetaType, Registry, TypeDef, TypeDefPrimitive, TypeInfo,
};
use scale_value::{Composite, Primitive, Value, ValueDef};
use serde::{Deserialize, Serialize};
use sp_std::{collections::btree_map::BTreeMap, vec::Vec};

use crate::{
	eip712::{EIP712Domain, Eip712DomainType, Eip712Error, TypedData},
	hash::keccak256,
};
use minimized_scale_value::MinimizedScaleValue;

pub mod build_eip712_data;
pub mod eip712;
#[cfg(test)]
pub mod extra_tests;
pub mod hash;
pub mod lexer;
pub mod minimized_scale_value;
pub mod serde_helpers;
pub mod typeinfo_decoder;

pub fn encode_eip712_using_type_info<T: TypeInfo + Encode + 'static>(
	value: T,
	domain: EIP712Domain,
) -> Result<TypedData, Eip712Error> {
	let mut registry = Registry::new();
	let id = registry.register_type(&MetaType::new::<T>());

	let portable_registry: scale_info::PortableRegistry = registry.into();
	let value =
		scale_value::scale::decode_as_type(&mut &value.encode()[..], id.id, &portable_registry)
			.map_err(|e| {
				Eip712Error::Message(
					format!("Failed to decode the scale-encoded value into the type provided by TypeInfo: {e}")
				)
			})?
			.remove_context();

	let mut types: BTreeMap<String, Vec<Eip712DomainType>> = BTreeMap::new();
	let (primary_type, minimized_value) =
		recursively_construct_types(value.clone(), MetaType::new::<T>(), &mut types)
			.map_err(|e| Eip712Error::Message(format!("error while constructing types: {e}")))?;

	let minimized_scale_value = MinimizedScaleValue::try_from(minimized_value).map_err(|e| {
		Eip712Error::Message(format!("Failed to convert scale value into MinimizedScaleValue: {e}"))
	})?;
	let typed_data = TypedData {
		domain,
		types,
		primary_type: primary_type.name,
		message: minimized_scale_value,
	};

	Ok(typed_data)
}

/// Optimized version of `encode_eip712_using_type_info` that bypasses registry construction.
///
/// This function uses a custom decoder that traverses `TypeInfo` directly instead of
/// building a `PortableRegistry`, which is significantly faster for large type trees.
pub fn encode_eip712_using_type_info_fast<T: TypeInfo + Encode + 'static>(
	value: T,
	domain: EIP712Domain,
) -> Result<TypedData, Eip712Error> {
	// Decode directly using TypeInfo - no registry needed
	let decoded_value = typeinfo_decoder::decode_with_type_info::<T>(&mut &value.encode()[..])
		.map_err(|e| Eip712Error::Message(format!("Failed to decode using TypeInfo: {:?}", e)))?;

	let mut types: BTreeMap<String, Vec<Eip712DomainType>> = BTreeMap::new();
	let (primary_type, minimized_value) =
		recursively_construct_types(decoded_value, MetaType::new::<T>(), &mut types)
			.map_err(|e| Eip712Error::Message(format!("error while constructing types: {e}")))?;

	let minimized_scale_value = MinimizedScaleValue::try_from(minimized_value).map_err(|e| {
		Eip712Error::Message(format!("Failed to convert scale value into MinimizedScaleValue: {e}"))
	})?;
	let typed_data = TypedData {
		domain,
		types,
		primary_type: primary_type.name,
		message: minimized_scale_value,
	};

	Ok(typed_data)
}

pub fn recursively_construct_types(
	v: Value,
	ty: MetaType,
	types: &mut BTreeMap<String, Vec<Eip712DomainType>>,
) -> Result<(TypeName, Value), &'static str> {
	//handle errors
	let t = ty.type_info();

	let (mut type_name, maybe_add_type, value): (TypeName, AddTypeOrNot, Value) =
		match (t.type_def, v.value.clone()) {
			(TypeDef::Composite(type_def_composite), ValueDef::Composite(comp_value)) =>
			// if the type is primitive_types::H160, we interpret it as an address. We also map
			// other primitives to solidity primitives directly without recursing further.
				match t.path.segments.as_slice() {
					["primitive_types", "H160"] => (
						TypeName { name: "address".to_string(), contains_type_id: false },
						AddTypeOrNot::DontAdd,
						scale_value_bytes_to_hex(extract_primitive_types(comp_value)?)?,
					),
					["primitive_types", "U256"] => (
						TypeName { name: "uint256".to_string(), contains_type_id: false },
						AddTypeOrNot::DontAdd,
						stringify_primitive_integers_types(extract_primitive_types(comp_value)?)?,
					),
					["primitive_types", "U128"] => (
						TypeName { name: "uint128".to_string(), contains_type_id: false },
						AddTypeOrNot::DontAdd,
						stringify_primitive_integers_types(extract_primitive_types(comp_value)?)?,
					),
					["primitive_types", "H128"] => (
						TypeName { name: "bytes".to_string(), contains_type_id: false },
						AddTypeOrNot::DontAdd,
						scale_value_bytes_to_hex(extract_primitive_types(comp_value)?)?,
					),
					["primitive_types", "H256"] => (
						TypeName { name: "bytes".to_string(), contains_type_id: false },
						AddTypeOrNot::DontAdd,
						scale_value_bytes_to_hex(extract_primitive_types(comp_value)?)?,
					),
					["primitive_types", "H384"] => (
						TypeName { name: "bytes".to_string(), contains_type_id: false },
						AddTypeOrNot::DontAdd,
						scale_value_bytes_to_hex(extract_primitive_types(comp_value)?)?,
					),
					["primitive_types", "H512"] => (
						TypeName { name: "bytes".to_string(), contains_type_id: false },
						AddTypeOrNot::DontAdd,
						scale_value_bytes_to_hex(extract_primitive_types(comp_value)?)?,
					),
					path => process_composite(
						type_def_composite.fields,
						comp_value,
						types,
						concatenate_name_segments(path.to_vec())?,
					)?,
				},
			(TypeDef::Variant(type_def_variant), ValueDef::Variant(value_variant)) =>
			// find the variant in type_def_variant that matches the variant the value is
			// initialized with
				type_def_variant
					.variants
					.into_iter()
					.find(|variant| value_variant.name == variant.name)
					.map(|variant| -> Result<_, &'static str> {
						if variant.fields.is_empty() {
							Ok((
								TypeName { name: "string".to_string(), contains_type_id: true },
								AddTypeOrNot::DontAdd,
								Value::string(value_variant.name.to_string()),
							))
						} else {
							// concatenate_name_segments should never error. Its a composite type so
							// the name has to exist
							let (mut type_name, add_type_or_not, value) = process_composite(
								variant.fields,
								value_variant.values,
								types,
								concatenate_name_segments(t.path.segments)? +
									"__" + &value_variant.name.to_string(),
							)?;
							// since variant struct itself is a instantiable type
							type_name.contains_type_id = true;
							Ok((type_name, add_type_or_not, value))
						}
					})
					.ok_or("variant name in value should match one of the variants in type def")??,

			(TypeDef::Sequence(type_def_sequence), ValueDef::Composite(Composite::Unnamed(fs))) => {
				let (type_names, values): (Vec<_>, Vec<_>) = fs
					.into_iter()
					.map(|f| recursively_construct_types(f, type_def_sequence.type_param, types))
					.collect::<Result<Vec<_>, _>>()?
					.into_iter()
					.unzip();

				let modified_value =
					Value::without_context(ValueDef::Composite(Composite::Unnamed(values)));

				// If the sequence is empty, there is no use, constructing the type of the array
				// elements, and so we map the empty array to an Empty type called EmptySequence
				if let Some(type_name) = type_names.first() {
					// convert the type name of the sequence to something like "TypeName[]".
					// If its a sequence of type u8, then we interpret it as bytes.
					// empty vec![] ensures that we dont add this type to types list
					if type_name.name == "uint8" {
						(
							TypeName { name: "bytes".to_string(), contains_type_id: false },
							AddTypeOrNot::DontAdd,
							scale_value_bytes_to_hex(modified_value)?,
						)
					} else {
						(
							TypeName {
								name: type_name.name.clone() + "[]",
								contains_type_id: type_names.iter().any(|tn| tn.contains_type_id),
							},
							AddTypeOrNot::DontAdd,
							modified_value,
						)
					}
				} else {
					recursively_construct_types(
						modified_value,
						MetaType::new::<EmptySequence>(),
						types,
					)
					.map(|(n, v)| (n, AddTypeOrNot::DontAdd, v))?
				}
			},
			(TypeDef::Array(type_def_array), ValueDef::Composite(Composite::Unnamed(fs))) => {
				let (type_names, values): (Vec<_>, Vec<_>) = fs
					.into_iter()
					.map(|f| recursively_construct_types(f, type_def_array.type_param, types))
					.collect::<Result<Vec<_>, _>>()?
					.into_iter()
					.unzip();
				let modified_value =
					Value::without_context(ValueDef::Composite(Composite::Unnamed(values)));

				// If the array is empty, there is no use, constructing the type of the array
				// elements, and so we map the empty array to an Empty type called EmptyArray
				if let Some(type_name) = type_names.first() {
					// convert the type name of the array to something like "TypeName[len]".
					// If its a sequence of type u8, then we interpret it as bytes.
					// vec![] ensures that we dont add this type to types list
					if type_name.name == "uint8" {
						(
							TypeName { name: "bytes".to_string(), contains_type_id: false },
							AddTypeOrNot::DontAdd,
							scale_value_bytes_to_hex(modified_value)?,
						)
					} else {
						(
							TypeName {
								name: type_name.name.clone() +
									"[" + &type_def_array.len.to_string() +
									"]",
								contains_type_id: type_names.iter().any(|tn| tn.contains_type_id),
							},
							AddTypeOrNot::DontAdd,
							modified_value,
						)
					}
				} else {
					recursively_construct_types(modified_value, type_def_array.type_param, types)
						.map(|(n, v)| (n, AddTypeOrNot::DontAdd, v))?
				}
			},
			(TypeDef::Tuple(type_def_tuple), ValueDef::Composite(Composite::Unnamed(fs))) => {
				let (type_fields_and_ids, values): (Vec<_>, Vec<_>) = type_def_tuple
					.fields
					.clone()
					.into_iter()
					.zip(fs.into_iter())
					.enumerate()
					.map(|(i, (ty, value))| -> Result<_, &'static str> {
						let (type_name, value) =
							recursively_construct_types(value.clone(), ty, types)?;
						let field_name = type_name.name.clone() + "__" + &i.to_string();
						Ok((
							(
								Eip712DomainType {
									// In case of unnamed type_fields, we decide to name it by its
									// type name appended by its index in the tuple
									name: field_name.clone(),
									r#type: type_name.name,
								},
								type_name.contains_type_id,
							),
							(field_name, value),
						))
					})
					.collect::<Result<Vec<_>, _>>()?
					.into_iter()
					.unzip();
				let (type_fields, ids): (Vec<_>, Vec<_>) = type_fields_and_ids.into_iter().unzip();
				(
					// In case of tuple, we decide to name it "UnnamedTuple_{first 4 bytes of hash
					// of the type_fields}". Naming it so will display it in metamask
					// which will indicate to the signer that this is indeed a tuple. The 4
					// bytes of hash is just to avoid name collisions in case there are
					// multiple unnamed tuples.
					TypeName {
						name: "UnnamedTuple__".to_string() +
							&hex::encode(&keccak256(format!("{type_fields:?}"))[..4]),
						contains_type_id: ids.into_iter().any(|id| id),
					},
					AddTypeOrNot::AddType { type_fields },
					Value::named_composite(values),
				)
			},
			(TypeDef::Primitive(type_def_primitive), ValueDef::Primitive(_p)) => (
				TypeName {
					name: match type_def_primitive {
						TypeDefPrimitive::Bool => "bool".to_string(),
						TypeDefPrimitive::Char => "string".to_string(),
						TypeDefPrimitive::Str => "string".to_string(),
						TypeDefPrimitive::U8 => "uint8".to_string(),
						TypeDefPrimitive::U16 => "uint16".to_string(),
						TypeDefPrimitive::U32 => "uint32".to_string(),
						TypeDefPrimitive::U64 => "uint64".to_string(),
						TypeDefPrimitive::U128 => "uint128".to_string(),
						TypeDefPrimitive::U256 => "uint256".to_string(),
						TypeDefPrimitive::I8 => "int8".to_string(),
						TypeDefPrimitive::I16 => "int16".to_string(),
						TypeDefPrimitive::I32 => "int32".to_string(),
						TypeDefPrimitive::I64 => "int64".to_string(),
						TypeDefPrimitive::I128 => "int128".to_string(),
						TypeDefPrimitive::I256 => "int256".to_string(),
					},
					contains_type_id: false,
				},
				AddTypeOrNot::DontAdd,
				v,
			),

			(TypeDef::Compact(type_def_compact), _) => {
				let (type_name, c_value) =
					recursively_construct_types(v.clone(), type_def_compact.type_param, types)?;
				(type_name, AddTypeOrNot::DontAdd, c_value)
			},
			// this is only used when scale-info's bitvec feature is enabled and since we dont use
			// that feature, this variant should be unreachable.
			(TypeDef::BitSequence(_), _) => return Err("Unreachable"),

			_ => return Err("Type and Value do not match"),
		};

	if let AddTypeOrNot::AddType { type_fields } = maybe_add_type {
		// If there are generic parameters to this type, append uniqueness to the type name to avoid
		// collisions
		if type_name.contains_type_id || t.type_params.len() > 0 {
			type_name.name =
				type_name.name + "__" + &hex::encode(&keccak256(format!("{type_fields:?}"))[..8]);
		}

		//TODO: maybe use the full path as the type name to avoid collisions due to same name types
		// in different paths

		types.insert(type_name.name.clone(), type_fields);
	}

	Ok((type_name, value))
}

fn process_composite(
	type_fields: Vec<Field>,
	comp_value: Composite<()>,
	types: &mut BTreeMap<String, Vec<Eip712DomainType>>,
	type_main_name: String,
) -> Result<(TypeName, AddTypeOrNot, Value), &'static str> {
	match comp_value {
		Composite::Named(fs) => {
			let fs_map = fs.into_iter().collect::<BTreeMap<_, _>>();
			type_fields
				.clone()
				.into_iter()
				.map(|field| -> Result<_, &'static str> {
					// shouldn't be possible since we are in Named variant
					let field_name = field.name.ok_or("field name doesn't exist")?.to_string();
					let value =
						fs_map.get(&field_name).ok_or("field with this name has to exist")?.clone();
					let (type_name, value) = recursively_construct_types(value, field.ty, types)?;
					Ok((
						(
							Eip712DomainType { name: field_name.clone(), r#type: type_name.name },
							type_name.contains_type_id,
						),
						(field_name, value),
					))
				})
				.collect::<Result<_, _>>()
		},
		Composite::Unnamed(fs) => {
			type_fields
				.clone()
				.into_iter()
				.zip(fs)
				.enumerate()
				.map(|(i, (field, value))| -> Result<_, &'static str> {
					let (type_name, value) =
						recursively_construct_types(value.clone(), field.ty, types)?;
					// In case of unnamed type_fields, we decide to name it by its type name
					// appended by its index in the tuple
					let field_name = type_name.name.clone() + "_" + &i.to_string();
					Ok((
						(
							Eip712DomainType { name: field_name.clone(), r#type: type_name.name },
							type_name.contains_type_id,
						),
						(field_name, value),
					))
				})
				.collect::<Result<_, _>>()
		},
	}
	.map(|v: Vec<((Eip712DomainType, bool), (String, Value))>| {
		let (type_and_ids, vals): (Vec<_>, Vec<_>) = v.into_iter().unzip();
		let (type_fields, maybe_extra_ids): (Vec<_>, Vec<_>) = type_and_ids.into_iter().unzip();
		(
			// if any of the types of the fields of this composite type carries with it a type id
			// in its name, we need to propagate that to the outer type which should also carry
			// a type_id
			TypeName {
				name: type_main_name,
				contains_type_id: maybe_extra_ids.into_iter().any(|id| id),
			},
			AddTypeOrNot::AddType { type_fields },
			Value::named_composite(vals),
		)
	})
}

enum AddTypeOrNot {
	DontAdd,
	AddType { type_fields: Vec<Eip712DomainType> },
}

pub fn scale_value_bytes_to_hex(v: Value) -> Result<Value, &'static str> {
	if let ValueDef::Composite(Composite::Unnamed(v)) = v.value {
		Ok(Value {
			value: ValueDef::Primitive(Primitive::String(
				"0x".to_string() +
					&hex::encode(
						v.into_iter()
							.map(|e| match e.value {
								ValueDef::Primitive(Primitive::U128(b)) =>
									Ok(b.try_into().map_err(|_| "u128 to u8 conversion failed")?),
								_ => Err("Expected u8 primitive"),
							})
							.collect::<Result<Vec<u8>, _>>()?,
					),
			)),
			context: (),
		})
	} else {
		Err("Expected unnamed composite for bytes extraction")
	}
}

// uints are commonly stringified due to how ethers-js encodes
fn stringify_primitive_integers_types(v: Value) -> Result<Value, &'static str> {
	match v.value {
		// this corresponds to the U256 in primitive_types crate.
		ValueDef::Composite(Composite::Unnamed(v)) => {
			let val_vec = v
				.into_iter()
				.map(|e| match e.value {
					ValueDef::Primitive(Primitive::U128(b)) =>
						Ok(b.try_into().map_err(|_| "u128 to u64 conversion failed")?),
					_ => Err("Expected u64 primitive"),
				})
				.collect::<Result<Vec<u64>, _>>()?;

			Ok(Value {
				value: ValueDef::Primitive(Primitive::String(
					if let Ok(arr) = <[u64; 4]>::try_from(val_vec.as_slice()) {
						U256(arr).to_string()
					} else if let Ok(arr) = <[u64; 2]>::try_from(val_vec) {
						U128(arr).to_string()
					} else {
						return Err("failed to convert scale value into U256 or U128")
					},
				)),
				context: (),
			})
		},
		_ => Err("unexpected value: cannot convert to stringified number"),
	}
}

fn extract_primitive_types(v: Composite<()>) -> Result<Value, &'static str> {
	if let Composite::Unnamed(fs) = v {
		if fs.len() != 1 {
			return Err("expected one element");
		}
		Ok(fs[0].clone())
	} else {
		Err("expected Unnamed")
	}
}

fn concatenate_name_segments(segments: Vec<&'static str>) -> Result<String, &'static str> {
	if segments.is_empty() {
		Err("Type doesn't have a name")
	} else {
		Ok(segments.join("____"))
	}
}

#[derive(TypeInfo, Clone, Encode)]
pub struct EmptySequence;
#[derive(TypeInfo, Clone, Encode)]
pub struct EmptyArray;

/// Benchmark helper module exposing individual steps of `encode_eip712_using_type_info`.
/// These functions are intended for performance analysis only.
#[cfg(feature = "runtime-benchmarks")]
#[expect(clippy::type_complexity)]
pub mod benchmark_helpers {
	use super::*;

	/// Step 1: Create registry and register type. Returns the portable registry and type id.
	pub fn step1_registry_and_type_registration<T: TypeInfo + 'static>(
	) -> (scale_info::PortableRegistry, u32) {
		let mut registry = Registry::new();
		let id = registry.register_type(&MetaType::new::<T>());
		let portable_registry: scale_info::PortableRegistry = registry.into();
		(portable_registry, id.id)
	}

	/// Step 2: Encode value and decode via type info.
	/// Returns the decoded scale_value::Value.
	pub fn step2_encode_decode<T: TypeInfo + Encode + 'static>(
		value: &T,
		portable_registry: &scale_info::PortableRegistry,
		type_id: u32,
	) -> Result<Value, Eip712Error> {
		scale_value::scale::decode_as_type(&mut &value.encode()[..], type_id, portable_registry)
			.map_err(|e| {
				Eip712Error::Message(format!(
					"Failed to decode the scale-encoded value into the type provided by TypeInfo: {e}"
				))
			})
			.map(|v| v.remove_context())
	}

	/// Step 3: Recursively construct EIP-712 types from the decoded value.
	/// Returns the primary type name, transformed value, and types map.
	pub fn step3_recursive_type_construction<T: TypeInfo + 'static>(
		value: Value,
	) -> Result<(String, Value, BTreeMap<String, Vec<Eip712DomainType>>), Eip712Error> {
		let mut types: BTreeMap<String, Vec<Eip712DomainType>> = BTreeMap::new();
		let (primary_type, minimized_value) =
			recursively_construct_types(value, MetaType::new::<T>(), &mut types).map_err(|e| {
				Eip712Error::Message(format!("error while constructing types: {e}"))
			})?;
		Ok((primary_type.name, minimized_value, types))
	}

	/// Step 4: Convert scale_value::Value to MinimizedScaleValue.
	pub fn step4_minimized_scale_value_conversion(
		value: Value,
	) -> Result<MinimizedScaleValue, Eip712Error> {
		MinimizedScaleValue::try_from(value).map_err(|e| {
			Eip712Error::Message(format!(
				"Failed to convert scale value into MinimizedScaleValue: {e}"
			))
		})
	}
}

#[cfg(test)]
pub mod tests {

	use super::*;
	use crate::eip712::Eip712;
	use ethabi::ethereum_types::{H160, U256};
	use scale_info::prelude::string::String;

	#[test]
	fn test_type_info_eip_712() {
		let domain = EIP712Domain {
			name: Some(String::from("Seaport")),
			version: Some(String::from("1.1")),
			chain_id: Some(U256([1, 0, 0, 0])),
			verifying_contract: Some(H160([0xef; 20])),
			salt: None,
		};

		#[derive(TypeInfo, Clone, Encode)]
		pub struct Mail {
			pub from: Person,
			pub to: Person,
			pub message: String,
		}

		#[derive(TypeInfo, Clone, Encode)]
		pub struct Person {
			pub name: String,
		}

		let payload = Mail {
			from: Person { name: String::from("Ramiz") },
			to: Person { name: String::from("Albert") },
			message: String::from("hello Albert"),
		};

		assert_eq!(
			hex::encode(
				&keccak256(
					encode_eip712_using_type_info(payload, domain)
						.unwrap()
						.encode_eip712()
						.unwrap()
				)[..]
			),
			"336ec92e268f26ea42492c5a5d80111b76fc59ea4897eb4128f10dc7da396452"
		);
	}

	/// Helper to test that both implementations produce identical EIP-712 hashes.
	fn assert_implementations_equivalent<T: TypeInfo + Encode + Clone + 'static>(value: T) {
		let domain = EIP712Domain {
			name: Some(String::from("Test")),
			version: Some(String::from("1")),
			chain_id: None,
			verifying_contract: None,
			salt: None,
		};

		let old_result = encode_eip712_using_type_info(value.clone(), domain.clone());
		let fast_result = encode_eip712_using_type_info_fast(value, domain);

		match (old_result, fast_result) {
			(Ok(old_typed_data), Ok(fast_typed_data)) => {
				assert_eq!(
					old_typed_data, fast_typed_data,
					"EIP-712 TypedData differ between implementations"
				);
			},
			(Err(e1), Err(e2)) => {
				assert_eq!(
					e1.to_string(),
					e2.to_string(),
					"Both implementations failed, but with different errors"
				);
			},
			(Ok(_), Err(e)) => panic!("Fast implementation failed but old succeeded: {:?}", e),
			(Err(e), Ok(_)) => panic!("Old implementation failed but fast succeeded: {:?}", e),
		}
	}

	#[test]
	fn test_equivalence_simple_struct() {
		#[derive(TypeInfo, Clone, Encode)]
		struct SimpleStruct {
			a: u32,
			b: String,
			c: u64,
		}

		assert_implementations_equivalent(SimpleStruct { a: 42, b: "hello".to_string(), c: 12345 });
	}

	#[test]
	fn test_equivalence_nested_struct() {
		#[derive(TypeInfo, Clone, Encode)]
		struct Inner {
			x: u16,
			y: u16,
		}

		#[derive(TypeInfo, Clone, Encode)]
		struct Outer {
			inner: Inner,
			name: String,
		}

		assert_implementations_equivalent(Outer {
			inner: Inner { x: 10, y: 20 },
			name: "test".to_string(),
		});
	}

	#[test]
	fn test_equivalence_enum_variants() {
		#[derive(TypeInfo, Clone, Encode)]
		enum TestEnum {
			Unit,
			Tuple(u32, u64),
			Struct { a: u8, b: u32 },
		}

		// Note: Enums must be wrapped in a struct to be used as root type in EIP-712
		#[derive(TypeInfo, Clone, Encode)]
		struct WrapEnum(TestEnum);

		assert_implementations_equivalent(WrapEnum(TestEnum::Unit));
		assert_implementations_equivalent(WrapEnum(TestEnum::Tuple(1, 2)));
		assert_implementations_equivalent(WrapEnum(TestEnum::Struct { a: 5, b: 100 }));
	}

	#[test]
	fn test_equivalence_vec() {
		#[derive(TypeInfo, Clone, Encode)]
		struct WithVec {
			items: Vec<u32>,
		}

		assert_implementations_equivalent(WithVec { items: vec![] });
		assert_implementations_equivalent(WithVec { items: vec![1, 2, 3, 4, 5] });
	}

	#[test]
	fn test_equivalence_array() {
		#[derive(TypeInfo, Clone, Encode)]
		struct WithArray {
			data: [u16; 4],
		}

		assert_implementations_equivalent(WithArray { data: [1, 2, 3, 4] });
	}

	#[test]
	fn test_equivalence_tuple() {
		#[derive(TypeInfo, Clone, Encode)]
		struct WithTuple {
			pair: (u32, String),
		}

		assert_implementations_equivalent(WithTuple { pair: (42, "tuple".to_string()) });
	}

	#[test]
	fn test_equivalence_compact() {
		#[derive(TypeInfo, Clone, Encode)]
		struct WithCompact {
			#[codec(compact)]
			value: u128,
		}

		assert_implementations_equivalent(WithCompact { value: 0 });
		assert_implementations_equivalent(WithCompact { value: 63 }); // single byte
		assert_implementations_equivalent(WithCompact { value: 16383 }); // two bytes
		assert_implementations_equivalent(WithCompact { value: 1073741823 }); // four bytes
		assert_implementations_equivalent(WithCompact { value: u128::MAX }); // big integer mode
	}

	#[test]
	fn test_equivalence_bool_and_primitives() {
		// Note: EIP-712 encoder only supports unsigned integers, not signed integers
		#[derive(TypeInfo, Clone, Encode)]
		struct Primitives {
			flag: bool,
			small: u8,
			medium: u32,
			large: u128,
		}

		assert_implementations_equivalent(Primitives {
			flag: true,
			small: 255,
			medium: 1000000,
			large: u128::MAX,
		});
	}

	#[test]
	fn test_equivalence_option() {
		#[derive(TypeInfo, Clone, Encode)]
		struct WithOption {
			maybe: Option<u32>,
		}

		assert_implementations_equivalent(WithOption { maybe: None });
		assert_implementations_equivalent(WithOption { maybe: Some(42) });
	}

	#[test]
	fn test_equivalence_nested_option() {
		#[derive(TypeInfo, Clone, Encode)]
		struct Inner {
			value: u32,
		}

		#[derive(TypeInfo, Clone, Encode)]
		struct WithNestedOption {
			maybe_inner: Option<Inner>,
		}

		assert_implementations_equivalent(WithNestedOption { maybe_inner: None });
		assert_implementations_equivalent(WithNestedOption {
			maybe_inner: Some(Inner { value: 99 }),
		});
	}

	#[test]
	fn test_equivalence_empty_struct() {
		#[derive(TypeInfo, Clone, Encode)]
		struct Empty;

		#[derive(TypeInfo, Clone, Encode)]
		struct WrapEmpty(Empty);

		assert_implementations_equivalent(WrapEmpty(Empty));
	}

	#[test]
	fn test_equivalence_complex_nested() {
		#[derive(TypeInfo, Clone, Encode)]
		struct Person {
			name: String,
			age: u8,
		}

		#[derive(TypeInfo, Clone, Encode)]
		enum Status {
			Active,
			Inactive { reason: String },
		}

		#[derive(TypeInfo, Clone, Encode)]
		struct ComplexStruct {
			id: u64,
			people: Vec<Person>,
			status: Status,
			metadata: Option<(u32, String)>,
		}

		assert_implementations_equivalent(ComplexStruct {
			id: 123,
			people: vec![
				Person { name: "Alice".to_string(), age: 30 },
				Person { name: "Bob".to_string(), age: 25 },
			],
			status: Status::Active,
			metadata: Some((1, "test".to_string())),
		});

		assert_implementations_equivalent(ComplexStruct {
			id: 456,
			people: vec![],
			status: Status::Inactive { reason: "maintenance".to_string() },
			metadata: None,
		});
	}
}
