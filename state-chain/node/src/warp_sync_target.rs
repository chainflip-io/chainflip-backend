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

//! Resolution of an explicit warp sync target block.
//!
//! Warp sync normally downloads GRANDPA warp proofs to establish the latest finalized authority
//! set, and then downloads the state at the tip. That means the resulting database only ever
//! contains the state of a *recent* block, which is useless if you need the state at a specific
//! (older) block, for example to run `benchmark block` against it.
//!
//! Substrate supports pointing warp sync at an explicit target header
//! ([`sc_service::WarpSyncConfig::WithTarget`]), which skips the proof download entirely. The
//! header is trusted unconditionally, so it has to come from somewhere trusted: here, the RPC
//! endpoint of an archive node operated by the same person running the node. Hence the `unsafe`
//! prefix on the CLI flag.

use cf_primitives::BlockNumber;
use jsonrpsee::{
	core::client::ClientT, http_client::HttpClientBuilder, rpc_params, ws_client::WsClientBuilder,
};
use sc_service::Configuration;
use sp_core::H256;
use sp_runtime::traits::Header as HeaderT;
use state_chain_runtime::opaque::Header;
use std::time::Duration;

/// Timeout applied to the individual requests made against the target RPC.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Resolve the header for `--unsafe-warp-sync-target-block`, if it was given.
///
/// All the failure modes here (missing sibling flag, wrong sync mode, unreachable RPC, unknown
/// block) are reported as input errors before the node is built.
pub async fn resolve(
	config: &Configuration,
	target_block: Option<BlockNumber>,
	target_rpc: Option<String>,
) -> sc_cli::Result<Option<Header>> {
	let Some(target_block) = target_block else { return Ok(None) };

	// Enforced by clap (`requires`), checked again so that this function is safe on its own.
	let target_rpc = target_rpc.ok_or_else(|| {
		sc_cli::Error::Input(
			"--unsafe-warp-sync-target-block requires --warp-sync-target-rpc.".to_string(),
		)
	})?;

	if !config.network.sync_mode.is_warp() {
		return Err(sc_cli::Error::Input(format!(
			"--unsafe-warp-sync-target-block requires `--sync warp`, but the sync mode is {:?}.",
			config.network.sync_mode
		)))
	}

	let header = resolve_target_header(&target_rpc, target_block)
		.await
		.map_err(sc_cli::Error::Input)?;

	log::warn!(
		"UNSAFE: warp syncing to block {target_block} ({:?}), as resolved by {target_rpc}. \
		This header is trusted without verification, and no GRANDPA warp proofs are downloaded. \
		Only the state at the target block is downloaded, so the resulting database is not a \
		substitute for a normally synced one.",
		header.hash(),
	);

	Ok(Some(header))
}

/// Fetch and sanity-check the header of `block_number` from the node at `rpc_url`.
///
/// The returned header is used as-is as the warp sync target, so this fails rather than falling
/// back to regular warp sync: a silently wrong target would produce a database that looks fine but
/// doesn't contain the state we asked for.
async fn resolve_target_header(rpc_url: &str, block_number: BlockNumber) -> Result<Header, String> {
	if rpc_url.starts_with("ws://") || rpc_url.starts_with("wss://") {
		let client = WsClientBuilder::default()
			.request_timeout(REQUEST_TIMEOUT)
			.build(rpc_url)
			.await
			.map_err(|e| connect_error(rpc_url, e))?;
		fetch_header(&client, block_number).await
	} else if rpc_url.starts_with("http://") || rpc_url.starts_with("https://") {
		let client = HttpClientBuilder::default()
			.request_timeout(REQUEST_TIMEOUT)
			.build(rpc_url)
			.map_err(|e| connect_error(rpc_url, e))?;
		fetch_header(&client, block_number).await
	} else {
		Err(format!("Invalid warp sync target RPC url {rpc_url}: expected ws, wss, http or https."))
	}
}

fn connect_error(rpc_url: &str, e: impl core::fmt::Display) -> String {
	format!("Unable to connect to warp sync target RPC {rpc_url}: {e}")
}

/// The `chain_getBlockHash` + `chain_getHeader` round trip, plus the checks on the result.
async fn fetch_header<C: ClientT>(client: &C, block_number: BlockNumber) -> Result<Header, String> {
	let hash: Option<H256> = client
		.request("chain_getBlockHash", rpc_params![block_number])
		.await
		.map_err(|e| format!("chain_getBlockHash({block_number}) failed: {e}"))?;
	let hash = hash.ok_or_else(|| {
		format!("The warp sync target RPC doesn't know block {block_number}. Is it synced past it?")
	})?;

	let header: Option<Header> = client
		.request("chain_getHeader", rpc_params![hash])
		.await
		.map_err(|e| format!("chain_getHeader({hash:?}) failed: {e}"))?;
	let header = header.ok_or_else(|| {
		format!("The warp sync target RPC returned no header for block {block_number} ({hash:?})")
	})?;

	// Guards against pointing at a node for a different chain, and against our `Header` type
	// disagreeing with the remote node's.
	if *header.number() != block_number {
		return Err(format!(
			"The warp sync target RPC returned a header for block {} when asked for {block_number}",
			header.number()
		))
	}
	if header.hash() != hash {
		return Err(format!(
			"The header the warp sync target RPC returned for block {block_number} hashes to {:?}, \
			 but the RPC reported the block hash as {hash:?}.",
			header.hash()
		))
	}

	Ok(header)
}
