pub trait ChainApi {
	fn latest_finalized_hash(&self) -> state_chain_runtime::Hash;
	fn latest_unfinalized_hash(&self) -> state_chain_runtime::Hash;
}
