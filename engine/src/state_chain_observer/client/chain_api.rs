use async_trait::async_trait;

use super::StateChainStreamApi;

#[async_trait]
pub trait ChainApi {
	fn latest_finalized_block(&self) -> super::BlockInfo;
	fn latest_unfinalized_block(&self) -> super::BlockInfo;

	async fn finalized_block_stream(&self) -> Box<dyn StateChainStreamApi>;
	async fn unfinalized_block_stream(&self) -> Box<dyn StateChainStreamApi<false>>;
}
