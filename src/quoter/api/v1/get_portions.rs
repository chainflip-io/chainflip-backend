use crate::{
    common::api::ResponseError, quoter::vault_node::VaultNodeInterface,
    vault::api::v1::PortionsParams,
};
use std::sync::Arc;
use warp::http::StatusCode;

/// Submit a stake quoter
pub async fn get_portions<V: VaultNodeInterface>(
    params: PortionsParams,
    vault_node: Arc<V>,
) -> Result<serde_json::Value, ResponseError> {
    // Nothing to do on the quoter, just proxy the request
    vault_node
        .get_portions(params)
        .await
        .map_err(|err| ResponseError::new(StatusCode::BAD_REQUEST, &err))
}
