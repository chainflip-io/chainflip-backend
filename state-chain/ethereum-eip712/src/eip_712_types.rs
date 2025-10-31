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
