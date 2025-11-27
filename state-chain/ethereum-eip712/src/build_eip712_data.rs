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
use super::{
	eip712::{EIP712Domain, Eip712DomainType, Eip712Error, TypedData},
	encode_eip712_using_type_info,
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

	let typed_data = encode_eip712_using_type_info(chainflip_extrinsic, domain)?;

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

#[cfg(feature = "std")]
pub fn to_ethers_typed_data(
	typed_data: TypedData,
) -> Result<ethers_core::types::transaction::eip712::TypedData, String> {
	let message_scale_value: scale_value::Value =
		typed_data.message.clone().stringify_integers().into();

	Ok(ethers_core::types::transaction::eip712::TypedData {
		domain: ethers_core::types::transaction::eip712::EIP712Domain {
			name: typed_data.domain.name,
			version: typed_data.domain.version,
			chain_id: typed_data.domain.chain_id,
			verifying_contract: typed_data.domain.verifying_contract,
			salt: typed_data.domain.salt,
		},
		types: typed_data
			.types
			.iter()
			.map(|(s, tys)| {
				(
					s.clone(),
					tys.iter()
						.map(|t| ethers_core::types::transaction::eip712::Eip712DomainType {
							name: t.name.clone(),
							r#type: t.r#type.clone(),
						})
						.collect(),
				)
			})
			.collect(),
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
