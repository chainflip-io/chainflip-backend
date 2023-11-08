use sp_core::H256;

use super::client::BlockInfo;

pub fn test_header(number: u32, parent_hash: Option<H256>) -> BlockInfo {
	BlockInfo {
		number,
		parent_hash: parent_hash.unwrap_or_default(),
		hash: H256::from_low_u64_le(number.into()),
	}
}
