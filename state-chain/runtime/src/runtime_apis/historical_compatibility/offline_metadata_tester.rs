use cf_utilities::migrations::basics::{
	migrate_from_generic_type, migrate_to_historical_type, HasGenericVariant, HasVersion,
	VariantName,
};
use codec::{Decode, Encode};
use frame_metadata::{v15::RuntimeMetadataV15, RuntimeMetadata, RuntimeMetadataPrefixed};
use proptest::{
	arbitrary::Arbitrary,
	test_runner::{Config, FileFailurePersistence, TestRunner},
};
use scale_decode::DecodeAsType;
use scale_json::ScaleDecodedToJson;
use std::{collections::HashMap, fmt::Debug};

use crate::runtime_apis::historical_compatibility::tester_trait::HistoricalCompatibilityTester;

pub struct OfflineMetadataTester {
	loaded_metadata: HashMap<u32, RuntimeMetadataV15>,
}

impl OfflineMetadataTester {
	pub fn new() -> OfflineMetadataTester {
		OfflineMetadataTester { loaded_metadata: Default::default() }
	}

	/// Load historical metadata for a given spec version.
	///
	/// Metadata files are stored in `state-chain/runtime_historical_metadata/` with the naming
	/// convention `runtime_{spec_version}.scale`.
	fn load_metadata(&mut self, spec_version: u32) {
		if !self.loaded_metadata.contains_key(&spec_version) {
			let path = format!(
				"{}/state-chain/runtime_historical_metadata/runtime_{}.scale",
				env!("CARGO_MANIFEST_DIR").trim_end_matches("/state-chain/runtime"),
				spec_version,
			);
			let bytes = std::fs::read(&path)
				.unwrap_or_else(|e| panic!("Failed to read metadata file {path}: {e}"));
			let prefixed = RuntimeMetadataPrefixed::decode(&mut &bytes[..])
				.expect("Failed to decode RuntimeMetadataPrefixed");
			let metadata = match prefixed.1 {
				RuntimeMetadata::V15(m) => m,
				other => panic!("Expected V15 metadata, got version {:?}", other),
			};
			self.loaded_metadata.insert(spec_version, metadata);
		}
	}

	/// Gets the metadata if it's already loaded
	fn get_loaded_metadata(&self, spec_version: u32) -> &RuntimeMetadataV15 {
		self.loaded_metadata.get(&spec_version).unwrap()
	}

	/// Find the input and output type IDs for a runtime API method in the metadata.
	///
	/// Returns `(input_type_id, output_type_id)`. For methods with multiple input params,
	/// the metadata stores them as a tuple type, so `input_type_id` refers to that tuple.
	fn find_method_types(
		&self,
		spec_version: u32,
		api_name: &str,
		method_name: &str,
	) -> (Vec<u32>, u32) {
		let metadata = self.get_loaded_metadata(spec_version);

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

	fn decode_as_type(
		&self,
		spec_version: u32,
		type_id: u32,
		cursor: &mut &[u8],
	) -> ScaleDecodedToJson {
		let metadata = self.get_loaded_metadata(spec_version);
		<ScaleDecodedToJson as DecodeAsType>::decode_as_type(cursor, type_id, &metadata.types)
			.map_err(|e| format!("Input decode failed for type_id {type_id}: {e}"))
			.unwrap()
	}
}

impl HistoricalCompatibilityTester for OfflineMetadataTester {
	fn test_call<
		V: VariantName,
		I: std::fmt::Debug
			+ HasVersion<V, HistoricalType: Encode + std::fmt::Debug>
			+ HasGenericVariant<GenericType: Arbitrary>,
		O: std::fmt::Debug
			+ HasVersion<V, HistoricalType: Encode + Decode>
			+ HasGenericVariant<GenericType: Arbitrary>,
	>(
		&mut self,
		version: V,
		api_name: &'static str,
		method_name: &'static str,
		file_path: &'static str,
	) {
		let spec_version = V::LATEST_RUNTIME_PATCH_VERSION;

		let mut runner = TestRunner::new(Config {
			source_file: Some(file_path),
			failure_persistence: Some(Box::new(FileFailurePersistence::SourceParallel(
				"proptest-regressions",
			))),
			cases: 200,
			..Default::default()
		});

		// the metadata has to be loaded if it isn't already
		self.load_metadata(spec_version);

		let (input_type_ids, output_type_id) =
			self.find_method_types(spec_version, api_name, method_name);

		let strategy = (
			<I as HasGenericVariant>::GenericType::arbitrary(),
			<O as HasGenericVariant>::GenericType::arbitrary(),
		);

		runner
			.run(&strategy, |(generic_input, generic_output)| {
				// Encode the input using the legacy type
				let input: I = migrate_from_generic_type(generic_input);
				let old_input = migrate_to_historical_type(version, input);
				let encoded_input = old_input.encode();

				// Verify that encoding is decodable against each input param's historical type
				let mut cursor = &encoded_input[..];
				for &type_id in &input_type_ids {
					let _decoded = self.decode_as_type(spec_version, type_id, &mut cursor);
				}
				assert!(
					cursor.is_empty(),
					"Encoding mismatch: {} trailing bytes remain after decoding inputs (type ids {input_type_ids:?})",
					cursor.len(),
				);

				// Encode the output using the legacy type
				let output: O = migrate_from_generic_type(generic_output);
				let old_output = migrate_to_historical_type(version, output);
				let encoded_output = old_output.encode();
				let mut cursor = &encoded_output[..];
				let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
					self.decode_as_type(spec_version, output_type_id, &mut cursor)
				}));
				if let Err(e) = result {
					panic!(
						"Output decode panicked for old_input: {:?}\n\nOriginal panic: {:?}",
						old_input, e
					);
				}

				assert!(
					cursor.is_empty(),
					"Encoding mismatch: {} trailing bytes remain after decoding output (type_id {output_type_id})",
					cursor.len(),
				);

				Ok(())
			})
			.unwrap();
	}
}
