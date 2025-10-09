#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode};
use scale_info::{
	prelude::{
		format,
		string::{String, ToString},
	},
	Field, MetaType, Registry, TypeDef, TypeDefPrimitive, TypeInfo,
};
use scale_value::{Composite, Value, ValueDef};
use sp_std::{collections::btree_map::BTreeMap, vec, vec::Vec};

use crate::eip712::{EIP712Domain, Eip712, Eip712DomainType, Eip712Error, TypedData};

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

	let typed_data = TypedData {
		domain,
		types: registry
			.types()
			.filter_map(|(_, ty)| {
				Some((
					ty.path.segments.last()?.to_string().clone(),
					match &ty.type_def {
						TypeDef::Composite(comp_type) => {
							comp_type
								.fields
								.clone()
								.into_iter()
								.filter_map(|field| {
									Some(Eip712DomainType {
										//find out if there are unnamed fields in evm structs
										// e.g. struct A(pub u8)
										name: field.name?.to_string(),
										// find out in which cases type name would be empty.
										r#type: match field.type_name.unwrap() {
											s if s == "String" => "string".to_string(),
											s => s.to_string(),
										},
									})
								})
								.collect()
						},
						//todo
						_ => Default::default(),
					},
				))
			})
			.collect(),
		primary_type: T::type_info().path.segments.last().unwrap().to_string(),
		message: Default::default(),
	};

	let portable_registry: scale_info::PortableRegistry = registry.into();
	let value =
		scale_value::scale::decode_as_type(&mut &value.encode()[..], id.id, &portable_registry)
			.map_err(|e| {
				Eip712Error::Message(
					format!("Failed to decode the scale-encoded value into the type provided by TypeInfo: {e}")
				)
			})?;

	let mut types: BTreeMap<String, Vec<Eip712DomainType>> = BTreeMap::new();
	let mut values: BTreeMap<String, Vec<scale_value::Value>> = BTreeMap::new();
	let _ = recursively_construct_types(value, MetaType::new::<T>(), &mut types, &mut values)
		.map_err(|e| Eip712Error::Message(format!("error while constructing types: {e}")))?;

	typed_data.encode_eip712()
}

pub fn recursively_construct_types<C: Clone>(
	v: Value<C>,
	ty: MetaType,
	types: &mut BTreeMap<String, Vec<Eip712DomainType>>,
	values: &mut BTreeMap<String, Vec<scale_value::Value>>,
) -> Result<String, &'static str> {
	//handle errors
	let t = ty.type_info();

	let (type_name, fields): (String, Vec<Eip712DomainType>) = match (t.type_def, v.value.clone()) {
		(TypeDef::Composite(type_def_composite), ValueDef::Composite(comp_value)) => (
			t.path
				.ident()
				// Should never error. Its a composite type so the name has to exist
				.ok_or("Type doesnt have a name")?
				.to_string(),
			process_composite(type_def_composite.fields, comp_value, types, values)?,
		),

		(TypeDef::Variant(type_def_variant), ValueDef::Variant(value_variant)) => (
			t.path
				.ident()
				// Should never error. Its a composite type so the name has to exist
				.ok_or("Type doesnt have a name")?
				.to_string() + "_" + &value_variant.name.to_string(),
			// find the variant in type_def_variant that matches the
			type_def_variant
				.variants
				.into_iter()
				.find(|variant| value_variant.name == variant.name)
				.map(|variant| process_composite(variant.fields, value_variant.values, types, values))
				.ok_or("variant name in value should match one of the variants in type def")??,
		),

		(TypeDef::Sequence(type_def_sequence), ValueDef::Composite(Composite::Unnamed(fs))) =>
			(
				// convert the type name of the sequence to something like "TypeName[]"
				recursively_construct_types(
					fs[0].clone(),
					type_def_sequence.type_param,
					types,, values
				)? + "[]",
				// ensures that we dont add this type to types list
				vec![],
			),
		(TypeDef::Array(type_def_array), ValueDef::Composite(Composite::Unnamed(fs))) => (
			// convert the type name of the array to something like "TypeName[len]"
			recursively_construct_types(fs[0].clone(), type_def_array.type_param, types, values)? +
				"[" + &type_def_array.len.to_string() +
				"]",
			// ensures that we dont add this type to types list
			vec![],
		),
		(TypeDef::Tuple(type_def_tuple), ValueDef::Composite(Composite::Unnamed(fs))) => {
			(
				// In case of tuple, we decide to name it "UnnamedTuple" since tuples, although
				// supported in solidity, cant be easily displayed in metamask. Naming it so will
				// display it in metamask which will indicate to the signer that this is indeed a
				// tuple
				"UnnamedTuple".to_string(),
				type_def_tuple
					.fields
					.clone()
					.into_iter()
					.zip(fs.into_iter())
					.map(|(ty, value)| {
						let type_name = recursively_construct_types(value, ty, types, values)?;
						Ok::<_, &'static str>(Eip712DomainType {
							// In case of unnamed fields, we decide to name it by its type name
							name: type_name.clone(),
							r#type: type_name,
						})
					})
					.collect::<Result<_, _>>()?,
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
		(TypeDef::Compact(type_def_compact), _) =>
			(recursively_construct_types(v, type_def_compact.type_param, types, values)?, vec![]),
		(TypeDef::BitSequence(_), _) => return Err("This is only used when scale-info's bitvec feature is enabled and since we dont use that feature, this variant should be unreachable"),

		_ => return Err("Type and Value do not match"),
	};

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
	values: &mut BTreeMap<String, Vec<scale_value::Value>>,
) -> Result<Vec<Eip712DomainType>, &'static str> {
	match comp_value {
		Composite::Named(fs) => {
			let fs_map = fs.into_iter().collect::<BTreeMap<_, _>>();
			fields
				.clone()
				.into_iter()
				.map(|field| -> Result<Eip712DomainType, &'static str> {
					let field_name = field.name.ok_or("field name doesnt exist. shouldn't be possible since we are in Named variant")?.to_string();
					Ok(Eip712DomainType {
						name: field_name.clone(),
						// find out in which cases type name would be empty.
						r#type: recursively_construct_types(
							fs_map
								.get(&field_name)
								.ok_or("field with this name has to exist")?
								.clone(),
							field.ty,
							types, values
						)?,
					})
				})
				.collect::<Result<_, _>>()
		},
		Composite::Unnamed(fs) => {
			fields
				.clone()
				.into_iter()
				.zip(fs)
				.map(|(field, value)| -> Result<Eip712DomainType, &'static str>{
					let type_name = recursively_construct_types(value, field.ty, types, values)?;
					Ok(Eip712DomainType {
						// In case of unnamed fields, we decide to name it by its type name
						name: type_name.clone(),
						r#type: type_name,
					})
				})
				.collect::<Result<_, _>>()
		},
	}
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
		let id = registry.register_type(&MetaType::new::<BTreeMap<u8, u64>>());

		//let types = vec![];
		for (id, ty) in registry.types() {
			//types.push(ty)
			println!("ID: {:?},   Type: {:#?}", id, ty);
		}
		let mut b = BTreeMap::new();
		b.insert(1u8, 2u64);

		let portable_registry: scale_info::PortableRegistry = registry.into();
		let val =
			scale_value::scale::decode_as_type(&mut &b.encode()[..], id.id, &portable_registry)
				.unwrap();
		//println!("{:?}", encode_eip712_using_type_info(t, domain))
		println!("{:?}", val);
		println!("{:?}", serde_json::to_value(&val));
	}
}
