use crate::{
	error_decoder::{DispatchError, ErrorDecoder},
	events_decoder::{DynamicEventError, DynamicEvents, EventsDecoder},
};
use frame_support::dispatch::DispatchInfo;
use sp_api::runtime_decl_for_core::CoreV5;
use sp_core::{
	serde::{Deserialize, Serialize},
	H256,
};
use std::sync::OnceLock;

pub mod error_decoder;
pub mod events_decoder;
pub mod signer;
pub mod subxt_state_chain_config;

/// This macro generates a strongly typed API from a WASM file. All types are substituted with
/// corresponding types that implement some traits, allowing subxt to scale encode/decode these
/// types. However, this makes it challenging to convert from the new generated types to cf types,
/// especially for hierarchical types. The trick is use the `substitute_type` directive to instruct
/// the subxt macro to use certain types in place of the default generated types. Example:
/// ```ignore
/// substitute_type(path = "cf_chains::ChannelRefundParametersGeneric<A>", with = "::subxt::utils::Static<cf_chains::ChannelRefundParametersGeneric<A>>")
/// ```
/// * This will generate: ::subxt::utils::Static<cf_chains::ChannelRefundParametersGeneric<A>> in
///   place of the default runtime_types::cf_chains::ChannelRefundParametersGeneric<A>
/// * The `::subxt::utils::Static` is required to wrap the type and implement the necessary
///   `EncodeAsType` and `DecodeAsType` traits.
/// * Any cf type that needs to be substituted must be defined in the `substitute_type` directive.
#[subxt::subxt(
	runtime_path = "../../target/release/wbuild/state-chain-runtime/state_chain_runtime.wasm",
	substitute_type(
		path = "cf_chains::address::EncodedAddress",
		with = "::subxt::utils::Static<cf_chains::address::EncodedAddress>"
	),
	substitute_type(
		path = "cf_primitives::chains::ForeignChain",
		with = "::subxt::utils::Static<cf_primitives::chains::ForeignChain>"
	),
	substitute_type(
		path = "cf_chains::ChannelRefundParametersGeneric<A>",
		with = "::subxt::utils::Static<cf_chains::ChannelRefundParametersGeneric<A>>"
	)
)]
pub mod cf_static_runtime {}

pub fn build_runtime_version() -> &'static sp_version::RuntimeVersion {
	static BUILD_RUNTIME_VERSION: OnceLock<sp_version::RuntimeVersion> = OnceLock::new();
	BUILD_RUNTIME_VERSION.get_or_init(state_chain_runtime::Runtime::version)
}

pub type ExtrinsicDetails =
	(H256, Vec<state_chain_runtime::RuntimeEvent>, state_chain_runtime::Header, DispatchInfo);

pub type ExtrinsicData = (H256, DynamicEvents, state_chain_runtime::Header, DispatchInfo);

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default)]
pub enum WaitFor {
	// Return immediately after the extrinsic is submitted
	NoWait,
	// Wait until the extrinsic is included in a block
	InBlock,
	// Wait until the extrinsic is in a finalized block
	#[default]
	Finalized,
}

#[derive(Debug)]
pub enum WaitForResult {
	// The hash of the SC transaction that was submitted.
	TransactionHash(H256),
	Details(ExtrinsicDetails),
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
		dispatch_error: sp_runtime::DispatchError,
	) -> DispatchError {
		self.error_decoder.decode_dispatch_error(dispatch_error)
	}
}

/// Common macro to extract dynamic events
#[macro_export]
macro_rules! extract_dynamic_event {
    ($dynamic_events:expr, $cf_static_event_variant:path, { $($field:ident),* }, $result:expr) => {

		match $dynamic_events
			.find_static_event::<$cf_static_event_variant>(true)?
		{
			Some($cf_static_event_variant { $($field),*, .. } ) => Ok($result),
			None => Err($crate::events_decoder::DynamicEventError::StaticEventNotFound(stringify!($cf_static_event_variant)))
		}
    };
}
