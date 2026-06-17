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

use cf_utilities::migrations::basics::{HasGenericVariant, HasVersion, Version};
use codec::{Decode, Encode};
use proptest::arbitrary::Arbitrary;
use scale_info::TypeInfo;

use crate::historical_compatibility::{
	tester_trait::{
		fuzzy_test_encode_decode_compatibility, HistoricalCompatibilityTester, SubTypeDetails,
		SubTypeIncompatibility, SubTypeLocation, TypeDiff, TypeIncompatibilityInfo, TypeName,
		TypeRef,
	},
	type_describer::describe_expected_type,
};

pub struct OnlineNodeTester {
	pub get_blockhash_from_spec_version: Box<dyn Fn(u32) -> Option<&'static str>>,
	pub node_url: &'static str,
}

impl HistoricalCompatibilityTester for OnlineNodeTester {
	fn test_call<
		V: Version,
		I: HasVersion<V, HistoricalType: Encode + std::fmt::Debug + TypeInfo + 'static + Arbitrary>
			+ HasGenericVariant<GenericType: Arbitrary>,
		O: HasVersion<
				V,
				HistoricalType: Encode + Decode + TypeInfo + std::fmt::Debug + 'static + Arbitrary,
			> + HasGenericVariant<GenericType: Arbitrary>,
	>(
		&mut self,
		_version: V,
		api_name: &'static str,
		method_name: &'static str,
	) -> Vec<TypeIncompatibilityInfo> {
		let canonical_runtime_patch_version = V::CANONICAL_RUNTIME_PATCH_VERSION_FOR_COMPATIBILITY_TEST.expect(
            "Encountered a runtime version with `CANONICAL_RUNTIME_PATCH_VERSION_FOR_COMPATIBILITY_TEST = None` in a compatibility test."
        );
		let blockhash =
			(self.get_blockhash_from_spec_version)(canonical_runtime_patch_version)
            .unwrap_or_else(|| panic!("No blockhash was specified for runtime version {canonical_runtime_patch_version} when trying to run compatibility test against online archive node."));

		let client = reqwest::blocking::Client::new();
		let call_method = format!("{}_{}", api_name, method_name);

		let outer_details = SubTypeDetails {
			type_name: TypeName::InputArgumentList,
			location: SubTypeLocation::Input { pos: None },
		};

		let result = fuzzy_test_encode_decode_compatibility(
			3,
			&I::HistoricalType::arbitrary(),
			&|value| {
				let params_hex = format!("0x{}", hex::encode(value.encode()));

				let input_fail = |error: String| SubTypeIncompatibility {
					sub_type_details: SubTypeDetails {
						type_name: TypeName::InputArgumentList,
						location: SubTypeLocation::Input { pos: None },
					},
					error,
				};
				let output_fail = |error: String| SubTypeIncompatibility {
					sub_type_details: SubTypeDetails {
						type_name: TypeName::Named { name: None },
						location: SubTypeLocation::Output,
					},
					error,
				};

				let response = client
					.post(self.node_url)
					.json(&serde_json::json!({
						"id": 1,
						"jsonrpc": "2.0",
						"method": "state_call",
						"params": [&call_method, params_hex, blockhash]
					}))
					.send()
					.map_err(|e| input_fail(format!("RPC request failed: {e}")))?;

				let body: serde_json::Value = response
					.json()
					.map_err(|e| input_fail(format!("Failed to parse RPC response: {e}")))?;

				if let Some(err) = body.get("error") {
					return Err(input_fail(format!("RPC returned error: {err}")));
				}

				let result_hex = body["result"].as_str().ok_or_else(|| {
					input_fail(format!("RPC response missing 'result' field: {body}"))
				})?;

				let encoded_output = hex::decode(result_hex.trim_start_matches("0x"))
					.map_err(|e| output_fail(format!("Failed to decode hex output: {e}")))?;

				Ok(encoded_output)
			},
			&|cursor| {
				let output_fail = |error: String| SubTypeIncompatibility {
					sub_type_details: SubTypeDetails {
						type_name: TypeName::Named { name: None },
						location: SubTypeLocation::Output,
					},
					error,
				};

				<O as HasVersion<V>>::HistoricalType::decode(cursor).map_err(|e| {
					output_fail(format!("Failed to decode output into HistoricalType: {e}"))
				})?;

				if !cursor.is_empty() {
					return Err(output_fail(format!(
						"Encoding mismatch: {} trailing bytes remain after decoding output",
						cursor.len(),
					)));
				}

				Ok(())
			},
			outer_details,
		);

		result
			.err()
			.into_iter()
			.map(|err| TypeIncompatibilityInfo {
				type_ref: TypeRef::RuntimeCall {
					api_name,
					method_name,
					version: canonical_runtime_patch_version,
				},
				type_diff: TypeDiff {
					expected_encoding: match err.sub_type_details.location {
						SubTypeLocation::Input { .. } =>
							describe_expected_type::<I::HistoricalType>(),
						SubTypeLocation::Output => describe_expected_type::<O::HistoricalType>(),
					},
					actual_encoding: String::new(),
				},
				sub_type_incompat: err,
			})
			.collect()
	}
}
