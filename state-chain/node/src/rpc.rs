//! A collection of node-specific RPC methods.
//! Substrate provides the `sc-rpc` crate, which defines the core RPC layer
//! used by Substrate nodes. This file extends those RPC definitions with
//! capabilities that are specific to this project's runtime configuration.

#![warn(missing_docs)]

use std::sync::Arc;

use cf_p2p::MessagingCommand;
use futures::channel::mpsc::UnboundedSender;
use sp_block_builder::BlockBuilder;
pub use sc_rpc_api::DenyUnsafe;

/// Instantiate all full RPC extensions.
pub fn create_full(
	rpc_command_sender: Arc<UnboundedSender<MessagingCommand>>,
	params: Arc<cf_p2p_rpc::RpcCore>,
) -> jsonrpc_core::IoHandler<sc_rpc::Metadata> {
	let mut io = jsonrpc_core::IoHandler::default();
	io.extend_with(cf_p2p_rpc::RpcApi::to_delegate(
		cf_p2p_rpc::Rpc::new(rpc_command_sender, params)
	));
	io
}
