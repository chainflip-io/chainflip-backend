mod chain_tracking;
mod source;

use std::{collections::HashMap, sync::Arc};

use cf_chains::Solana;
use cf_primitives::EpochIndex;
use futures_core::Future;
use sp_core::H160;
use utilities::task_scope::Scope;

use crate::{
	db::PersistentKeyDB,
	sol::retry_rpc::SolRetryRpcClient,
	state_chain_observer::client::{
		chain_api::ChainApi,
		extrinsic_api::signed::SignedExtrinsicApi,
		storage_api::StorageApi,
		stream_api::{StreamApi, FINALIZED},
		STATE_CHAIN_CONNECTION,
	},
};

use super::{
	common::{chain_source::extension::ChainSourceExt, epoch_source::EpochSourceBuilder},
	sol::source::SolSource,
};

use anyhow::{Context, Result};

use chainflip_node::chain_spec::berghain::SOLANA_SAFETY_MARGIN;

pub async fn start<StateChainClient, StateChainStream, ProcessCall, ProcessingFut>(
	scope: &Scope<'_, anyhow::Error>,
	sol_client: SolRetryRpcClient,
	process_call: ProcessCall,
	state_chain_client: Arc<StateChainClient>,
	state_chain_stream: StateChainStream,
	epoch_source: EpochSourceBuilder<'_, '_, StateChainClient, (), ()>,
	db: Arc<PersistentKeyDB>,
) -> Result<()>
where
	StateChainClient: StorageApi + ChainApi + SignedExtrinsicApi + 'static + Send + Sync,
	StateChainStream: StreamApi<FINALIZED> + Clone,
	ProcessCall: Fn(state_chain_runtime::RuntimeCall, EpochIndex) -> ProcessingFut
		+ Send
		+ Sync
		+ Clone
		+ 'static,
	ProcessingFut: Future<Output = ()> + Send + 'static,
{
	let sol_source = SolSource::new(sol_client.clone()).strictly_monotonic().shared(scope);

	sol_source
		.clone()
		.chunk_by_time(epoch_source.clone(), scope)
		.chain_tracking(state_chain_client.clone(), sol_client.clone())
		.logging("chain tracking")
		.spawn(scope);

	Ok(())
}

// #[cfg(test)]
// mod tests {

// 	use std::path::PathBuf;

// 	use cf_chains::{Solana, Chain};
// 	use cf_primitives::AccountRole;

// 	use crate::{
// 		settings::{NodeContainer, WsHttpEndpoints},
// 		state_chain_observer,
// 		witness::common::epoch_source::EpochSource,
// 	};

// 	use futures::FutureExt;
// 	use utilities::{
// 		logging::LoggingSettings, task_scope::task_scope,
// 		testing::new_temp_directory_with_nonexistent_file,
// 	};

// 	use super::*;

// 	#[ignore = "requires a running localnet"]
// 	#[tokio::test]
// 	async fn run_arb_witnessing() {
// 		let _start_logger_server_fn = Some(
// 			utilities::logging::init_json_logger(LoggingSettings {
// 				span_lifecycle: false,
// 				command_server_port: 6666,
// 			})
// 			.await,
// 		);

// 		task_scope(|scope| {
// 			async move {
// 				let (state_chain_stream, _unfinalised_state_chain_stream, state_chain_client) =
// 					state_chain_observer::client::StateChainClient::connect_with_account(
// 						scope,
// 						"ws://localhost:9944",
// 						PathBuf::from("/Users/kylezs/Documents/cf-repos/chainflip-backend/localnet/init/keys/bashful/
// signing_key_file").as_path(), 						AccountRole::Validator,
// 						false,
// 						false,
// 						None,
// 					)
// 					.await.unwrap();

// 				let witness_call = {
// 					move |call, epoch_index| async move {
// 						println!("Witnessing epoch index {epoch_index} call: {call:?}");
// 					}
// 				};

// 				let epoch_source =
// 					EpochSource::builder(scope, state_chain_stream.clone(), state_chain_client.clone())
// 						.await
// 						.participating(state_chain_client.account_id())
// 						.await;

// 				let sol_client = {
// 					let expected_arb_chain_id = web3::types::U256::from(
// 						state_chain_client
// 							.storage_value::<pallet_cf_environment::SolanaChainId<state_chain_runtime::Runtime>>(
// 								state_chain_client.latest_finalized_block().hash,
// 							)
// 							.await
// 							.expect(STATE_CHAIN_CONNECTION),
// 					);

// 					SolRetryRpcClient::<SolRpcSigningClient>::new(
// 						scope,
// 						PathBuf::from("/Users/kylezs/Documents/cf-repos/chainflip-backend/localnet/init/keys/bashful/
// eth_private_key_file"), 						NodeContainer { primary: WsHttpEndpoints { ws_endpoint: "ws://localhost:8548".into(), http_endpoint: "http://localhost:8547".into()}, backup: None },
// 						expected_arb_chain_id,
// 						"arb_rpc",
// 						"arb_subscribe",
// 						"Solana",
// 						Solana::WITNESS_PERIOD,
// 					).unwrap()
// 				};

// 				let (_dir, db_path) = new_temp_directory_with_nonexistent_file();
// 				let db = Arc::new(PersistentKeyDB::open_and_migrate_to_latest(&db_path, None).unwrap());

// 				start(scope, sol_client, witness_call, state_chain_client, state_chain_stream, epoch_source,
// db).await.unwrap();

// 				Ok(())
// 			}
// 			.boxed()
// 		})
// 		.await.unwrap();
// 	}
// }
