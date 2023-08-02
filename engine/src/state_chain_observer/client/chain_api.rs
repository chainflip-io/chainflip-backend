pub trait ChainApi {
	fn latest_finalized_hash(&self) -> state_chain_runtime::Hash;
}
