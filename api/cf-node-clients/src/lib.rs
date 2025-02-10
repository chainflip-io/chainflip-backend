use crate::events_decoder::DynamicEvents;
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

#[subxt::subxt(
	runtime_path = "../../target/release/wbuild/state-chain-runtime/state_chain_runtime.wasm"
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
