use crate::{
    common::api::ResponseError,
    quoter::vault_node::{UnstakeParams, VaultNodeInterface},
};
use std::sync::Arc;
use warp::http::StatusCode;

/// Submit a stake quoter
pub async fn unstake<V: VaultNodeInterface>(
    params: UnstakeParams,
    vault_node: Arc<V>,
) -> Result<serde_json::Value, ResponseError> {
    // Nothing to do on the quoter, just proxy the request
    vault_node
        .submit_unstake(params)
        .await
        .map_err(|err| ResponseError::new(StatusCode::BAD_REQUEST, &err))
}
