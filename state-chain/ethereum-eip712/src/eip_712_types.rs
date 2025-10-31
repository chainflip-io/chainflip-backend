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
// use pallet_cf_environment::submit_runtime_call::ChainflipExtrinsic;
use codec::{alloc::string::ToString, Encode};
use scale_info::{prelude::string::String, TypeInfo};
use sp_std::vec;

// Building the EIP-712 typed data customized to the types we expect
// and validate in the pallet_cf_environment::submit_runtime_call.rs
pub fn build_eip712_typed_data<T: TypeInfo + Encode + 'static>(
	chainflip_network_name: String,
	chainflip_extrinsic: T,
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

	// let message_scale_value: scale_value::Value = typed_data.message.clone().into();

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
		// message: serde_json::to_value(message_scale_value)?
		// 	.as_object()
		// 	.ok_or(Eip712Error::Message(
		// 		"the primary type is not a JSON object but one of the primitive types".to_string(),
		// 	))?
		// 	.clone()
		// 	.into_iter()
		// 	.collect(),
	})
}

// #[test]
// #[ignore = "used to generate the Json typed data to then test in the browser"]
// fn test_build_eip712_typed_data() {
// 	use pallet_cf_ingress_egress::DepositWitness;
// 	let chainflip_network = ChainflipNetwork::Mainnet;

// 	let call = state_chain_runtime::RuntimeCall::SolanaIngressEgress(
// 		pallet_cf_ingress_egress::Call::process_deposits {
// 			deposit_witnesses: vec![
// 				DepositWitness {
// 					deposit_address: [3u8; 32].into(),
// 					amount: 5000u64,
// 					asset: cf_chains::assets::sol::Asset::Sol,
// 					deposit_details: (),
// 				},
// 				DepositWitness {
// 					deposit_address: [4u8; 32].into(),
// 					amount: 6000u64,
// 					asset: cf_chains::assets::sol::Asset::SolUsdc,
// 					deposit_details: (),
// 				},
// 			],
// 			block_height: 6u64,
// 		},
// 	);

// 	let transaction_metadata = TransactionMetadata { nonce: 1, expiry_block: 1000 };
// 	let spec_version = 1;

// 	let typed_data_result =
// 		build_eip712_typed_data(&chainflip_network, call, &transaction_metadata, spec_version)
// 			.unwrap();

// 	println!(
// 		"Typed Data: {:#?}",
// 		serde_json::to_writer_pretty(std::io::stdout(), &typed_data_result).unwrap()
// 	);
// }
