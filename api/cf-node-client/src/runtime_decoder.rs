use crate::{
	error_decoder,
	error_decoder::ErrorDecoder,
	events_decoder::{DynamicEventError, DynamicEvents, EventsDecoder},
};
use sp_api::runtime_decl_for_core::CoreV5;
use sp_runtime::DispatchError;
use std::sync::OnceLock;

pub fn build_runtime_version() -> &'static sp_version::RuntimeVersion {
	static BUILD_RUNTIME_VERSION: OnceLock<sp_version::RuntimeVersion> = OnceLock::new();
	BUILD_RUNTIME_VERSION.get_or_init(state_chain_runtime::Runtime::version)
}

pub struct RuntimeDecoder {
	pub events_decoder: EventsDecoder,
	pub error_decoder: ErrorDecoder,
}

impl Default for RuntimeDecoder {
	fn default() -> Self {
		let opaque_metadata = state_chain_runtime::Runtime::metadata_at_version(15)
			.expect("Version 15 should be supported by the runtime.");

		Self::new(opaque_metadata)
	}
}

impl RuntimeDecoder {
	pub fn new(opaque_metadata: sp_core::OpaqueMetadata) -> Self {
		Self {
			events_decoder: EventsDecoder::new(&opaque_metadata),
			error_decoder: ErrorDecoder::new(opaque_metadata),
		}
	}

	pub fn decode_extrinsic_events(
		&self,
		extrinsic_index: usize,
		bytes: Option<Vec<u8>>,
	) -> Result<DynamicEvents, DynamicEventError> {
		self.events_decoder.decode_extrinsic_events(extrinsic_index, bytes)
	}

	pub fn decode_dispatch_error(
		&self,
		dispatch_error: DispatchError,
	) -> error_decoder::DispatchError {
		self.error_decoder.decode_dispatch_error(dispatch_error)
	}
}

/// Common macro to extract dynamic events
#[macro_export]
macro_rules! extract_dynamic_event {
    ($dynamic_events:expr, $cf_static_event_variant:path, { $($field:ident),* }, $result:expr) => {

		match $dynamic_events
			.find_static_event::<$cf_static_event_variant>(false)?
		{
			Some($cf_static_event_variant { $($field),*, .. } ) => Ok($result),
			None => Err($crate::events_decoder::DynamicEventError::StaticEventNotFound(stringify!($cf_static_event_variant)))
		}
    };
}
