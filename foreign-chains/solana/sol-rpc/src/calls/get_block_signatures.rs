use jsonrpsee::rpc_params;
use serde_json::json;
use sol_prim::{Digest, Signature, SlotNumber};

use crate::{traits::Call, types::Commitment};

use super::GetBlockSignatures;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockSignatures {
	#[serde(rename = "blockHeight")]
	pub slot: SlotNumber,
	#[serde(rename = "blockhash")]
	pub hash: Digest,

	#[serde(rename = "parentSlot")]
	pub parent_slot: Option<SlotNumber>,
	#[serde(rename = "previousBlockhash")]
	pub parent_hash: Option<Digest>,

	pub signatures: Vec<Signature>,
}

impl GetBlockSignatures {
	pub fn at(slot_number: SlotNumber) -> Self {
		Self { slot_number, commitment: Default::default() }
	}
	pub fn commitment(self, commitment: Commitment) -> Self {
		Self { commitment, ..self }
	}
}

impl Call for GetBlockSignatures {
	type Response = BlockSignatures;
	const CALL_METHOD_NAME: &'static str = "getBlock";

	fn call_params(&self) -> jsonrpsee::core::params::ArrayParams {
		rpc_params![
			self.slot_number,
			json!({
				"commitment": self.commitment,
				"transactionDetails": "signatures"
			})
		]
	}
}
