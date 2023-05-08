use std::sync::Arc;

use anyhow::{anyhow, bail};
use async_trait::async_trait;
use cf_chains::{dot::RuntimeVersion, Polkadot};
use futures::StreamExt;
use sp_core::H256;
use tokio::sync::{oneshot, Mutex};
use tracing::{info, info_span, Instrument};

use crate::{
	state_chain_observer::client::{
		extrinsic_api::signed::SignedExtrinsicApi, storage_api::StorageApi,
	},
	witnesser::{
		epoch_process_runner::{
			self, start_epoch_process_runner, EpochProcessGenerator, EpochWitnesser,
			WitnesserInitResult,
		},
		ChainBlockNumber, EpochStart,
	},
};

use super::rpc::DotRpcApi;

pub async fn start<StateChainClient, DotRpc>(
	epoch_starts_receiver: async_broadcast::Receiver<EpochStart<Polkadot>>,
	dot_client: DotRpc,
	state_chain_client: Arc<StateChainClient>,
	latest_block_hash: H256,
) -> anyhow::Result<()>
where
	StateChainClient: SignedExtrinsicApi + StorageApi + 'static + Send + Sync,
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
		Err(e) => bail!("Failed to get PolkadotRuntimeVersion from SC: {:?}", e),
	};

	start_epoch_process_runner(
		// NOTE: we only use Arc<Mutex> here to
		// satisfy the interface...
		Arc::new(Mutex::new(epoch_starts_receiver)),
		RuntimeVersionUpdaterGenerator { state_chain_client, dot_client },
		on_chain_runtime_version,
	)
	.instrument(info_span!("DOT-Runtime-Version"))
	.await
	.map_err(|()| anyhow!("DOT-Runtime-Version witnesser exited unexpectedly"))
}

struct RuntimeVersionUpdater<StateChainClient> {
	state_chain_client: Arc<StateChainClient>,
	current_epoch: EpochStart<Polkadot>,
}

#[async_trait]
impl<StateChainClient> EpochWitnesser for RuntimeVersionUpdater<StateChainClient>
where
	StateChainClient: SignedExtrinsicApi + StorageApi + 'static + Send + Sync,
{
	type Data = RuntimeVersion;
	type Chain = Polkadot;

	// Last version witnessed
	type StaticState = RuntimeVersion;

	const SHOULD_PROCESS_HISTORICAL_EPOCHS: bool = false;

	async fn run_witnesser(
		self,
		data_stream: std::pin::Pin<
			Box<dyn futures::Stream<Item = anyhow::Result<Self::Data>> + Send + 'static>,
		>,
		end_witnessing_receiver: oneshot::Receiver<ChainBlockNumber<Self::Chain>>,
		state: Self::StaticState,
	) -> Result<Self::StaticState, ()> {
		epoch_process_runner::run_witnesser_data_stream(
			self,
			data_stream,
			end_witnessing_receiver,
			state,
		)
		.await
	}

	async fn do_witness(
		&mut self,
		new_runtime_version: RuntimeVersion,
		last_version_witnessed: &mut RuntimeVersion,
	) -> anyhow::Result<()> {
		if new_runtime_version.spec_version > last_version_witnessed.spec_version {
			self.state_chain_client
				.submit_signed_extrinsic(pallet_cf_witnesser::Call::witness_at_epoch {
					call: Box::new(
						pallet_cf_environment::Call::update_polkadot_runtime_version {
							runtime_version: new_runtime_version,
						}
						.into(),
					),
					epoch_index: self.current_epoch.epoch_index,
				})
				.await;
			info!("Polkadot runtime version update submitted, version witnessed: {new_runtime_version:?}");
			*last_version_witnessed = new_runtime_version;
		}

		Ok(())
	}
}

struct RuntimeVersionUpdaterGenerator<StateChainClient, DotRpc> {
	state_chain_client: Arc<StateChainClient>,
	dot_client: DotRpc,
}

#[async_trait]
impl<StateChainClient, DotRpc> EpochProcessGenerator
	for RuntimeVersionUpdaterGenerator<StateChainClient, DotRpc>
where
	StateChainClient: SignedExtrinsicApi + StorageApi + 'static + Send + Sync,
	DotRpc: DotRpcApi + 'static + Send + Sync + Clone,
{
	type Witnesser = RuntimeVersionUpdater<StateChainClient>;
	async fn init(
		&mut self,
		epoch: EpochStart<Polkadot>,
	) -> anyhow::Result<WitnesserInitResult<RuntimeVersionUpdater<StateChainClient>>> {
		// NB: The first item of this stream is the current runtime version.
		let runtime_version_subscription = self.dot_client.subscribe_runtime_version().await?;

		let witnesser = RuntimeVersionUpdater {
			state_chain_client: self.state_chain_client.clone(),
			current_epoch: epoch,
		};

		let stream = runtime_version_subscription.map(Ok);

		Ok(WitnesserInitResult::Created((witnesser, Box::pin(stream))))
	}
}
