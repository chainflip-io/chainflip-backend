use jsonrpc_core::{Error as RpcError, ErrorCode, Result};
use jsonrpc_derive::rpc;
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_runtime::{generic::BlockId, traits::Block as BlockT};
use std::sync::Arc;
use transactions_runtime_api::WitnessApi as WitnessRuntimeApi;

#[rpc]
pub trait WitnessApi<BlockHash> {
    #[rpc(name = "get_valid_witnesses")]
    fn get_valid_witnesses(&self, at: Option<BlockHash>) -> Result<Vec<Vec<u8>>>;
}


pub struct Witness<C, M> {
    client: Arc<C>,
    _marker: std::marker::PhantomData<M>,
}

impl<C, M> Witness<C, M> {
    pub fn new(client: Arc<C>) -> Self {
        Self { client, _marker: Default::default() }
    }
}

impl<C, Block> WitnessApi<<Block as BlockT>::Hash> for Witness<C, Block>
    where
        Block: BlockT,
        C: Send + Sync + 'static,
        C: ProvideRuntimeApi<Block>,
        C: HeaderBackend<Block>,
        C::Api: WitnessRuntimeApi<Block>,
{
    fn get_valid_witnesses(&self, at: Option<<Block as BlockT>::Hash>) -> Result<Vec<Vec<u8>>> {
        let api = self.client.runtime_api();
        let at = BlockId::hash(at.unwrap_or_else(|| 
        self.client.info().best_hash));

        frame_support::debug::info!("get witnesses at: {:#}", at);

        let runtime_api_result = api.get_valid_witnesses(&at);

        frame_support::debug::info!("runtime api result: {:#?}", runtime_api_result);
        runtime_api_result.map_err(|e| RpcError {
            code: ErrorCode::ServerError(9876),
            message: "Something wrong".into(),
            data: Some(format!("{:#?}", e).into()),
        })
    }
}