#![cfg(test)]

use cf_utilities::migrations::basics::{
	migrate_from_historical_type, migrate_to_historical_type, HasVersion, VariantName,
};
use codec::{Decode, Encode};
use frame_metadata::{v15::RuntimeMetadataV15, RuntimeMetadata, RuntimeMetadataPrefixed};
use proptest::{
	arbitrary::Arbitrary,
	test_runner::{Config, FileFailurePersistence, TestRunner},
};
use scale_decode::DecodeAsType;
use scale_json::ScaleDecodedToJson;

/// Load historical metadata for a given spec version.
///
/// Metadata files are stored in `state-chain/runtime_historical_metadata/` with the naming
/// convention `runtime_{spec_version}.scale`.
fn load_metadata(spec_version: u32) -> RuntimeMetadataV15 {
	let path = format!(
		"{}/state-chain/runtime_historical_metadata/runtime_{}.scale",
		env!("CARGO_MANIFEST_DIR").trim_end_matches("/state-chain/runtime"),
		spec_version,
	);
	let bytes =
		std::fs::read(&path).unwrap_or_else(|e| panic!("Failed to read metadata file {path}: {e}"));
	let prefixed = RuntimeMetadataPrefixed::decode(&mut &bytes[..])
		.expect("Failed to decode RuntimeMetadataPrefixed");
	match prefixed.1 {
		RuntimeMetadata::V15(m) => m,
		other => panic!("Expected V15 metadata, got version {:?}", other),
	}
}

/// Find the input and output type IDs for a runtime API method in the metadata.
///
/// Returns `(input_type_id, output_type_id)`. For methods with multiple input params,
/// the metadata stores them as a tuple type, so `input_type_id` refers to that tuple.
fn find_method_types(
	metadata: &RuntimeMetadataV15,
	api_name: &str,
	method_name: &str,
) -> (Vec<u32>, u32) {
	let api = metadata.apis.iter().find(|a| a.name == api_name).unwrap_or_else(|| {
		let available: Vec<_> = metadata.apis.iter().map(|a| a.name.as_str()).collect();
		panic!("API '{api_name}' not found in metadata. Available: {available:?}")
	});

	let method = api.methods.iter().find(|m| m.name == method_name).unwrap_or_else(|| {
		let available: Vec<_> = api.methods.iter().map(|m| m.name.as_str()).collect();
		panic!("Method '{method_name}' not found in API '{api_name}'. Available: {available:?}")
	});

	let input_type_ids: Vec<u32> = method.inputs.iter().map(|i| i.ty.id).collect();
	let output_type_id = method.output.id;

	(input_type_ids, output_type_id)
}

/// Assert that the given encoded bytes can be fully decoded against a type in the historical
/// metadata registry.
///
/// This verifies encoding compatibility: the bytes produced by the new (legacy) type must be
/// valid according to the type definition in the old runtime's metadata.
fn assert_decodable(encoded: &[u8], type_id: u32, metadata: &RuntimeMetadataV15) {
	let mut cursor = &encoded[..];
	<ScaleDecodedToJson as DecodeAsType>::decode_as_type(&mut cursor, type_id, &metadata.types)
		.unwrap_or_else(|e| {
			panic!("Failed to decode {} bytes against type_id {type_id}: {e}", encoded.len())
		});
	assert!(
		cursor.is_empty(),
		"Encoding mismatch: {} trailing bytes remain after decoding against type_id {type_id}",
		cursor.len(),
	);
}

/// Test that a runtime API call's input and output types, as encoded by the current legacy type
/// implementations, are compatible with the historical metadata.
///
/// # Type parameters
/// - `I`: The current input type (implements `Arbitrary` for proptest generation)
/// - `O`: The current output type (implements `Arbitrary` for proptest generation)
/// - `IOld`: The legacy input type (must be constructible `From<I>` and `Encode`)
/// - `OOld`: The legacy output type (must be constructible `From<O>` and `Encode`)
///
/// # Arguments
/// - `api_name`: The runtime API trait name (e.g. `"CustomRuntimeApi"`)
/// - `method_name`: The method name within the API (e.g. `"cf_pool_info"`)
/// - `test_against_version`: The historical spec version to test against
pub fn test_runtime_call_for_codec_compatibility_with_static_runtime<
	V: VariantName,
	I: Arbitrary + std::fmt::Debug + HasVersion<V, HistoricalType: Encode>,
	O: Arbitrary + std::fmt::Debug + HasVersion<V, HistoricalType: Encode>,
>(
	version: V,
	api_name: &'static str,
	method_name: &'static str,
	test_against_version: u32,
) {
	let path = module_path!();

	let mut runner = TestRunner::new(Config {
		source_file: Some(path),
		failure_persistence: Some(Box::new(FileFailurePersistence::SourceParallel(
			"proptest-regressions",
		))),
		cases: 200,
		..Default::default()
	});

	let metadata = load_metadata(test_against_version);
	let (input_type_ids, output_type_id) = find_method_types(&metadata, api_name, method_name);

	let strategy = (I::arbitrary(), O::arbitrary());

	runner
		.run(&strategy, |(input, output)| {
			// Encode the input using the legacy type
			let old_input = migrate_to_historical_type(version, input);
			let encoded_input = old_input.encode();

			// Verify that encoding is decodable against each input param's historical type
			let mut cursor = &encoded_input[..];
			for &type_id in &input_type_ids {
				<ScaleDecodedToJson as DecodeAsType>::decode_as_type(
					&mut cursor,
					type_id,
					&metadata.types,
				)
				.map_err(|e| {
					proptest::test_runner::TestCaseError::Fail(
						format!("Input decode failed for type_id {type_id}: {e}").into(),
					)
				})?;
			}
			if !cursor.is_empty() {
				return Err(proptest::test_runner::TestCaseError::Fail(
					format!("Input encoding mismatch: {} trailing bytes remain", cursor.len())
						.into(),
				));
			}

			// Encode the output using the legacy type
			let old_output = migrate_to_historical_type(version, output);
			let encoded_output = old_output.encode();

			// Verify output encoding
			assert_decodable(&encoded_output, output_type_id, &metadata);

			Ok(())
		})
		.unwrap();
}

pub fn lookup_blockhash_from_runtime_version_mainnet(version: u32) -> Option<&'static str> {
	match version {
		20012 => Some("0xc2068ad859fc5c3b3c7c5ecb3bd84033f1b5a0ce60e8c3b52cab4d22840eec37"),
		_ => None,
	}
}

pub fn test_runtime_call_for_codec_compatibility_with_historical_node<
	V: VariantName,
	I: Arbitrary + std::fmt::Debug + HasVersion<V, HistoricalType: Encode>,
	O: Arbitrary + std::fmt::Debug + HasVersion<V, HistoricalType: Encode + Decode>,
>(
	version: V,
	api_name: &'static str,
	method_name: &'static str,
	test_against_version: u32,
	archive_node_url: &'static str,
) {
	let blockhash = lookup_blockhash_from_runtime_version_mainnet(test_against_version).unwrap();

	let client = reqwest::blocking::Client::new();

	let path = module_path!();

	let mut runner = TestRunner::new(Config {
		source_file: Some(path),
		failure_persistence: Some(Box::new(FileFailurePersistence::SourceParallel(
			"proptest-regressions",
		))),
		cases: 10,
		..Default::default()
	});

	runner
		.run(&I::arbitrary(), |input| {
			// Encode the input using the legacy type
			let old_input = migrate_to_historical_type(version, input);
			let encoded_input = old_input.encode();

			// Call runtime API on the archive node at the given block hash
			let call_method = format!("{}_{}", api_name, method_name);
			let params_hex = format!("0x{}", hex::encode(&encoded_input));

			let response = client
				.post(archive_node_url)
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

			let encoded_output = hex::decode(result_hex.trim_start_matches("0x")).map_err(|e| {
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

			assert!(encoded_output_slice.is_empty(), "Decoding output did not consume all bytes!");

			let current_output: O = migrate_from_historical_type(version, historical_output);
			println!("Successfully migrated output: {current_output:?}");

			Ok(())
		})
		.unwrap()
}
