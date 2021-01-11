use crate::{
    common::api::ResponseError, quoter::vault_node::VaultNodeInterface,
    vault::api::v1::post_withdraw::WithdrawParams,
};
use std::sync::Arc;
use warp::http::StatusCode;

/// Submit a withdraw request
pub async fn withdraw<V: VaultNodeInterface>(
    params: WithdrawParams,
    vault_node: Arc<V>,
) -> Result<serde_json::Value, ResponseError> {
    // Nothing to do on the quoter, just proxy the request
    vault_node
        .submit_withdraw(params)
        .await
        .map_err(|err| ResponseError::new(StatusCode::BAD_REQUEST, &err))
}
