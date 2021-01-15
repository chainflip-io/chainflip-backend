//! A collection of node-specific RPC methods.
//! Substrate provides the `sc-rpc` crate, which defines the core RPC layer
//! used by Substrate nodes. This file extends those RPC definitions with
//! capabilities that are specific to this project's runtime configuration.

#![warn(missing_docs)]

use std::sync::Arc;

use state_chain_runtime::{opaque::Block};
use sp_api::ProvideRuntimeApi;
use sp_blockchain::{Error as BlockChainError, HeaderMetadata, HeaderBackend};
use sp_block_builder::BlockBuilder;
pub use sc_rpc_api::DenyUnsafe;
use sp_transaction_pool::TransactionPool;


/// Full client dependencies.
pub struct FullDeps<C, P> {
	/// The client instance to use.
	pub client: Arc<C>,
	/// Transaction pool instance.
	pub pool: Arc<P>,
	/// Whether to deny unsafe calls
	pub deny_unsafe: DenyUnsafe,
}

/// Instantiate all full RPC extensions.
pub fn create_full<C, P>(
	deps: FullDeps<C, P>,
) -> jsonrpc_core::IoHandler<sc_rpc::Metadata> where
	C: ProvideRuntimeApi<Block>,
	C: HeaderBackend<Block> + HeaderMetadata<Block, Error=BlockChainError> + 'static,
	C: Send + Sync + 'static,
	C::Api: BlockBuilder<Block>,
	C::Api: transactions_runtime_api::WitnessApi<Block>,
	P: TransactionPool + 'static,
{

	let mut io = jsonrpc_core::IoHandler::default();
	let FullDeps {
		client,
	 	pool: _,
	 	deny_unsafe: _,
	} = deps;
	
	// RPC for getting witness transactions from the state chain
	io.extend_with(transactions_rpc::WitnessApi::to_delegate(
		transactions_rpc::Witness::new(client)
	));

	io
}
