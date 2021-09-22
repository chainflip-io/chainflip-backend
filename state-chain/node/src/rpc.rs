//! A collection of node-specific RPC methods.
//! Substrate provides the `sc-rpc` crate, which defines the core RPC layer
//! used by Substrate nodes. This file extends those RPC definitions with
//! capabilities that are specific to this project's runtime configuration.

#![warn(missing_docs)]

use std::sync::{Arc, Mutex};

use sp_block_builder::BlockBuilder;
pub use sc_rpc_api::DenyUnsafe;

/// Instantiate all full RPC extensions.
pub fn create_full<T>(
	p2p_receiver: Arc<Mutex<T>>,
	params: Arc<cf_p2p_rpc::RpcCore>,
) -> jsonrpc_core::IoHandler<sc_rpc::Metadata> where
	T: cf_p2p::P2PMessaging + Send + Sync + 'static,
{
	let mut io = jsonrpc_core::IoHandler::default();
	io.extend_with(cf_p2p_rpc::RpcApi::to_delegate(
		cf_p2p_rpc::Rpc::new(p2p_receiver, params)
	));
	io
}
