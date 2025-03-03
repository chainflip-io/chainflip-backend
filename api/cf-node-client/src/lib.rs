use crate::events_decoder::DynamicEvents;
use frame_support::dispatch::DispatchInfo;
use sp_core::{
	serde::{Deserialize, Serialize},
	H256,
};

pub mod error_decoder;
pub mod events_decoder;
pub mod runtime_decoder;
pub mod signer;
pub mod subxt_state_chain_config;

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

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum ApiWaitForResult<T> {
	TxHash(H256),
	TxDetails { tx_hash: H256, response: T },
}

impl<T> ApiWaitForResult<T> {
	pub fn map_details<R>(self, f: impl FnOnce(T) -> R) -> ApiWaitForResult<R> {
		match self {
			ApiWaitForResult::TxHash(hash) => ApiWaitForResult::TxHash(hash),
			ApiWaitForResult::TxDetails { response, tx_hash } =>
				ApiWaitForResult::TxDetails { tx_hash, response: f(response) },
		}
	}

	#[track_caller]
	pub fn unwrap_details(self) -> T {
		match self {
			ApiWaitForResult::TxHash(_) => panic!("unwrap_details called on TransactionHash"),
			ApiWaitForResult::TxDetails { response, .. } => response,
		}
	}
}

#[derive(Debug)]
pub enum WaitForDynamicResult {
	// The hash of the SC transaction that was submitted.
	TransactionHash(H256),
	Data(ExtrinsicData),
}
