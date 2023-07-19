use cf_chains::{dot::PolkadotAccountId, Polkadot};
use cf_primitives::{chains::assets, TxId};
use pallet_cf_ingress_egress::{DepositAddressDetails, DepositWitness};
use state_chain_runtime::PolkadotInstance;
use subxt::events::{EventDetails, Phase, StaticEvent};

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
		extrinsic_api::signed::SignedExtrinsicApi, storage_api::StorageApi, StateChainStreamApi,
	},
	witness::{
		chain_source::{dot_source::DotUnfinalisedSource, extension::ChainSourceExt},
		epoch_source::EpochSource,
	},
};

use anyhow::Result;

pub async fn start<StateChainClient, Epochs: Into<EpochSource<(), ()>>>(
use super::chain_source::dot_source::DotFinalisedSource;

fn filter_map_events(
	res_event_details: Result<EventDetails, subxt::Error>,
) -> Option<(Phase, EventWrapper)> {
	match res_event_details {
		Ok(event_details) => match (event_details.pallet_name(), event_details.variant_name()) {
			(ProxyAdded::PALLET, ProxyAdded::EVENT) => Some(EventWrapper::ProxyAdded(
				event_details.as_event::<ProxyAdded>().unwrap().unwrap(),
			)),
			(Transfer::PALLET, Transfer::EVENT) =>
				Some(EventWrapper::Transfer(event_details.as_event::<Transfer>().unwrap().unwrap())),
			(TransactionFeePaid::PALLET, TransactionFeePaid::EVENT) =>
				Some(EventWrapper::TransactionFeePaid(
					event_details.as_event::<TransactionFeePaid>().unwrap().unwrap(),
				)),
			_ => None,
		}
		.map(|event| (event_details.phase(), event)),
		Err(err) => {
			error!("Error while parsing event: {:?}", err);
			None
		},
	}
}

pub async fn start<StateChainClient, StateChainStream, Epochs: Into<EpochSource<(), ()>>>(
	scope: &Scope<'_, anyhow::Error>,
	settings: &settings::Dot,
	state_chain_client: Arc<StateChainClient>,
    state_chain_stream: StateChainStream,
	epoch_source: Epochs,
) -> Result<()>
where
	StateChainClient: StorageApi + SignedExtrinsicApi + 'static + Send + Sync,
	StateChainStream: StateChainStreamApi,
{
	let dot_client = DotRetryRpcClient::new(
		scope,
		DotHttpRpcClient::new(&settings.http_node_endpoint).await?,
		DotSubClient::new(&settings.ws_node_endpoint),
	);

	let dot_chain_tracking = DotUnfinalisedSource::new(dot_client.clone())
		.shared(scope)
		.then(|header| async move { header.data.iter().filter_map(filter_map_events).collect() })
		.chunk_by_time(epoch_source.clone())
		.await
		.chain_tracking(state_chain_client.clone(), dot_client.clone())
		.run();

	scope.spawn(async move {
		dot_chain_tracking.await;
		Ok(())
	});

	let vaults = epoch_source.vaults().await;

	let dot_ingress_witnessing = DotFinalisedSource::new(dot_client)
		.shared(scope)
		.then(|header| async move {
			header.data.iter().filter_map(filter_map_events).collect::<Vec<_>>()
		})
		.chunk_by_vault(vaults)
		.await
		.ingress_addresses(scope, state_chain_stream, state_chain_client.clone())
		.await;

	let dot_ingress_witnessing = dot_ingress_witnessing
		.then(move |epoch, header| {
			let state_chain_client = state_chain_client.clone();
			async move {
				let (events, addresses_and_details) = header.data;

				let addresses = address_and_details_to_addresses(addresses_and_details);

				let mut deposit_witnesses = vec![];
				// TODO: We might as well add the proxy added here too.
				for (phase, wrapped_event) in events {
					if let Phase::ApplyExtrinsic(extrinsic_index) = phase {
						match wrapped_event {
							EventWrapper::Transfer(Transfer { to, amount, from: _ }) => {
								if addresses.contains(&to) {
									deposit_witnesses.push(DepositWitness {
										deposit_address: to,
										asset: assets::dot::Asset::Dot,
										amount,
										tx_id: TxId { block_number: header.index, extrinsic_index },
									});
								}
							},
							evt => {
								println!("Ignoring event: {:?}", evt);
							},
						}
					}
				}

				if !deposit_witnesses.is_empty() {
					state_chain_client
						.submit_signed_extrinsic(pallet_cf_witnesser::Call::witness_at_epoch {
							call: Box::new(
								pallet_cf_ingress_egress::Call::<_, PolkadotInstance>::process_deposits {
									deposit_witnesses,
								}
								.into(),
							),
							epoch_index: epoch.index,
						})
						.await;
				}
			}
		})
		.run();

	scope.spawn(async move {
		dot_ingress_witnessing.await;
		Ok(())
	});

	Ok(())
}

fn address_and_details_to_addresses(
	address_and_details: Vec<(PolkadotAccountId, DepositAddressDetails<Polkadot>)>,
) -> Vec<PolkadotAccountId> {
	address_and_details
		.into_iter()
		.map(|(address, details)| {
			assert_eq!(details.source_asset, assets::dot::Asset::Dot);
			address
		})
		.collect()
}
