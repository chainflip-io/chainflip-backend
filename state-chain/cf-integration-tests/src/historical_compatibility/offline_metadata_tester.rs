use cf_utilities::migrations::basics::{HasGenericVariant, HasVersion, VariantName};
use codec::{Decode, Encode};
use frame_metadata::{v15::RuntimeMetadataV15, RuntimeMetadata, RuntimeMetadataPrefixed};
use proptest::arbitrary::Arbitrary;
use scale_decode::DecodeAsType;
use scale_info::TypeInfo;
use scale_json::ScaleDecodedToJson;
use std::collections::HashMap;

use crate::historical_compatibility::{
	tester_trait::{
		fuzzy_test_encode_decode_compatibility, HistoricalCompatibilityTester, SubTypeDetails,
		SubTypeIncompatibility, SubTypeLocation, TypeDiff, TypeIncompatibilityInfo, TypeName,
		TypeRef,
	},
	type_describer::{
		describe_expected_type, describe_metadata_type, describe_metadata_types_as_tuple,
		metadata_type_name,
	},
};

#[derive(Default)]
pub struct OfflineMetadataTester {
	loaded_metadata: HashMap<u32, RuntimeMetadataV15>,
}

impl OfflineMetadataTester {
	/// Load historical metadata for a given spec version.
	///
	/// Metadata files are stored in `state-chain/cf-integration-tests/historical_metadata/` with
	/// the naming convention `runtime_{spec_version}.scale`.
	fn load_metadata(&mut self, spec_version: u32) {
		self.loaded_metadata.entry(spec_version).or_insert_with(|| {
			let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
				.join("historical_metadata")
				.join(format!("runtime_{spec_version}.scale"));
			let bytes = std::fs::read(&path)
				.unwrap_or_else(|e| panic!("Failed to read metadata file {}: {e}", path.display()));
			let prefixed = RuntimeMetadataPrefixed::decode(&mut &bytes[..])
				.expect("Failed to decode RuntimeMetadataPrefixed");
			let metadata = match prefixed.1 {
				RuntimeMetadata::V15(m) => m,
				other => panic!("Expected V15 metadata, got version {:?}", other),
			};
			metadata
		});
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

	fn try_decode_as_type(
		&self,
		spec_version: u32,
		type_id: u32,
		cursor: &mut &[u8],
		sub_type_ref: SubTypeDetails,
	) -> Result<ScaleDecodedToJson, SubTypeIncompatibility> {
		let metadata = self.get_loaded_metadata(spec_version);
		<ScaleDecodedToJson as DecodeAsType>::decode_as_type(cursor, type_id, &metadata.types)
			.map_err(|e| SubTypeIncompatibility {
				sub_type_details: sub_type_ref,
				error: format!("{e}"),
			})
	}

	fn describe_metadata_type(&self, spec_version: u32, type_id: u32) -> String {
		describe_metadata_type(self.get_loaded_metadata(spec_version), type_id)
	}

	fn metadata_type_name(&self, spec_version: u32, type_id: u32) -> Option<String> {
		metadata_type_name(self.get_loaded_metadata(spec_version), type_id)
	}
}

impl HistoricalCompatibilityTester for OfflineMetadataTester {
	fn test_call<
		V: VariantName,
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
		let spec_version = V::LATEST_RUNTIME_PATCH_VERSION;

		// the metadata has to be loaded if it isn't already
		self.load_metadata(spec_version);

		let (input_type_ids, output_type_id) =
			self.find_method_types(spec_version, api_name, method_name);

		let input_type_names: Vec<_> = input_type_ids
			.iter()
			.map(|type_id| match self.metadata_type_name(spec_version, *type_id) {
				Some(name) => name,
				None => "<anonymous>".into(),
			})
			.collect();
		let input_type_name = format!("({})", input_type_names.join(", "));

		let input_result = fuzzy_test_encode_decode_compatibility(
			200,
			&I::HistoricalType::arbitrary(),
			&|value| Ok(value.encode()),
			&|encoded| {
				for (arg_pos, type_id) in input_type_ids.iter().enumerate() {
					self.try_decode_as_type(
						spec_version,
						*type_id,
						encoded,
						SubTypeDetails {
							type_name: TypeName::Named {
								name: self.metadata_type_name(spec_version, *type_id),
							},
							location: SubTypeLocation::Input { pos: Some(arg_pos as u32) },
						},
					)?;
				}
				Ok(())
			},
			SubTypeDetails {
				type_name: TypeName::Named { name: Some(input_type_name.clone()) },
				location: SubTypeLocation::Input { pos: None },
			},
		)
		.map_err(|err| {
			let metadata = self.get_loaded_metadata(spec_version);
			TypeIncompatibilityInfo {
				type_ref: TypeRef::RuntimeCall {
					api_name,
					method_name,
					version: V::LATEST_RUNTIME_PATCH_VERSION,
				},
				type_diff: TypeDiff {
					expected_encoding: describe_expected_type::<I::HistoricalType>(),
					actual_encoding: describe_metadata_types_as_tuple(metadata, &input_type_ids),
				},
				sub_type_incompat: err,
			}
		});

		let output_result = fuzzy_test_encode_decode_compatibility(
			200,
			&O::HistoricalType::arbitrary(),
			&|value| Ok(value.encode()),
			&|encoded| {
				self.try_decode_as_type(
					spec_version,
					output_type_id,
					encoded,
					SubTypeDetails {
						type_name: TypeName::Named {
							name: self.metadata_type_name(spec_version, output_type_id),
						},
						location: SubTypeLocation::Output,
					},
				)?;
				Ok(())
			},
			SubTypeDetails {
				type_name: TypeName::Named {
					name: self.metadata_type_name(spec_version, output_type_id),
				},
				location: SubTypeLocation::Output,
			},
		)
		.map_err(|err| TypeIncompatibilityInfo {
			type_ref: TypeRef::RuntimeCall {
				api_name,
				method_name,
				version: V::LATEST_RUNTIME_PATCH_VERSION,
			},
			type_diff: TypeDiff {
				expected_encoding: describe_expected_type::<O::HistoricalType>(),
				actual_encoding: self.describe_metadata_type(spec_version, output_type_id),
			},
			sub_type_incompat: err,
		});

		input_result.err().into_iter().chain(output_result.err()).collect()
	}
}
