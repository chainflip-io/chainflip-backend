use std::sync::Arc;

use cf_chains::Polkadot;
use futures::StreamExt;
use sp_core::H256;

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
	logger: slog::Logger,
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
		"DOT-Runtime-Version".to_string(),
		epoch_starts_receiver,
		|epoch_start| epoch_start.current,
		on_chain_runtime_version,
		move |end_witnessing_signal, epoch_start, mut last_version_witnessed, logger| {
			let state_chain_client = state_chain_client.clone();
			let dot_client = dot_client.clone();
			async move {
				// NB: The first item of this stream is the current runtime version.
				let mut runtime_version_subscription =
					dot_client.subscribe_runtime_version().await?;

				while let Some(result_runtime_version) = runtime_version_subscription.next().await {
					// TODO: Change end_witnessing_signal to a oneshot channel and tokio::select!
					// the two futures. Currently this process will keep running, waiting on the
					// above `.next()` call until a PolkadotRuntime upgrade is introduced. This is
					// not a problem, but it is not ideal.
					// https://github.com/chainflip-io/chainflip-backend/issues/2825
					if let Some(_end_block) = *end_witnessing_signal.lock().unwrap() {
						break
					}

					match result_runtime_version {
						Ok(new_runtime_version) => {
							if new_runtime_version.spec_version >
								last_version_witnessed.spec_version
							{
								assert!(
									new_runtime_version.transaction_version >=
										last_version_witnessed.transaction_version,
									"Polkadot spec version must be bumped when the transaction version is bumped"
								);
								let _result = state_chain_client
									.submit_signed_extrinsic(
										pallet_cf_witnesser::Call::witness_at_epoch {
											call: Box::new(
												pallet_cf_environment::Call::update_polkadot_runtime_version {
													runtime_version: new_runtime_version,
												}
												.into(),
											),
											epoch_index: epoch_start.epoch_index,
										},
										&logger,
									)
									.await;
								slog::info!(
									logger,
									"Polkadot runtime version update submitted, version witnessed: {:?}",
									new_runtime_version
								);
								last_version_witnessed = new_runtime_version;
							}
						},
						Err(e) => {
							slog::error!(logger, "Error receiving runtime version update: {:?}", e);
						},
					}
				}

				Ok(last_version_witnessed)
			}
		},
		&logger,
	)
	.await
}
