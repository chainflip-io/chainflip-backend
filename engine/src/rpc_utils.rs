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

//! Common utilities for JSON-RPC calls across different blockchains

use reqwest::{header::CONTENT_TYPE, Client};
use serde::{Deserialize, Serialize};
use serde_json::json;
use thiserror::Error;
use tracing::warn;

// From jsonrpc crate
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RpcError {
	/// The integer identifier of the error
	pub code: i32,
	/// A string describing the error
	pub message: String,
	/// Additional data specific to the error
	pub data: Option<Box<serde_json::value::RawValue>>,
}

#[derive(Error, Debug)]
pub enum Error {
	#[error("Transport error: {0}")]
	Transport(#[from] reqwest::Error),
	#[error("JSON decode error: {0}")]
	Json(#[from] serde_json::Error),
	#[error("RPC error response: {0:?}")]
	Rpc(RpcError),
}

/// Make a simple JSON-RPC 2.0 call (used by Solana, Tron, etc.)
/// Returns a single result value
pub async fn call_rpc_raw(
	client: &Client,
	url: &str,
	method: &str,
	params: Option<serde_json::Value>,
) -> Result<serde_json::Value, Error> {
	let request_body = json!({
		"jsonrpc": "2.0",
		"id": 0,
		"method": method,
		"params": params.clone().unwrap_or_else(|| json!([]))
	});

	let response = client
		.post(url)
		.header(CONTENT_TYPE, "application/json")
		.json(&request_body)
		.send()
		.await?;

	let mut json = response.json::<serde_json::Value>().await?;

	if json.is_object() {
		if json["error"].is_object() {
			return Err(Error::Rpc(serde_json::from_value(json["error"].clone())?));
		}

		Ok(json["result"].take())
	} else {
		warn!(
			"The rpc response returned for {method:?} with params: {params:?} was not a valid json object: {json:?}"
		);
		Err(Error::Rpc(serde_json::from_value(json)?))
	}
}
