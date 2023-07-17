use subxt::events::StaticEvent;

use tracing::error;

use std::sync::Arc;

use utilities::task_scope::Scope;

use crate::{
	dot::{
		http_rpc::DotHttpRpcClient,
		retry_rpc::DotRetryRpcClient,
		rpc::DotSubClient,
		witnesser::{EventWrapper, ProxyAdded, TransactionFeePaid, Transfer},
	},
	settings::{self},
	state_chain_observer::client::{
		extrinsic_api::signed::SignedExtrinsicApi, storage_api::StorageApi,
	},
	witness::{
		chain_source::{dot_source::DotUnfinalisedSource, extension::ChainSourceExt},
		epoch_source::EpochSource,
	},
};

use anyhow::Result;

pub async fn start<StateChainClient>(
	scope: &Scope<'_, anyhow::Error>,
	settings: &settings::Dot,
	state_chain_client: Arc<StateChainClient>,
	epoch_source: EpochSource<'_, '_, StateChainClient, (), ()>,
) -> Result<()>
where
	StateChainClient: StorageApi + SignedExtrinsicApi + 'static + Send + Sync,
{
	let dot_client = DotRetryRpcClient::new(
		scope,
		DotHttpRpcClient::new(&settings.http_node_endpoint).await?,
		DotSubClient::new(&settings.ws_node_endpoint),
	);

	let dot_chain_tracking = DotUnfinalisedSource::new(dot_client.clone())
		.then(|header| async move {
			header
				.data
				.iter()
				.filter_map(|event_details| match event_details {
					Ok(event_details) =>
						match (event_details.pallet_name(), event_details.variant_name()) {
							(ProxyAdded::PALLET, ProxyAdded::EVENT) =>
								Some(EventWrapper::ProxyAdded(
									event_details.as_event::<ProxyAdded>().unwrap().unwrap(),
								)),
							(Transfer::PALLET, Transfer::EVENT) => Some(EventWrapper::Transfer(
								event_details.as_event::<Transfer>().unwrap().unwrap(),
							)),
							(TransactionFeePaid::PALLET, TransactionFeePaid::EVENT) =>
								Some(EventWrapper::TransactionFeePaid(
									event_details
										.as_event::<TransactionFeePaid>()
										.unwrap()
										.unwrap(),
								)),
							_ => None,
						}
						.map(|event| (event_details.phase(), event)),
					Err(err) => {
						error!("Error while parsing event: {:?}", err);
						None
					},
				})
				.collect()
		})
		.chunk_by_time(epoch_source)
		.await
		.chain_tracking(state_chain_client, dot_client)
		.run();

	scope.spawn(async move {
		dot_chain_tracking.await;
		Ok(())
	});
	Ok(())
}
