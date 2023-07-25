use codec::Decode;
use std::collections::BTreeMap;
use thiserror::Error;

pub struct ErrorDecoder {
	errors: BTreeMap<u8, (String, BTreeMap<u8, (String, Vec<String>)>)>,
}

impl Default for ErrorDecoder{
	fn default() -> Self {
		let metadata = state_chain_runtime::Runtime::metadata();

		let metadata: frame_support::metadata::RuntimeMetadataLastVersion = match metadata.1 {
			frame_support::metadata::RuntimeMetadata::V14(metadata) => metadata,
			_ => {
				panic!("If this breaks change the version above to match new metadata version, and update the test below like you should have.");
			},
		};

		Self {
			errors: metadata
				.pallets
				.into_iter()
				.filter_map(|pallet| {
					Some((
						pallet.index,
						(
							pallet.name,
							match &metadata.types.types[pallet.error?.ty.id as usize].ty.type_def {
								scale_info::TypeDef::Variant(variant_type_def) => variant_type_def
									.variants
									.iter()
									.map(|variant| {
										(
											variant.index,
											(variant.name.clone(), variant.docs.clone()),
										)
									})
									.collect::<BTreeMap<_, _>>(),
								_ => panic!("Pallet error type is not an Enum"),
							},
						),
					))
				})
				.collect::<BTreeMap<_, _>>(),
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

#[derive(Error, Debug)]
pub enum DispatchError {
	#[error("{0:?}")]
	DispatchError(sp_runtime::DispatchError),
	#[error("Module error ‘{name}‘ from pallet ‘{pallet}‘: ‘{error}‘")]
	KnownModuleError { pallet: String, name: String, error: String },
}

#[cfg(test)]
mod tests {
	#[test]
	fn check_metadata_version() {
		let metadata = state_chain_runtime::Runtime::metadata();

		match metadata.1 {
			frame_support::metadata::RuntimeMetadata::V14(_) => {},
			_ => {
				panic!("If this breaks also change version above, to match new metadata version");
			},
		}
	}
}
