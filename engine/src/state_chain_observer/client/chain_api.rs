pub trait ChainApi {
	fn latest_finalized_block(&self) -> super::BlockInfo;
	fn latest_unfinalized_block(&self) -> super::BlockInfo;
}
