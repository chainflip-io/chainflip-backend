use frame_support::dispatch::DispatchInfo;
use sp_core::{
	serde::{Deserialize, Serialize},
	H256,
};

pub mod error_decoder;
pub mod signer;

pub type ExtrinsicDetails =
	(H256, Vec<state_chain_runtime::RuntimeEvent>, state_chain_runtime::Header, DispatchInfo);

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
