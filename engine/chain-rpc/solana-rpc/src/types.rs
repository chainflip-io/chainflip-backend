use std::collections::HashMap;

use serde::{Deserialize, Serialize};

mod commitment;
mod serde_b58_hash;

pub const BLOCK_HASH_LEN: usize = 32;
pub const ACCOUNT_ADDRESS_LEN: usize = 32;

pub type JsValue = serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Commitment {
	Processed = 1,
	Confirmed = 2,
	Finalized = 3,
}

impl Default for Commitment {
	fn default() -> Self {
		Self::Finalized
	}
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ResponseContext {
	pub slot: u64,

	#[serde(flatten)]
	pub extra: HashMap<String, JsValue>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Response<Value> {
	pub context: ResponseContext,
	pub value: Value,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Blockhash(#[serde(with = "serde_b58_hash")] [u8; BLOCK_HASH_LEN]);

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrioritizationFeeRecord {
	prioritization_fee: u64,
	slot: u64,
}
