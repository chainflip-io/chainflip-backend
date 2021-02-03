use jsonrpc_core::{Error as RpcError, ErrorCode, Result};
use jsonrpc_derive::rpc;
use sp_api::ProvideRuntimeApi;
use sp_blockchain::HeaderBackend;
use sp_runtime::{generic::BlockId, traits::Block as BlockT};
use std::sync::Arc;
use witness_fetch_runtime_api::WitnessApi as WitnessRuntimeApi;

#[rpc]
pub trait WitnessApi<BlockHash> {
    #[rpc(name = "get_valid_witnesses")]
    fn get_valid_witnesses(&self, at: Option<BlockHash>) -> Result<Vec<String>>;
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
    fn get_valid_witnesses(&self, at: Option<<Block as BlockT>::Hash>) -> Result<Vec<String>> {
        let api = self.client.runtime_api();
        let at = BlockId::hash(at.unwrap_or_else(|| 
        self.client.info().best_hash));

        let runtime_api_result = api.get_valid_witnesses(&at);

        match runtime_api_result {
            Ok(result) => {
                let witnesses = byte_witnesses_to_string_witnesses(result).map_err(|e| {
                    RpcError {
                        code: ErrorCode::ServerError(9998),
                        message: "Failed to parse witnesses".into(),
                        data: Some(format!("{:#?}", e).into())
                    }
                })?;
                return Ok(witnesses);
            },
            Err(err) => {
                return Err(RpcError {
                    code: ErrorCode::ServerError(9999),
                    message: "Could not fetch witnesses".into(),
                    data: Some(format!("{:#?}", err).into())
                });
            }
        }
    }
}

fn byte_witnesses_to_string_witnesses(byte_witnesses: Vec<Vec<u8>>) -> std::result::Result<Vec<String>, String> {
    let mut string_witnesses: Vec<String> = Vec::new();
    for b_witness in byte_witnesses {
        let utf8_witness = String::from_utf8(b_witness).map_err(|e| e.to_string())?;
        string_witnesses.push(utf8_witness);
    }
    Ok(string_witnesses)
}