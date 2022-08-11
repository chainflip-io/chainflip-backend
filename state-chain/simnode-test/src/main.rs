use sc_consensus_manual_seal::consensus::{
	aura::AuraConsensusDataProvider, timestamp::SlotTimestampProvider,
};
use sc_service::TFullBackend;
use std::sync::Arc;
use substrate_simnode::{
	ChainInfo, FullClientFor, RpcHandlerArgs, SignatureVerificationOverride, SimnodeCli,
};

pub struct NodeTemplateChainInfo;

impl ChainInfo for NodeTemplateChainInfo {
	// Opaque block type!
	type Block = node_primitives::Block;
	type ExecutorDispatch = ExecutorDispatch;
	type Runtime = state_chain_runtime::Runtime;
	type RuntimeApi = state_chain_runtime::RuntimeApi;
	type SelectChain = sc_consensus::LongestChain<TFullBackend<Self::Block>, Self::Block>;
	type BlockImport =
		BlockImport<Self::Block, TFullBackend<Self::Block>, FullClientFor<Self>, Self::SelectChain>;
	// Inherent providers
	type InherentDataProviders = (
		//  Here we use [`SlotTimestampProvider`] to provide us with the next timestamp,
		// based on the runtime configured minimum duration between blocks and the current
		// slot number.
		SlotTimestampProvider,
		// Babe uses the timestamp from [`SlotTimestampProvider`] to calculate the current
		// slot number.
		sp_consensus_aura::inherents::InherentDataProvider,
	);
	// Pass your Cli impl here
	type Cli = ChainflipCli;
	fn create_rpc_io_handler<SC>(
		_deps: RpcHandlerArgs<Self, SC>,
	) -> jsonrpc_core::MetaIoHandler<sc_rpc::Metadata> {
		Default::default()
	}
}

pub struct ChainflipCli;

impl SimnodeCli for ChainflipCli {
	type CliConfig = sc_cli::RunCmd;
	type SubstrateCli = polkadot_cli::Cli;

	fn cli_config(cli: &Self::SubstrateCli) -> &Self::CliConfig {
		&cli.run.base
	}

	fn log_filters(cli_config: &Self::CliConfig) -> Result<String, sc_cli::Error> {
		cli_config.log_filters()
	}
}

pub struct ExecutorDispatch;

impl sc_executor::NativeExecutionDispatch for ExecutorDispatch {
	type ExtendHostFunctions = (
		frame_benchmarking::benchmarking::HostFunctions,
		// This allows [`Node::<T>::submit_extrinsic`] work by disabling
		// runtime signature verification.
		SignatureVerificationOverride,
	);

	fn dispatch(method: &str, data: &[u8]) -> Option<Vec<u8>> {
		state_chain_runtime::api::dispatch(method, data)
	}

	fn native_version() -> sc_executor::NativeVersion {
		state_chain_runtime::native_version()
	}
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
	// substrate_simnode::standalone_node::<NodeTemplateChainInfo, _, _, _>(
	// 	|client, select_chain, keystore, _| {
	// 		// set up the babe -> grandpa import pipeline
	// 		let (grandpa_block_import, ..) = grandpa::block_import(
	// 			client.clone(),
	// 			&(client.clone() as Arc<_>),
	// 			select_chain.clone(),
	// 			None,
	// 		)?;

	// 		let slot_duration = sc_consensus_aura::Config::get(&*client)?;
	// 		let (block_import, babe_link) = sc_consensus_aura::block_import(
	// 			slot_duration.clone(),
	// 			grandpa_block_import,
	// 			client.clone(),
	// 		)?;

	// 		// set up our standalone runtime's consensus data provider, used by manual seal
	// 		// to include any digests expected by the consensus pallets of your runtime
	// 		// in order to author blocks that are valid for your runtime.
	// 		// let consensus_data_provider = AuraConsensusDataProvider::new(
	// 		// 	client.clone(),
	// 		// 	keystore.sync_keystore(),
	// 		// 	babe_link.epoch_changes().clone(),
	// 		// 	vec![(AuthorityId::from(Alice.public()), 1000)],
	// 		// )
	// 		// .expect("failed to create ConsensusDataProvider");

	// 		let consensus_data_provider = AuraConsensusDataProvider::new(client.clone())
	// 			.expect("failed to create ConsensusDataProvider");

	// 		let create_inherent_data_providers = {
	// 			let cloned_client = client.clone();

	// 			Box::new(move |_, _| {
	// 				let client = cloned_client.clone();
	// 				async move {
	// 					// inherents that our runtime expects.
	// 					let timestamp = SlotTimestampProvider::new_babe(client.clone())
	// 						.map_err(|err| format!("{:?}", err))?;
	// 					let aura = sp_consensus_aura::inherents::InherentDataProvider::new(
	// 						timestamp.slot().into(),
	// 					);
	// 					Ok((timestamp, aura))
	// 				}
	// 			})
	// 		};

	// 		Ok((
	// 			block_import,
	// 			Some(Box::new(consensus_data_provider)),
	// 			create_inherent_data_providers,
	// 		))
	// 	},
	// 	/* here we'll get the node
	// 	 * |node| async move {
	// 	 * // seals blocks
	// 	 * node.seal_blocks(1).await;
	// 	 * // submit extrinsics
	// 	 * let alice = MultiSigner::from(Alice.public()).into_account();
	// 	 * let _hash = node
	// 	 * .submit_extrinsic(
	// 	 * frame_system::Call::remark_with_event { remark: (b"hello world").to_vec() },
	// 	 * alice,
	// 	 * )
	// 	 * .await
	// 	 * .unwrap(); */
	// 	/* 	// look ma, I can read state.
	// 	 * let _events =
	// 	 * node.with_state(None, || frame_system::Pallet::<node_runtime::Runtime>::events()); */
	// 	/* 	println!("{:#?}", _events);
	// 	 * // get access to the underlying client.
	// 	 * let _client = node.client(); */
	// 	/* 	node.until_shutdown().await; */
	// 	/* 	Ok(())
	// 	 * }, */
	// )
	Ok(())
}
