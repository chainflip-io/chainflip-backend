use std::sync::Arc;

use cf_chains::Polkadot;
use futures::StreamExt;
use sp_core::H256;
use tokio::select;
use tracing::{info, info_span, Instrument};

use crate::{
	state_chain_observer::client::{extrinsic_api::ExtrinsicApi, storage_api::StorageApi},
	witnesser::{epoch_witnesser, EpochStart},
};

use super::rpc::DotRpcApi;
pub async fn start<StateChainClient, DotRpc>(
	epoch_starts_receiver: async_broadcast::Receiver<EpochStart<Polkadot>>,
	dot_client: DotRpc,
	state_chain_client: Arc<StateChainClient>,
	latest_block_hash: H256,
) -> Result<(), (async_broadcast::Receiver<EpochStart<Polkadot>>, anyhow::Error)>
where
	StateChainClient: ExtrinsicApi + StorageApi + 'static + Send + Sync,
	DotRpc: DotRpcApi + 'static + Send + Sync + Clone,
{
	// When this witnesser starts up, we should check that the runtime version is up to
	// date with the chain. This is in case we missed a Polkadot runtime upgrade when
	// we were down.
	let on_chain_runtime_version = match state_chain_client
		.storage_value::<pallet_cf_environment::PolkadotRuntimeVersion<state_chain_runtime::Runtime>>(
			latest_block_hash,
		)
		.await
	{
		Ok(version) => version,
		Err(e) =>
			return Err((
				epoch_starts_receiver,
				anyhow::anyhow!("Failed to get PolkadotRuntimeVersion from SC: {:?}", e),
			)),
	};

	epoch_witnesser::start(
		epoch_starts_receiver,
		|epoch_start| epoch_start.current,
		on_chain_runtime_version,
		move |mut end_witnessing_receiver, epoch_start, mut last_version_witnessed| {
			let state_chain_client = state_chain_client.clone();
			let dot_client = dot_client.clone();
			async move {
				// NB: The first item of this stream is the current runtime version.
				let mut runtime_version_subscription =
					dot_client.subscribe_runtime_version().await?;

				loop {
					select! {
						_end_block = &mut end_witnessing_receiver => {
							break;
						}
						Some(new_runtime_version) = runtime_version_subscription.next() => {
							if new_runtime_version.spec_version > last_version_witnessed.spec_version {
								let _result = state_chain_client
									.submit_signed_extrinsic(pallet_cf_witnesser::Call::witness_at_epoch {
										call: Box::new(
											pallet_cf_environment::Call::update_polkadot_runtime_version {
												runtime_version: new_runtime_version,
											}
											.into(),
										),
										epoch_index: epoch_start.epoch_index,
									})
									.await;
								info!("Polkadot runtime version update submitted, version witnessed: {new_runtime_version:?}");
								last_version_witnessed = new_runtime_version;
							}
						}
					}
				}
				Ok(last_version_witnessed)
			}
		},
	)
	.instrument(info_span!("DOT-Runtime-Version"))
	.await
}
