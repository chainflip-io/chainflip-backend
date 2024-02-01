use async_trait::async_trait;
use jsonrpsee::core::RpcResult;

use super::stream_api::{StreamApi, FINALIZED, UNFINALIZED};

#[async_trait]
pub trait ChainApi {
	fn latest_finalized_block(&self) -> super::BlockInfo;
	fn latest_unfinalized_block(&self) -> super::BlockInfo;

	async fn finalized_block_stream(&self) -> Box<dyn StreamApi<FINALIZED>>;
	async fn unfinalized_block_stream(&self) -> Box<dyn StreamApi<UNFINALIZED>>;

	async fn block(&self, hash: state_chain_runtime::Hash) -> RpcResult<super::BlockInfo>;
}
