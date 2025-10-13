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
use cf_primitives::ChainflipNetwork;
use pallet_cf_environment::TransactionMetadata;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// Represents the name and type pair
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Eip712DomainType {
	pub name: String,
	#[serde(rename = "type")]
	pub r#type: String,
}

pub type Types = BTreeMap<String, Vec<Eip712DomainType>>;

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TypedData {
	/// Signing domain metadata. The signing domain is the intended context for the signature (e.g.
	/// the dapp, protocol, etc. that it's intended for). This data is used to construct the domain
	/// seperator of the message.
	#[serde(default)]
	pub domain: EIP712Domain,
	/// The custom types used by this message.
	pub types: Types,
	#[serde(rename = "primaryType")]
	/// The type of the message.
	pub primary_type: String,
	// The message to be signed.
	pub message: BTreeMap<String, Value>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EIP712Domain {
	///  The user readable name of signing domain, i.e. the name of the DApp or the protocol.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub name: Option<String>,

	/// The current major version of the signing domain. Signatures from different versions are not
	/// compatible.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub version: Option<String>,

	/// The EIP-155 chain id. The user-agent should refuse signing if it does not match the
	/// currently active chain.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub chain_id: Option<sp_core::U256>,

	/// The address of the contract that will verify the signature.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub verifying_contract: Option<sp_core::H160>,

	/// A disambiguating salt for the protocol. This can be used as a domain separator of last
	/// resort.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub salt: Option<[u8; 32]>,
}

// Building the EIP-712 typed data customized to the types we expect
// and validate in the pallet_cf_environment::submit_runtime_call.rs
pub fn build_eip712_typed_data(
	chainflip_network: &ChainflipNetwork,
	call: Vec<u8>,
	transaction_metadata: &TransactionMetadata,
	spec_version: u32,
) -> Result<TypedData, serde_json::Error> {
	let json = serde_json::json!({
		"domain": {
			"name": chainflip_network.as_str().to_string(),
			"version": spec_version.to_string(),
		},
		"types": {
			"EIP712Domain": [
				{
					"name": "name",
					"type": "string"
				},
				{
					"name": "version",
					"type": "string"
				},
			],
			"Metadata": [
				{ "name": "nonce", "type": "uint32" },
				{ "name": "expiryBlock", "type": "uint32" },
			],
			"RuntimeCall": [
				{
					"name": "value",
					"type": "bytes"
				}
			],
			"Transaction": [
				{
					"name": "call",
					"type": "RuntimeCall"
				},
				{
					"name": "metadata",
					"type": "Metadata"
				},
			]
		},
		"primaryType": "Transaction",
		"message": {
			"call": {
				"value": format!("0x{}", hex::encode(&call)),
			},
			"metadata": {
				"nonce": transaction_metadata.nonce.to_string(),
				"expiryBlock": transaction_metadata.expiry_block.to_string(),
			},
		}
	});

	serde_json::from_value(json)
}
