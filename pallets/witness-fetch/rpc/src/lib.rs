use chainflip_common::types::utf8::ByteString;
use jsonrpc_core::{Error as RpcError, ErrorCode, Result};
use jsonrpc_derive::rpc;
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_runtime::{generic::BlockId, traits::Block as BlockT};
use std::sync::Arc;
use witness_fetch_runtime_api::WitnessApi as WitnessRuntimeApi;

#[rpc]
pub trait WitnessApi<BlockHash> {
    #[rpc(name = "get_confirmed_witnesses")]
    fn get_confirmed_witnesses(&self, at: Option<BlockHash>) -> Result<Vec<ByteString>>;
}

pub struct Witness<C, M> {
    client: Arc<C>,
    _marker: std::marker::PhantomData<M>,
}

impl<C, M> Witness<C, M> {
    pub fn new(client: Arc<C>) -> Self {
        Self {
            client,
            _marker: Default::default(),
        }
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
    fn get_confirmed_witnesses(
        &self,
        at: Option<<Block as BlockT>::Hash>,
    ) -> Result<Vec<ByteString>> {
        let api = self.client.runtime_api();
        let at = BlockId::hash(at.unwrap_or_else(|| self.client.info().best_hash));

        let runtime_api_result = api.get_confirmed_witnesses(&at);

        match runtime_api_result {
            Ok(result) => {
                let witnesses: Vec<ByteString> = result.iter().map(|w| w.clone().into()).collect();
                return Ok(witnesses);
            }
            Err(err) => {
                return Err(RpcError {
                    code: ErrorCode::ServerError(9999),
                    message: "Could not fetch witnesses".into(),
                    data: Some(format!("{:#?}", err).into()),
                });
            }
        }
    }
}
