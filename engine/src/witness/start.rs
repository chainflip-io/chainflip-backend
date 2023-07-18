use std::sync::Arc;

use utilities::task_scope::Scope;

use crate::{
	settings::Settings,
	state_chain_observer::client::{
		extrinsic_api::signed::SignedExtrinsicApi, storage_api::StorageApi, StateChainStreamApi,
	},
};

use super::{epoch_source::EpochSource, vault::EthAssetApi};

use anyhow::Result;

/// Starts all the witnessing tasks.
pub async fn start<StateChainClient, StateChainStream>(
	scope: &Scope<'_, anyhow::Error>,
	settings: &Settings,
	state_chain_client: Arc<StateChainClient>,
	state_chain_stream: StateChainStream,
) -> Result<()>
where
	StateChainStream: StateChainStreamApi,
	StateChainClient: StorageApi + EthAssetApi + SignedExtrinsicApi + 'static + Send + Sync,
{
	let initial_block_hash = state_chain_stream.cache().block_hash;
	let epoch_source = EpochSource::builder(scope, state_chain_stream, state_chain_client.clone())
		.await
		.participating(state_chain_client.account_id())
		.await;

	super::eth::start(
		scope,
		&settings.eth,
		state_chain_client.clone(),
		epoch_source.clone(),
		initial_block_hash,
	)
	.await?;

	super::btc::start(scope, &settings.btc, state_chain_client.clone(), epoch_source.clone())
		.await?;

	super::dot::start(scope, &settings.dot, state_chain_client, epoch_source).await?;

	Ok(())
}

#[cfg(test)]
mod tests {

	use crate::state_chain_observer;
	use cf_primitives::AccountId;
	use futures::{FutureExt, StreamExt};
	use std::str::FromStr;
	use utilities::task_scope::task_scope;

	use super::*;

	#[tokio::test]
	#[ignore = "useful for testing the epoch source"]
	async fn run_epoch_source() {
		task_scope(|scope| {
			async {
				let (state_chain_stream, state_chain_client) =
					state_chain_observer::client::StateChainClient::connect_without_account(
						scope,
						"ws://localhost:9944",
					)
					.await?;

				let bashful =
					AccountId::from_str("cFK7GTahm9qeX5Jjct3yfSvV4qLb8LJaArHL2SL6m9HAzc2sq")
						.unwrap();

				let mut epoch_source =
					EpochSource::new(scope, state_chain_stream, state_chain_client.clone())
						.await
						.participating(bashful)
						.await
						.into_stream()
						.await
						.into_box()
						.into_stream();

				while let Some(epoch) = epoch_source.next().await {
					println!("Epoch: {:?}", epoch.index);
				}

				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap()
	}
}
