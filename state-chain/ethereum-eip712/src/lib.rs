#![cfg_attr(not(feature = "std"), no_std)]

use scale_info::{prelude::string::ToString, MetaType, Registry, TypeDef, TypeInfo};
use serde::Serialize;

use crate::eip712::{EIP712Domain, Eip712, Eip712DomainType, Eip712Error, TypedData};

pub mod bytes;
pub mod eip712;
pub mod hash;
pub mod lexer;
pub mod serde_helpers;

pub fn encode_eip712_using_type_info<T: TypeInfo + Serialize + 'static>(
	value: T,
	domain: EIP712Domain,
) -> Result<[u8; 32], Eip712Error> {
	let mut registry = Registry::new();
	registry.register_type(&MetaType::new::<T>());

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
		message: match serde_json::to_value(&value).unwrap() {
			serde_json::Value::Object(fields) => fields.into_iter().collect(),
			_ => Default::default(), //todo
		},
	};

	typed_data.encode_eip712()
}

#[cfg(test)]
pub mod tests {
	use super::*;
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

		#[derive(TypeInfo, Clone, Serialize)]
		pub struct Mail {
			pub from: Person,
			pub to: Person,
			pub message: String,
		}

		#[derive(TypeInfo, Clone, Serialize)]
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
}
