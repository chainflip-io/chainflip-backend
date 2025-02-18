use crate::{
	error_decoder::ErrorDecoder,
	events_decoder::{DynamicEventError, DynamicEvents, EventsDecoder},
};
use frame_support::dispatch::DispatchInfo;
use sp_api::runtime_decl_for_core::CoreV5;
use sp_core::{
	serde::{Deserialize, Serialize},
	H256,
};
use sp_runtime::DispatchError;
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
/// * For any other complex types with multiple hieracrchies or generics, please add manual
///   conversion functions below.
#[subxt::subxt(
	runtime_path = "../../target/release/wbuild/state-chain-runtime/state_chain_runtime.wasm",
	substitute_type(
		path = "cf_chains::address::EncodedAddress",
		with = "::subxt::utils::Static<cf_chains::address::EncodedAddress>"
	),
	substitute_type(
		path = "cf_primitives::chains::assets::any::Asset",
		with = "::subxt::utils::Static<cf_primitives::chains::assets::any::Asset>"
	),
	substitute_type(
		path = "cf_primitives::chains::ForeignChain",
		with = "::subxt::utils::Static<cf_primitives::chains::ForeignChain>"
	),
	substitute_type(
		path = "cf_amm::common::Side",
		with = "::subxt::utils::Static<cf_amm::common::Side>"
	),
	substitute_type(
		path = "cf_chains::ChannelRefundParametersGeneric<A>",
		with = "::subxt::utils::Static<cf_chains::ChannelRefundParametersGeneric<A>>"
	)
)]
pub mod cf_static_runtime {}

// substitute_type(
// path = "frame_support::dispatch::DispatchInfo",
// with = "::subxt::utils::Static<frame_support::dispatch::DispatchInfo>"
// ),

// substitute_type(
// path = "sp_runtime::DispatchError",
// with = "::subxt::utils::Static<sp_runtime::DispatchError>"
// ),

//
// substitute_type(
// path = "primitive_types::U256",
// with = "::subxt::utils::Static<sp_core::U256>"
// ),

// substitute_type(
// path = "pallet_cf_pools::pallet::IncreaseOrDecrease<A>",
// with = "::subxt::utils::Static<pallet_cf_pools::pallet::IncreaseOrDecrease<A>>"
// ),
// substitute_type(
// path = "cf_amm::common::PoolPairsMap<A>",
// with = "::subxt::utils::Static<cf_amm::common::PoolPairsMap<sp_core::U256>>"
// ),

// substitute_type(
// path = "cf_amm::limit_orders::Position",
// with = "::subxt::utils::Static<cf_amm::limit_orders::Position>"
// ),
// substitute_type(
// path = "cf_amm::common::PoolPairsMap<A>",
// with = "::subxt::utils::Static<cf_amm::common::PoolPairsMap<A>>"
// ),

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

#[derive(Debug)]
pub enum WaitForDynamicResult {
	// The hash of the SC transaction that was submitted.
	TransactionHash(H256),
	Data(ExtrinsicData),
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
			.find_static_event::<$cf_static_event_variant>(true)?
		{
			Some($cf_static_event_variant { $($field),*, .. } ) => Ok($result),
			None => Err($crate::events_decoder::DynamicEventError::StaticEventNotFound(stringify!($cf_static_event_variant)))
		}
    };
}

// ---- Conversions

impl<T> From<cf_static_runtime::runtime_types::cf_amm::common::PoolPairsMap<T>>
	for cf_amm::common::PoolPairsMap<T>
{
	fn from(value: cf_static_runtime::runtime_types::cf_amm::common::PoolPairsMap<T>) -> Self {
		Self { base: value.base, quote: value.quote }
	}
}

impl<T> From<cf_static_runtime::runtime_types::pallet_cf_pools::pallet::IncreaseOrDecrease<T>>
	for pallet_cf_pools::pallet::IncreaseOrDecrease<T>
{
	fn from(
		value: cf_static_runtime::runtime_types::pallet_cf_pools::pallet::IncreaseOrDecrease<T>,
	) -> Self {
		match value {
			cf_static_runtime::runtime_types::pallet_cf_pools::pallet::IncreaseOrDecrease::Increase(t) => pallet_cf_pools::pallet::IncreaseOrDecrease::Increase(t),
			cf_static_runtime::runtime_types::pallet_cf_pools::pallet::IncreaseOrDecrease::Decrease(t) => pallet_cf_pools::pallet::IncreaseOrDecrease::Decrease(t),
		}
	}
}

impl
	From<
		cf_static_runtime::runtime_types::pallet_cf_pools::pallet::IncreaseOrDecrease<
			cf_static_runtime::runtime_types::pallet_cf_pools::pallet::RangeOrderChange,
		>,
	> for pallet_cf_pools::pallet::IncreaseOrDecrease<pallet_cf_pools::RangeOrderChange>
{
	fn from(
		value: cf_static_runtime::runtime_types::pallet_cf_pools::pallet::IncreaseOrDecrease<
			cf_static_runtime::runtime_types::pallet_cf_pools::pallet::RangeOrderChange,
		>,
	) -> Self {
		match value {
			cf_static_runtime::runtime_types::pallet_cf_pools::pallet::IncreaseOrDecrease::Increase(t) => pallet_cf_pools::pallet::IncreaseOrDecrease::Increase(pallet_cf_pools::RangeOrderChange{
				liquidity: t.liquidity,
				amounts: t.amounts.into(),
			}),
			cf_static_runtime::runtime_types::pallet_cf_pools::pallet::IncreaseOrDecrease::Decrease(t) => pallet_cf_pools::pallet::IncreaseOrDecrease::Decrease(pallet_cf_pools::RangeOrderChange {
				liquidity: t.liquidity,
				amounts: t.amounts.into(),
			}),
		}
	}
}

// impl<T> From<cf_static_runtime::runtime_types::pallet_cf_pools::pallet::IncreaseOrDecrease<T>>
// for pallet_cf_pools::pallet::IncreaseOrDecrease<T> { 	fn from(value:
// cf_static_runtime::runtime_types::pallet_cf_pools::pallet::IncreaseOrDecrease<T>) -> Self {
// 		match value {
// 			cf_static_runtime::runtime_types::pallet_cf_pools::pallet::IncreaseOrDecrease::Increase(t) =>
// pallet_cf_pools::pallet::IncreaseOrDecrease::Increase(t),
// 			cf_static_runtime::runtime_types::pallet_cf_pools::pallet::IncreaseOrDecrease::Decrease(t) =>
// pallet_cf_pools::pallet::IncreaseOrDecrease::Decrease(t), 		}
// 	}
// }

// impl From<cf_static_runtime::runtime_types::pallet_cf_pools::pallet::RangeOrderChange> for
// pallet_cf_pools::pallet::RangeOrderChange { 	fn from(value:
// cf_static_runtime::runtime_types::pallet_cf_pools::pallet::RangeOrderChange) -> Self { 		Self {
// 			liquidity: value.liquidity,
// 			amounts: value.amounts.into(),
// 		}
// 	}
// }

//
// impl<T> cf_static_runtime::runtime_types::cf_amm::common::PoolPairsMap<T> {
// 	pub fn from_array(array: [T; 2]) -> Self {
// 		let [base, quote] = array;
// 		Self { base, quote }
// 	}
//
// 	pub fn map<R, F: FnMut(T) -> R>(self, mut f: F) ->
// cf_static_runtime::runtime_types::cf_amm::common::PoolPairsMap<R> {
// 		cf_static_runtime::runtime_types::cf_amm::common::PoolPairsMap { base: f(self.base), quote:
// f(self.quote) } 	}
// }
//
// impl<T> cf_static_runtime::runtime_types::pallet_cf_pools::pallet::IncreaseOrDecrease<T> {
// 	pub fn abs(&self) -> &T {
// 		match self {
// 			cf_static_runtime::runtime_types::pallet_cf_pools::pallet::IncreaseOrDecrease::Increase(t) =>
// t, 			cf_static_runtime::runtime_types::pallet_cf_pools::pallet::IncreaseOrDecrease::Decrease(t)
// => t, 		}
// 	}
//
// 	pub fn map<R, F: FnOnce(T) -> R>(self, f: F) ->
// cf_static_runtime::runtime_types::pallet_cf_pools::pallet::IncreaseOrDecrease<R> { 		match self {
// 			cf_static_runtime::runtime_types::pallet_cf_pools::pallet::IncreaseOrDecrease::Increase(t) =>
// cf_static_runtime::runtime_types::pallet_cf_pools::pallet::IncreaseOrDecrease::Increase(f(t)),
// 			cf_static_runtime::runtime_types::pallet_cf_pools::pallet::IncreaseOrDecrease::Decrease(t) =>
// cf_static_runtime::runtime_types::pallet_cf_pools::pallet::IncreaseOrDecrease::Decrease(f(t)),
// 		}
// 	}
//
// 	pub fn try_map<R, E, F: FnOnce(T) -> Result<R, E>>(
// 		self,
// 		f: F,
// 	) -> Result<cf_static_runtime::runtime_types::pallet_cf_pools::pallet::IncreaseOrDecrease<R>, E>
// { 		Ok(match self {
// 			cf_static_runtime::runtime_types::pallet_cf_pools::pallet::IncreaseOrDecrease::Increase(t) =>
// cf_static_runtime::runtime_types::pallet_cf_pools::pallet::IncreaseOrDecrease::Increase(f(t)?),
// 			cf_static_runtime::runtime_types::pallet_cf_pools::pallet::IncreaseOrDecrease::Decrease(t) =>
// cf_static_runtime::runtime_types::pallet_cf_pools::pallet::IncreaseOrDecrease::Decrease(f(t)?),
// 		})
// 	}
// }
