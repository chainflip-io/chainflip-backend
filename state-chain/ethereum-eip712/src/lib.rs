#![cfg_attr(not(feature = "std"), no_std)]

use codec::Encode;
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

pub mod bytes;
pub mod eip712;
pub mod hash;
pub mod lexer;
pub mod minimized_scale_value;
pub mod serde_helpers;

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
	let typed_data = TypedData { domain, types, primary_type, message: minimized_scale_value };

	Ok(typed_data)
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
		return Err("Type doesn't have a name")
	}
	let mut out = segments[0].to_string();
	for segment in &segments[1..] {
		out = out + "::" + segment;
	}
	Ok(out)
}

pub fn recursively_construct_types(
	v: Value,
	ty: MetaType,
	types: &mut BTreeMap<String, Vec<Eip712DomainType>>,
) -> Result<(String, Value), &'static str> {
	//handle errors
	let t = ty.type_info();

	let (mut type_name, (maybe_add_type, value)): (String, (AddTypeOrNot, Value)) =
		match (t.type_def, v.value.clone()) {
			(TypeDef::Composite(type_def_composite), ValueDef::Composite(comp_value)) =>
			// if the type is primitive_types::H160, we interpret it as an address. We also map
			// other primitives to solidity primitives directly without recursing further.
				match t.path.segments.as_slice() {
					["primitive_types", "H160"] => (
						"address".to_string(),
						(
							AddTypeOrNot::DontAdd,
							scale_value_bytes_to_hex(extract_primitive_types(comp_value)?)?,
						),
					),
					["primitive_types", "U256"] =>
						("uint256".to_string(), (AddTypeOrNot::DontAdd, v)),
					["primitive_types", "U128"] =>
						("uint128".to_string(), (AddTypeOrNot::DontAdd, v)),
					["primitive_types", "H128"] => (
						"bytes".to_string(),
						(
							AddTypeOrNot::DontAdd,
							scale_value_bytes_to_hex(extract_primitive_types(comp_value)?)?,
						),
					),
					["primitive_types", "H256"] => (
						"bytes".to_string(),
						(
							AddTypeOrNot::DontAdd,
							scale_value_bytes_to_hex(extract_primitive_types(comp_value)?)?,
						),
					),
					["primitive_types", "H384"] => (
						"bytes".to_string(),
						(
							AddTypeOrNot::DontAdd,
							scale_value_bytes_to_hex(extract_primitive_types(comp_value)?)?,
						),
					),
					["primitive_types", "H512"] => (
						"bytes".to_string(),
						(
							AddTypeOrNot::DontAdd,
							scale_value_bytes_to_hex(extract_primitive_types(comp_value)?)?,
						),
					),
					path => (
						concatenate_name_segments(path.to_vec())?,
						process_composite(type_def_composite.fields, comp_value, types)?,
					),
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
								"string".to_string(),
								(
									AddTypeOrNot::DontAdd,
									Value::string(value_variant.name.to_string()),
								),
							))
						} else {
							Ok((
								// Should never error. Its a composite type so the name has to
								// exist
								concatenate_name_segments(t.path.segments)? +
									"__" + &value_variant.name.to_string(),
								process_composite(variant.fields, value_variant.values, types)?,
							))
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
					if type_name == "uint8" {
						(
							"bytes".to_string(),
							(AddTypeOrNot::DontAdd, scale_value_bytes_to_hex(modified_value)?),
						)
					} else {
						(type_name.clone() + "[]", (AddTypeOrNot::DontAdd, modified_value))
					}
				} else {
					recursively_construct_types(
						modified_value,
						MetaType::new::<EmptySequence>(),
						types,
					)
					.map(|(n, v)| (n, (AddTypeOrNot::DontAdd, v)))?
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
					if type_name == "uint8" {
						(
							"bytes".to_string(),
							(AddTypeOrNot::DontAdd, scale_value_bytes_to_hex(modified_value)?),
						)
					} else {
						(
							type_name.clone() + "[" + &type_def_array.len.to_string() + "]",
							(AddTypeOrNot::DontAdd, modified_value),
						)
					}
				} else {
					recursively_construct_types(
						modified_value,
						MetaType::new::<EmptyArray>(),
						types,
					)
					.map(|(n, v)| (n, (AddTypeOrNot::DontAdd, v)))?
				}
			},
			(TypeDef::Tuple(type_def_tuple), ValueDef::Composite(Composite::Unnamed(fs))) => {
				let (type_fields, values): (_, Vec<(String, Value)>) = type_def_tuple
					.fields
					.clone()
					.into_iter()
					.zip(fs.into_iter())
					.enumerate()
					.map(|(i, (ty, value))| -> Result<_, &'static str> {
						let (type_name, value) =
							recursively_construct_types(value.clone(), ty, types)?;
						let field_name = type_name.clone() + "__" + &i.to_string();
						Ok((
							Eip712DomainType {
								// In case of unnamed type_fields, we decide to name it by its type
								// name appended by its index in the tuple
								name: field_name.clone(),
								r#type: type_name,
							},
							(field_name, value),
						))
					})
					.collect::<Result<Vec<_>, _>>()?
					.into_iter()
					.unzip();
				(
					// In case of tuple, we decide to name it "UnnamedTuple_{first 4 bytes of hash
					// of the type_fields}". Naming it so will display it in metamask
					// which will indicate to the signer that this is indeed a tuple. The 4
					// bytes of hash is just to avoid name collisions in case there are
					// multiple unnamed tuples.
					"UnnamedTuple__".to_string() +
						&hex::encode(&keccak256(format!("{type_fields:?}"))[..4]),
					(AddTypeOrNot::AddType { type_fields }, Value::named_composite(values)),
				)
			},
			(TypeDef::Primitive(type_def_primitive), ValueDef::Primitive(_p)) => (
				match type_def_primitive {
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
				(AddTypeOrNot::DontAdd, v),
			),
			(TypeDef::Compact(type_def_compact), _) => {
				let (type_name, c_value) =
					recursively_construct_types(v.clone(), type_def_compact.type_param, types)?;
				(type_name, (AddTypeOrNot::DontAdd, c_value))
			},
			// this is only used when scale-info's bitvec feature is enabled and since we dont use
			// that feature, this variant should be unreachable.
			(TypeDef::BitSequence(_), _) => return Err("Unreachable"),

			_ => return Err("Type and Value do not match"),
		};

	if let AddTypeOrNot::AddType { type_fields } = maybe_add_type {
		// If there are generic parameters to this type, append uniqueness to the type name to avoid
		// collisions
		if t.type_params.len() > 0 {
			type_name =
				type_name + "__" + &hex::encode(&keccak256(format!("{type_fields:?}"))[..4]);
		}

		//TODO: maybe use the full path as the type name to avoid collisions due to same name types
		// in different paths

		types.insert(type_name.clone(), type_fields);
	}

	Ok((type_name, value))
}

fn process_composite(
	type_fields: Vec<Field>,
	comp_value: Composite<()>,
	types: &mut BTreeMap<String, Vec<Eip712DomainType>>,
) -> Result<(AddTypeOrNot, Value), &'static str> {
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
						Eip712DomainType { name: field_name.clone(), r#type: type_name },
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
					let field_name = type_name.clone() + "_" + &i.to_string();
					Ok((
						Eip712DomainType { name: field_name.clone(), r#type: type_name },
						(field_name, value),
					))
				})
				.collect::<Result<_, _>>()
		},
	}
	.map(|v: Vec<(Eip712DomainType, (String, Value))>| {
		let (type_fields, vals): (_, Vec<(String, Value)>) = v.into_iter().unzip();
		(AddTypeOrNot::AddType { type_fields }, Value::named_composite(vals))
	})
}

#[derive(TypeInfo, Clone, Encode)]
pub struct EmptySequence;
#[derive(TypeInfo, Clone, Encode)]
pub struct EmptyArray;

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
				&encode_eip712_using_type_info(payload, domain).unwrap().encode_eip712().unwrap()[..]
			),
			"36b58675f9b9390f1de60902297828280ab2cc1eaecb6453cfcb0328b2c35b33"
		);
	}
}
