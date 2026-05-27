use cf_utilities::migrations::basics::{
	migrate_from_generic_type, migrate_from_historical_type, migrate_to_historical_type,
	HasGenericVariant, HasVersion, VariantName,
};
use codec::{Decode, Encode};
use proptest::{
	arbitrary::Arbitrary,
	test_runner::{Config, FileFailurePersistence, TestRunner},
};
use std::fmt::Debug;

use crate::runtime_apis::historical_compatibility::tester_trait::HistoricalCompatibilityTester;

pub struct OnlineNodeTester {
	pub get_blockhash_from_spec_version: Box<dyn Fn(u32) -> Option<&'static str>>,
	pub node_url: &'static str,
}

impl OnlineNodeTester {}

impl HistoricalCompatibilityTester for OnlineNodeTester {
	fn test_call<
		V: VariantName,
		I: std::fmt::Debug
			+ HasVersion<V, HistoricalType: Encode + std::fmt::Debug>
			+ HasGenericVariant<GenericType: Arbitrary>,
		O: std::fmt::Debug
			+ HasVersion<V, HistoricalType: Encode + Decode + std::fmt::Debug>
			+ HasGenericVariant<GenericType: Arbitrary>,
	>(
		&mut self,
		version: V,
		api_name: &'static str,
		method_name: &'static str,
		file_path: &'static str,
	) {
		let Some(blockhash) =
			(self.get_blockhash_from_spec_version)(V::LATEST_RUNTIME_PATCH_VERSION)
		else {
			return;
		};

		let client = reqwest::blocking::Client::new();

		let mut runner = TestRunner::new(Config {
			source_file: Some(file_path),
			failure_persistence: Some(Box::new(FileFailurePersistence::SourceParallel(
				"proptest-regressions",
			))),
			cases: 10,
			..Default::default()
		});

		runner
			.run(&<I as HasGenericVariant>::GenericType::arbitrary(), |generic_input| {
				// Encode the input using the legacy type
				let input: I = migrate_from_generic_type(generic_input);
				let old_input = migrate_to_historical_type(version, input);
				let encoded_input = old_input.encode();

				// Call runtime API on the archive node at the given block hash
				let call_method = format!("{}_{}", api_name, method_name);
				let params_hex = format!("0x{}", hex::encode(&encoded_input));

				let response = client
					.post(self.node_url)
					.json(&serde_json::json!({
						"id": 1,
						"jsonrpc": "2.0",
						"method": "state_call",
						"params": [call_method, params_hex, blockhash]
					}))
					.send()
					.map_err(|e| {
						proptest::test_runner::TestCaseError::Fail(
							format!("RPC request failed: {e}").into(),
						)
					})?;

				let body: serde_json::Value = response.json().map_err(|e| {
					proptest::test_runner::TestCaseError::Fail(
						format!("Failed to parse RPC response: {e}").into(),
					)
				})?;

				let result_hex = body["result"].as_str().ok_or_else(|| {
					proptest::test_runner::TestCaseError::Fail(
						format!("RPC response missing 'result' field: {body}").into(),
					)
				})?;

				let encoded_output =
					hex::decode(result_hex.trim_start_matches("0x")).map_err(|e| {
						proptest::test_runner::TestCaseError::Fail(
							format!("Failed to decode hex output: {e}").into(),
						)
					})?;

				let encoded_output_slice = &mut &encoded_output[..];

				// Decode into O::HistoricalType and migrate to O
				let historical_output = <O as HasVersion<V>>::HistoricalType::decode(
					encoded_output_slice,
				)
				.map_err(|e| {
					proptest::test_runner::TestCaseError::Fail(
						format!("Failed to decode output into HistoricalType: {e}").into(),
					)
				})?;

				assert!(
					encoded_output_slice.is_empty(),
					"Decoding output did not consume all bytes!"
				);

				let current_output: O = migrate_from_historical_type(version, historical_output);
				println!("Successfully migrated output: {current_output:?}");

				Ok(())
			})
			.unwrap()
	}
}
