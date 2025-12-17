// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

pub use cf_rpc_types::*;
use jsonrpsee::{
	tracing::log,
	types::{error::ErrorObjectOwned, ErrorCode, ErrorObject},
};
use serde::{Deserialize, Serialize};

pub mod broker;
pub mod lp;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum NotificationBehaviour {
	/// Subscription will return finalized blocks.
	Finalized,
	/// Subscription will return best blocks. In the case of a re-org it might drop events.
	#[default]
	Best,
	/// Subscription will return all new blocks. In the case of a re-org it might duplicate events.
	///
	/// The caller is responsible for de-duplicating events.
	New,
}

#[derive(thiserror::Error, Debug)]
pub enum RpcApiError {
	#[error(transparent)]
	ErrorObject(#[from] ErrorObjectOwned),
	#[error(transparent)]
	ClientError(#[from] jsonrpsee::core::ClientError),
	#[error(transparent)]
	Other(#[from] anyhow::Error),
}

/// Defines all possible error codes returned by the Chainflip RPC API.
/// According to the JSON-RPC 2.0 specification, the error code must be an integer in the range
/// of -32000 to -32099, which map to `ErrorCode::ServerError`.
/// Start from 32020 because some of the 32000 to 32019 are used by the jsonrpsee library.
#[repr(i32)]
pub enum CfErrorCode {
	OtherError = -32020,
	DispatchError = -32021,
	RuntimeApiError = -32022,
	SubstrateClientError = -32023,
	PoolClientError = -32024,
	DynamicEventsError = -32025,
	UnsupportedRuntimeApiVersion = -32026,
}

pub type RpcResult<T> = Result<T, RpcApiError>;

pub fn internal_error(error: impl core::fmt::Debug) -> ErrorObjectOwned {
	log::error!(target: "cf_rpc", "Internal error: {:?}", error);
	ErrorObject::owned(
		ErrorCode::InternalError.code(),
		"Internal error while processing request.",
		None::<()>,
	)
}
pub fn call_error(
	error: impl Into<Box<dyn core::error::Error + Sync + Send>>,
	err_code: CfErrorCode,
) -> ErrorObjectOwned {
	let error = error.into();
	log::debug!(target: "cf_rpc", "Call error: {}", error);
	ErrorObject::owned(
		ErrorCode::ServerError(err_code as i32).code(),
		format!("{error}"),
		None::<()>,
	)
}

impl From<RpcApiError> for ErrorObjectOwned {
	fn from(error: RpcApiError) -> Self {
		match error {
			RpcApiError::ClientError(client_error) => match client_error {
				jsonrpsee::core::client::Error::Call(obj) => obj,
				other => internal_error(other),
			},
			RpcApiError::ErrorObject(object) => object,
			RpcApiError::Other(error) => call_error(error, CfErrorCode::OtherError),
		}
	}
}
