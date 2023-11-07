use sp_core::H256;
use sp_runtime::Digest;
use state_chain_runtime::Header;

pub fn test_header(number: u32, parent_hash: Option<H256>) -> Header {
	Header {
		number,
		parent_hash: parent_hash.unwrap_or_default(),
		state_root: H256::default(),
		extrinsics_root: H256::default(),
		digest: Digest { logs: Vec::new() },
	}
}
