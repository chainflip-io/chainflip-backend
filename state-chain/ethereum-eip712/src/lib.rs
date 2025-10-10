#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode};
use scale_info::{
	prelude::{
		format,
		string::{String, ToString},
	},
	Field, MetaType, Path, Registry, TypeDef, TypeDefPrimitive, TypeInfo,
};
use scale_value::{Composite, Value, ValueDef};
use sp_std::{collections::btree_map::BTreeMap, vec, vec::Vec};

use crate::{
	eip712::{EIP712Domain, Eip712, Eip712DomainType, Eip712Error, TypedData},
	hash::keccak256,
};

pub mod bytes;
pub mod eip712;
//pub mod eip712_serializer;
pub mod hash;
pub mod lexer;
pub mod serde_helpers;

pub fn encode_eip712_using_type_info<T: TypeInfo + Encode + Decode + 'static>(
	value: T,
	domain: EIP712Domain,
) -> Result<[u8; 32], Eip712Error> {
	let mut registry = Registry::new();
	let id = registry.register_type(&MetaType::new::<T>());

	let portable_registry: scale_info::PortableRegistry = registry.into();
	let value =
		scale_value::scale::decode_as_type(&mut &value.encode()[..], id.id, &portable_registry)
			.map_err(|e| {
				Eip712Error::Message(
					format!("Failed to decode the scale-encoded value into the type provided by TypeInfo: {e}")
				)
			})?;

	let mut types: BTreeMap<String, Vec<Eip712DomainType>> = BTreeMap::new();
	let primary_type = recursively_construct_types(value.clone(), MetaType::new::<T>(), &mut types)
		.map_err(|e| Eip712Error::Message(format!("error while constructing types: {e}")))?;

	let typed_data = TypedData { domain, types, primary_type, message: value.remove_context() };

	typed_data.encode_eip712()
}

pub fn recursively_construct_types<C: Clone>(
	v: Value<C>,
	ty: MetaType,
	types: &mut BTreeMap<String, Vec<Eip712DomainType>>,
) -> Result<String, &'static str> {
	//handle errors
	let t = ty.type_info();

	let (mut type_name, fields): (String, Vec<Eip712DomainType>) =
		match (t.type_def, v.value.clone()) {
			(TypeDef::Composite(type_def_composite), ValueDef::Composite(comp_value)) =>
			// if the type is primitive_types::H160, we interpret it as an address. We also map
			// other primitives to solidity primitives directly without recursing further.
				match t.path {
					Path { segments: s } if s == vec!["primitive_types", "H160"] =>
						("address".to_string(), vec![]),
					Path { segments: s } if s == vec!["primitive_types", "U256"] =>
						("uint256".to_string(), vec![]),
					Path { segments: s } if s == vec!["primitive_types", "U128"] =>
						("uint128".to_string(), vec![]),
					Path { segments: s } if s == vec!["primitive_types", "H128"] =>
						("bytes".to_string(), vec![]),
					Path { segments: s } if s == vec!["primitive_types", "H256"] =>
						("bytes".to_string(), vec![]),
					Path { segments: s } if s == vec!["primitive_types", "H384"] =>
						("bytes".to_string(), vec![]),
					Path { segments: s } if s == vec!["primitive_types", "H512"] =>
						("bytes".to_string(), vec![]),
					path => (
						path.ident()
							// Should never error. Its a composite type so the name has to exist
							.ok_or("Type doesnt have a name")?
							.to_string(),
						process_composite(type_def_composite.fields, comp_value, types)?,
					),
				},
			(TypeDef::Variant(type_def_variant), ValueDef::Variant(value_variant)) => (
				t.path
					.ident()
					// Should never error. Its a composite type so the name has to exist
					.ok_or("Type doesnt have a name")?
					.to_string() + "__" +
					&value_variant.name.to_string(),
				// find the variant in type_def_variant that matches the
				type_def_variant
					.variants
					.into_iter()
					.find(|variant| value_variant.name == variant.name)
					.map(|variant| process_composite(variant.fields, value_variant.values, types))
					.ok_or(
						"variant name in value should match one of the variants in type def",
					)??,
			),

			(TypeDef::Sequence(type_def_sequence), ValueDef::Composite(Composite::Unnamed(fs))) => {
				let type_name = recursively_construct_types(
					fs[0].clone(),
					type_def_sequence.type_param,
					types,
				)?;
				(
					// convert the type name of the sequence to something like "TypeName[]". If its
					// aa sequence to type u8, then we interpret it as bytes
					if type_name == "u8" { "bytes".to_string() } else { type_name + "[]" },
					// ensures that we dont add this type to types list
					vec![],
				)
			},
			(TypeDef::Array(type_def_array), ValueDef::Composite(Composite::Unnamed(fs))) => {
				let type_name =
					recursively_construct_types(fs[0].clone(), type_def_array.type_param, types)?;
				(
					// convert the type name of the array to something like "TypeName[len]"
					if type_name == "u8" {
						"bytes".to_string()
					} else {
						type_name + "[" + &type_def_array.len.to_string() + "]"
					},
					// ensures that we dont add this type to types list
					vec![],
				)
			},
			(TypeDef::Tuple(type_def_tuple), ValueDef::Composite(Composite::Unnamed(fs))) => {
				let fields = type_def_tuple
					.fields
					.clone()
					.into_iter()
					.zip(fs.into_iter())
					.enumerate()
					.map(|(i, (ty, value))| -> Result<_, &'static str> {
						let type_name = recursively_construct_types(value.clone(), ty, types)?;
						Ok(Eip712DomainType {
							// In case of unnamed fields, we decide to name it by its type name
							// appended by its index in the tuple
							name: type_name.clone() + "__" + &i.to_string(),
							r#type: type_name,
						})
					})
					.collect::<Result<Vec<_>, _>>()?;
				(
					// In case of tuple, we decide to name it "UnnamedTuple_{first 4 bytes of hash
					// of the fields}" since tuples, although supported in solidity, cant be
					// easily displayed in metamask. Naming it so will display it in metamask
					// which will indicate to the signer that this is indeed a tuple. The 4
					// bytes of hash is just to avoid name collisions in case there are
					// multiple unnamed tuples.
					"UnnamedTuple__".to_string() +
						&hex::encode(&keccak256(&format!("{fields:?}"))[..4]),
					fields,
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
				vec![],
			),
			(TypeDef::Compact(type_def_compact), _) => (
				recursively_construct_types(v.clone(), type_def_compact.type_param, types)?,
				vec![],
			),
			// this is only used when scale-info's bitvec feature is enabled and since we dont use
			// that feature, this variant should be unreachable.
			(TypeDef::BitSequence(_), _) => return Err("Unreachable"),

			_ => return Err("Type and Value do not match"),
		};

	// If there are generic parameters to this type, append uniqueness to the type name to avoid
	// collisions
	if t.type_params.len() > 0 {
		type_name = type_name + "__" + &hex::encode(&keccak256(&format!("{fields:?}"))[..4]);
	}

	//TODO: maybe use the full path as the type name to avoid collisions due to same name types in
	// different paths

	// Only insert if there are fields (to avoid empty struct definitions)
	if !fields.is_empty() {
		types.insert(type_name.clone(), fields);
	}

	Ok(type_name)
}

fn process_composite<C: Clone>(
	fields: Vec<Field>,
	comp_value: Composite<C>,
	types: &mut BTreeMap<String, Vec<Eip712DomainType>>,
) -> Result<Vec<Eip712DomainType>, &'static str> {
	match comp_value {
		Composite::Named(fs) => {
			let fs_map = fs.into_iter().collect::<BTreeMap<_, _>>();
			fields
				.clone()
				.into_iter()
				.map(|field| -> Result<_, &'static str> {
					// shouldn't be possible since we are in Named variant
					let field_name = field.name.ok_or("field name doesn't exist")?.to_string();
					let value =
						fs_map.get(&field_name).ok_or("field with this name has to exist")?.clone();
					Ok(Eip712DomainType {
						name: field_name.clone(),
						// find out in which cases type name would be empty.
						r#type: recursively_construct_types(value.clone(), field.ty, types)?,
					})
				})
				.collect::<Result<_, _>>()
		},
		Composite::Unnamed(fs) => {
			fields
				.clone()
				.into_iter()
				.zip(fs)
				.enumerate()
				.map(|(i, (field, value))| -> Result<_, &'static str> {
					let type_name = recursively_construct_types(value.clone(), field.ty, types)?;
					Ok(Eip712DomainType {
						// In case of unnamed fields, we decide to name it by its type name appended
						// by its index in the tuple
						name: type_name.clone() + "_" + &i.to_string(),
						r#type: type_name,
					})
				})
				.collect::<Result<_, _>>()
		},
	}
}

impl<C: Clone> GetScaleValueFields for scale_value::Value<C> {
	fn get_scale_value_fields(&self) -> Result<Vec<Value<C>>, &'static str> {
		match &self.value {
			ValueDef::Composite(comp) => Ok(match comp.clone() {
				Composite::Named(fs) => fs.into_iter().map(|(_, v)| v).collect(),
				Composite::Unnamed(fs) => fs,
			}),
			ValueDef::Variant(v) => Ok(match v.values.clone() {
				Composite::Named(fs) => fs.into_iter().map(|(_, v)| v).collect(),
				Composite::Unnamed(fs) => fs,
			}),
			ValueDef::Primitive(_) => Err("Primitive type does not have fields"),
			ValueDef::BitSequence(_) => Err("BitSequence not supported"),
		}
	}
}

pub trait GetScaleValueFields: Sized {
	fn get_scale_value_fields(&self) -> Result<Vec<Self>, &'static str>;
}

#[cfg(test)]
pub mod tests {

	use super::*;
	use ethabi::ethereum_types::{H160, U256};
	use scale_info::prelude::string::String;
	//se serde::Deserialize;

	#[test]
	fn test_type_info_eip_712() {
		let domain = EIP712Domain {
			name: Some(String::from("Seaport")),
			version: Some(String::from("1.1")),
			chain_id: Some(U256([1, 0, 0, 0])),
			verifying_contract: Some(H160([0xef; 20])),
			salt: None,
		};

		#[derive(TypeInfo, Clone, Encode, Decode)]
		pub struct Mail {
			pub from: Person,
			pub to: Person,
			pub message: String,
		}

		#[derive(TypeInfo, Clone, Encode, Decode)]
		pub struct Person {
			pub name: String,
		}

		let payload = Mail {
			from: Person { name: String::from("Ramiz") },
			to: Person { name: String::from("Albert") },
			message: String::from("hello Albert"),
		};

		let mut registry = Registry::new();
		let id = registry.register_type(&MetaType::new::<Mail>());

		//let types = vec![];
		for (id, ty) in registry.types() {
			//types.push(ty)
			println!("ID: {:?},   Type: {:#?}", id, ty);
		}
		let mut b = BTreeMap::new();
		b.insert(1u8, 2u64);

		let portable_registry: scale_info::PortableRegistry = registry.into();
		let val = scale_value::scale::decode_as_type(
			&mut &payload.encode()[..],
			id.id,
			&portable_registry,
		)
		.unwrap();
		//println!("{:?}", encode_eip712_using_type_info(t, domain))
		println!("{:?}", val);
		// println!("{:?}", encode_eip712_using_type_info(payload,
		// domain).unwrap().encode_eip712());

		assert_eq!(
			hex::encode(&encode_eip712_using_type_info(payload, domain).unwrap()[..]),
			"36b58675f9b9390f1de60902297828280ab2cc1eaecb6453cfcb0328b2c35b33"
		);
	}

	#[test]
	fn playground() {
		use scale_value;
		let _domain = EIP712Domain {
			name: Some(String::from("Seaport")),
			version: Some(String::from("1.1")),
			chain_id: Some(U256([1, 0, 0, 0])),
			verifying_contract: Some(H160([0xef; 20])),
			salt: None,
		};

		#[derive(serde::Serialize, Encode, Decode, TypeInfo)]
		pub struct Mail {
			pub from: String,
			pub to: String,
			pub message: String,
		}

		#[derive(TypeInfo, serde::Serialize, Encode, Decode)]
		//#[scale_info(skip_type_params(S))]
		pub struct Test1<T, S> {
			#[codec(compact)]
			pub c: u128,
			pub a: T,
			pub b: S,
		}

		#[derive(TypeInfo, Encode, Decode)]
		pub enum TestEnum<T, S> {
			A(T),
			B(S),
			C(Vec<u128>),
			D { dd: u16 },
		}

		#[derive(TypeInfo, Encode, Decode)]
		pub struct Abs((u8, u128));

		// let t: u128 = 5;
		// let t = Test::A(Box::new(Test::C(5)));
		let t = Test1 { a: 5u64, b: 6u128, c: 6 };
		println!("{:?}", serde_json::to_value(&t).unwrap());
		println!("{:?}", serde_json::to_value(vec![2, 3, 4, 5]).unwrap());

		let mut registry = Registry::new();
		// registry.register_type(&MetaType::new::<Test1<u64, Test1<u8, u16>>>());
		//let id = registry.register_type(&MetaType::new::<TestEnum<u8, Mail>>());
		let id = registry.register_type(&MetaType::new::<U256>());

		//let types = vec![];
		for (id, ty) in registry.types() {
			//types.push(ty)
			println!("ID: {:?},   Type: {:#?}", id, ty);
		}

		let portable_registry: scale_info::PortableRegistry = registry.into();
		let val = scale_value::scale::decode_as_type(
			&mut &U256(Default::default()).encode()[..],
			id.id,
			&portable_registry,
		)
		.unwrap();
		//println!("{:?}", encode_eip712_using_type_info(t, domain))
		println!("{:?}", val);
		let jsonv = serde_json::to_value(val.clone()).unwrap();
		println!("{:?}", jsonv);
		let va: U256 = serde_json::from_value(jsonv).unwrap();
		println!("{:?}", va);
	}
}
