use codec::Decode;
use std::collections::BTreeMap;
use thiserror::Error;

pub struct ErrorDecoder {
	errors: BTreeMap<u8, (String, BTreeMap<u8, (String, Vec<String>)>)>,
}

impl Default for ErrorDecoder {
	fn default() -> Self {
		let metadata = frame_metadata::RuntimeMetadataPrefixed::decode(
			&mut state_chain_runtime::Runtime::metadata_at_version(15)
				.expect("Version 15 should be supported by the runtime.")
				.as_slice(),
		)
		.expect("Runtime metadata should be valid.");

		let metadata: frame_metadata::v15::RuntimeMetadataV15 = match metadata.1 {
			frame_metadata::RuntimeMetadata::V15(metadata) => metadata,
			_ => {
				panic!("If this breaks change the version above to match new metadata version, and update
		the test below like you should have.");
			},
		};

		Self {
			errors: match metadata
				.types
				.resolve(metadata.outer_enums.error_enum_ty.id)
				.unwrap()
				.type_def
				.clone()
			{
				scale_info::TypeDef::Variant(runtime_error_type) => runtime_error_type
					.variants
					.into_iter()
					.map(|pallet_error_type| {
						(
							pallet_error_type.index,
							(pallet_error_type.name, {
								let type_id = pallet_error_type
									.fields
									.first()
									.expect("error variant has exactly one field")
									.ty
									.id;
								match metadata.types.resolve(type_id).unwrap().type_def.clone() {
									scale_info::TypeDef::Variant(pallet_errors) => pallet_errors
										.variants
										.into_iter()
										.map(|error_variant| {
											(
												error_variant.index,
												(error_variant.name, error_variant.docs),
											)
										})
										.collect::<BTreeMap<_, _>>(),
									_ => panic!("Inner error type is not an Enum"),
								}
							}),
						)
					})
					.collect::<BTreeMap<_, _>>(),
				_ => panic!("Outer error type is not an Enum"),
			},
		}
	}
}

impl ErrorDecoder {
	pub fn decode_dispatch_error(
		&self,
		dispatch_error: sp_runtime::DispatchError,
	) -> DispatchError {
		match dispatch_error {
			sp_runtime::DispatchError::Module(module_error) => {
				if let Some((pallet, (name, error))) =
					u8::decode(&mut &module_error.error[..]).ok().and_then(|error_index| {
						self.errors.get(&module_error.index).and_then(|(pallet, pallet_errors)| {
							pallet_errors.get(&error_index).map(|error| (pallet, error))
						})
					}) {
					DispatchError::KnownModuleError {
						pallet: pallet.clone(),
						name: name.clone(),
						error: error.join(" "),
					}
				} else {
					DispatchError::DispatchError(sp_runtime::DispatchError::Module(module_error))
				}
			},
			dispatch_error => DispatchError::DispatchError(dispatch_error),
		}
	}
}

#[derive(Error, Debug, Clone)]
pub enum DispatchError {
	#[error("{0:?}")]
	DispatchError(sp_runtime::DispatchError),
	#[error("Module error ‘{name}‘ from pallet ‘{pallet}‘: ‘{error}‘")]
	KnownModuleError { pallet: String, name: String, error: String },
}

#[cfg(test)]
mod tests {
	use super::*;
	use codec::{Decode, Encode};
	use sp_runtime::ModuleError;

	#[test]
	fn check_metadata_version() {
		let metadata = frame_metadata::RuntimeMetadataPrefixed::decode(
			&mut state_chain_runtime::Runtime::metadata_at_version(15)
				.expect("Version 15 should be supported by the runtime.")
				.as_slice(),
		)
		.expect("Runtime metadata should be valid.");

		match metadata.1 {
			frame_metadata::RuntimeMetadata::V15(..) => {},
			_ => {
				panic!(
					"If this breaks change this test to match new metadata version, and update the code above like you should have."
				);
			},
		};
	}

	#[test]
	fn check_error_decoding() {
		let encoded_error = sp_runtime::DispatchError::from(
			pallet_cf_funding::Error::<state_chain_runtime::Runtime>::NoPendingRedemption,
		)
		.encode();
		let dispatch_error = sp_runtime::DispatchError::decode(&mut &encoded_error[..]).unwrap();

		// Message should be erased.
		assert!(matches!(
			dispatch_error,
			sp_runtime::DispatchError::Module(ModuleError { message: None, .. })
		));

		match ErrorDecoder::default().decode_dispatch_error(dispatch_error) {
			super::DispatchError::KnownModuleError { pallet, name, error } => {
				assert_eq!(pallet, "Funding");
				assert_eq!(name, "NoPendingRedemption");
				assert_eq!(error, "An invalid redemption has been witnessed: the account has no pending redemptions.")
			},
			_ => panic!("Unexpected error type"),
		}
	}
}
