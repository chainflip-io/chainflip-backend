use std::sync::Arc;

use cf_chains::{dot::RuntimeVersion, Polkadot};
use futures::StreamExt;
use tokio::select;
use tracing::{info, info_span, Instrument};

use crate::{
	state_chain_observer::client::{extrinsic_api::ExtrinsicApi, storage_api::StorageApi},
	try_with_logging,
	witnesser::{epoch_witnesser, EpochStart},
};

use super::rpc::DotRpcApi;
pub async fn start<StateChainClient, DotRpc>(
	epoch_starts_receiver: async_broadcast::Receiver<EpochStart<Polkadot>>,
	dot_client: DotRpc,
	last_version_witnessed: RuntimeVersion,
	state_chain_client: Arc<StateChainClient>,
) -> Result<(), (async_broadcast::Receiver<EpochStart<Polkadot>>, RuntimeVersion)>
where
	StateChainClient: ExtrinsicApi + StorageApi + 'static + Send + Sync,
	DotRpc: DotRpcApi + 'static + Send + Sync + Clone,
{
	epoch_witnesser::start(
		epoch_starts_receiver,
		|epoch_start| epoch_start.current,
		last_version_witnessed,
		move |mut end_witnessing_receiver, epoch_start, mut last_version_witnessed| {
			let state_chain_client = state_chain_client.clone();
			let dot_client = dot_client.clone();
			async move {
				// NB: The first item of this stream is the current runtime version.
				let mut runtime_version_subscription = try_with_logging!(
					dot_client.subscribe_runtime_version().await,
					last_version_witnessed
				);

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
