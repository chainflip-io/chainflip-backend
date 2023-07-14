use cf_chains::dot::{PolkadotHash, PolkadotTrackedData};
use subxt::events::{Phase, StaticEvent};

use tracing::error;

use std::sync::Arc;

use utilities::task_scope::task_scope;

use crate::{
	dot::{
		http_rpc::DotHttpRpcClient,
		retry_rpc::{DotRetryRpcApi, DotRetryRpcClient},
		rpc::DotSubClient,
		witnesser::{EventWrapper, ProxyAdded, TransactionFeePaid, Transfer},
	},
	settings::Settings,
	state_chain_observer::client::{
		extrinsic_api::signed::SignedExtrinsicApi, storage_api::StorageApi, StateChainStreamApi,
	},
	witness::{
		chain_source::{dot_source::DotUnfinalisedSource, extension::ChainSourceExt},
		epoch_source::EpochSource,
	},
};

use futures::FutureExt;

use super::{
	chain_source::Header, chunked_chain_source::chunked_by_time::chain_tracking::GetTrackedData,
};

#[async_trait::async_trait]
impl<T: DotRetryRpcApi + Send + Sync + Clone>
	GetTrackedData<cf_chains::Polkadot, PolkadotHash, Vec<(Phase, EventWrapper)>> for T
{
	async fn get_tracked_data(
		&self,
		header: &Header<
			<cf_chains::Polkadot as cf_chains::Chain>::ChainBlockNumber,
			PolkadotHash,
			Vec<(Phase, EventWrapper)>,
		>,
	) -> Result<<cf_chains::Polkadot as cf_chains::Chain>::TrackedData, anyhow::Error> {
		let mut tips = Vec::new();
		for (phase, wrapped_event) in header.data.iter() {
			if let Phase::ApplyExtrinsic(_) = phase {
				if let EventWrapper::TransactionFeePaid(TransactionFeePaid { tip, .. }) =
					wrapped_event
				{
					tips.push(*tip);
				}
			}
		}

		Ok(PolkadotTrackedData {
			median_tip: {
				tips.sort();
				tips.get({
					let len = tips.len();
					if len % 2 == 0 {
						(len / 2).saturating_sub(1)
					} else {
						len / 2
					}
				})
				.cloned()
				.unwrap_or_default()
			},
		})
	}
}

pub async fn dot_chain_tracking_test<StateChainClient, StateChainStream>(
	settings: Settings,
	state_chain_client: Arc<StateChainClient>,
	state_chain_stream: StateChainStream,
) where
	StateChainStream: StateChainStreamApi,
	StateChainClient: StorageApi + SignedExtrinsicApi + 'static + Send + Sync,
{
	let _ = task_scope(|scope| {
		async {
			let dot_client = DotRetryRpcClient::new(
				scope,
				DotHttpRpcClient::new(&settings.dot.http_node_endpoint).await.unwrap(),
				DotSubClient::new(&settings.dot.ws_node_endpoint).await.unwrap(),
			);

			let epoch_source =
				EpochSource::new(scope, state_chain_stream, state_chain_client.clone())
					.await
					.participating(state_chain_client.account_id())
					.await;

			DotUnfinalisedSource::new(dot_client.clone())
				.shared(scope)
				.then(|header| async move {
					header
						.data
						.iter()
						.filter_map(|event_details| match event_details {
							Ok(event_details) =>
								match (event_details.pallet_name(), event_details.variant_name()) {
									(ProxyAdded::PALLET, ProxyAdded::EVENT) =>
										Some(EventWrapper::ProxyAdded(
											event_details
												.as_event::<ProxyAdded>()
												.unwrap()
												.unwrap(),
										)),
									(Transfer::PALLET, Transfer::EVENT) =>
										Some(EventWrapper::Transfer(
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
				.run()
				.await;

			Ok(())
		}
		.boxed()
	})
	.await;
}
