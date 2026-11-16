// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0
#[cfg(feature = "std")]
use super::eip712::Types;
use super::{
	eip712::{EIP712Domain, Eip712DomainType, Eip712Error, TypedData},
	encode_eip712_using_type_info_fast,
};
use codec::{alloc::string::ToString, Encode};
use scale_info::{prelude::string::String, TypeInfo};
use sp_std::vec;

// Building the EIP-712 typed data for both validation in the environment
// pallet and for the encoding in the rpc layer, which will be serialized.
pub fn build_eip712_typed_data<T: TypeInfo + Encode + 'static>(
	chainflip_extrinsic: T,
	chainflip_network_name: String,
	spec_version: u32,
) -> Result<TypedData, Eip712Error> {
	let domain = EIP712Domain {
		name: Some(chainflip_network_name),
		version: Some(spec_version.to_string()),
		chain_id: None,
		verifying_contract: None,
		salt: None,
	};

	let typed_data = encode_eip712_using_type_info_fast(chainflip_extrinsic, domain)?;

	let mut types = typed_data.types.clone();
	types.insert(
		"EIP712Domain".to_string(),
		vec![
			Eip712DomainType { name: "name".to_string(), r#type: "string".to_string() },
			Eip712DomainType { name: "version".to_string(), r#type: "string".to_string() },
		],
	);

	Ok(TypedData {
		domain: typed_data.domain,
		types,
		primary_type: typed_data.primary_type,
		message: typed_data.message,
	})
}

/// EIP-712 typed data in the JSON shape expected by `eth_signTypedData_v4` signers (ethers,
/// viem, MetaMask), as returned by the `cf_encode_non_native_call` RPC.
#[cfg(feature = "std")]
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JsonTypedData {
	pub domain: EIP712Domain,
	pub types: Types,
	#[serde(rename = "primaryType")]
	pub primary_type: String,
	pub message: std::collections::BTreeMap<String, serde_json::Value>,
}

#[cfg(feature = "std")]
pub fn to_json_typed_data(typed_data: TypedData) -> Result<JsonTypedData, String> {
	let message_scale_value: scale_value::Value =
		typed_data.message.clone().stringify_integers().into();

	Ok(JsonTypedData {
		domain: typed_data.domain,
		types: typed_data.types,
		primary_type: typed_data.primary_type,
		message: serde_json::to_value(message_scale_value)
			.map_err(|e| format!("Failed to serialize message to JSON: {}", e))?
			.as_object()
			.ok_or(
				"the primary type is not a JSON object but one of the primitive types".to_string(),
			)?
			.clone()
			.into_iter()
			.collect(),
	})
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::eip712::Eip712;

	/// Cross-checks our hash against an independent implementation that only sees the JSON.
	fn assert_alloy_signing_hash_matches(typed_data: &TypedData) {
		let alloy_typed_data: alloy_dyn_abi::TypedData = serde_json::from_value(
			serde_json::to_value(to_json_typed_data(typed_data.clone()).unwrap()).unwrap(),
		)
		.unwrap();
		assert_eq!(
			alloy_typed_data.eip712_signing_hash().unwrap().0,
			crate::hash::keccak256(typed_data.encode_eip712().unwrap())
		);
	}

	#[test]
	fn test_complex_root_typed_data() {
		use crate::extra_tests::test_types::test_complex_type_with_vecs_and_enums::*;

		// Create instance with complex nested data
		let test_value = ComplexRoot {
			field_with_vector: TypeWithVector {
				items: vec![1, 2, 3, 4, 5],
				description: "Test description".to_string(),
			},
			field_with_vector_2: TypeWithVector {
				items: vec![],
				description: "Empty items".to_string(),
			},
			field_with_enum: TypeWithEnum {
				status: StatusEnum::Pending { reason: "Waiting for approval".to_string() },
				id: 42,
			},
			field_with_both: TypeWithBoth {
				tags: vec!["tag1".to_string(), "tag2".to_string(), "tag3".to_string()],
				priority: Priority::High,
				nested_items: vec![100, 200, 300],
			},
			field_with_enum_2: TypeWithEnum { status: StatusEnum::Active, id: 50 },
			field_with_enum_3: TypeWithEnum {
				status: StatusEnum::Completed { count: 5, timestamp: 6 },
				id: 60,
			},
		};

		// Build EIP-712 typed data
		let typed_data = build_eip712_typed_data(test_value, "Chainflip-Mainnet".to_string(), 1)
			.expect("Failed to build EIP-712 typed data");

		assert_eq!(
			hex::encode(crate::hash::keccak256(typed_data.encode_eip712().unwrap())),
			"e44f6b83980196ee164ddac1556600fef8cfe2f4d2c862ce5247ad6f2d4923df"
		);
		assert_alloy_signing_hash_matches(&typed_data);
	}

	#[test]
	fn test_simple_root_typed_data() {
		use crate::extra_tests::test_types::test_vec_of_enum::*;

		// Create instance with sample data
		let test_value = SimpleRoot {
			inner: InnerStruct {
				colors: vec![Color::Red, Color::Blue { intensity: 128 }, Color::Green],
			},
		};

		// Build EIP-712 typed data
		let typed_data = build_eip712_typed_data(test_value, "Chainflip-Mainnet".to_string(), 1)
			.expect("Failed to build EIP-712 typed data");

		assert_eq!(
			hex::encode(crate::hash::keccak256(typed_data.encode_eip712().unwrap())),
			"e3be730ad92618b9a3de27d3d12cdff5bea2a3fe8dd7f4d6f117267088456c8c"
		);
		assert_alloy_signing_hash_matches(&typed_data);
	}
}
