use crate::types::Blockhash;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LatestBlockhash {
	pub blockhash: Blockhash,
	pub last_valid_block_height: u64,
}
