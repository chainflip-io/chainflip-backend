use pallet_cf_ingress_egress::DepositChannelDetails;
use state_chain_runtime::PalletInstanceAlias;
use std::sync::Arc;
use utilities::task_scope::Scope;

use crate::{
	state_chain_observer::client::{
		storage_api::StorageApi, stream_api::StreamApi, STATE_CHAIN_CONNECTION,
	},
	witness::common::RuntimeHasChain,
};

use super::{builder::ChunkedByVaultBuilder, monitored_items::MonitoredSCItems, ChunkedByVault};

pub type Addresses<Inner> = Vec<
	DepositChannelDetails<
		state_chain_runtime::Runtime,
		<<Inner as ChunkedByVault>::Chain as PalletInstanceAlias>::Instance,
	>,
>;

impl<Inner: ChunkedByVault> ChunkedByVaultBuilder<Inner> {
	pub async fn deposit_addresses<
		'env,
		StateChainStream,
		StateChainClient,
		const IS_FINALIZED: bool,
	>(
		self,
		scope: &Scope<'env, anyhow::Error>,
		state_chain_stream: StateChainStream,
		state_chain_client: Arc<StateChainClient>,
	) -> ChunkedByVaultBuilder<
		MonitoredSCItems<
			Inner,
			Addresses<Inner>,
			impl Fn(Inner::Index, &Addresses<Inner>) -> Addresses<Inner> + Send + Sync + Clone + 'static,
		>,
	>
	where
		state_chain_runtime::Runtime: RuntimeHasChain<Inner::Chain>,
		StateChainStream: StreamApi<IS_FINALIZED>,
		StateChainClient: StorageApi + Send + Sync + 'static,
	{
		let state_chain_client_c = state_chain_client.clone();
		ChunkedByVaultBuilder::new(
			MonitoredSCItems::new(
				self.source,
				scope,
				state_chain_stream,
				state_chain_client,
				move |block_hash| {
					let state_chain_client = state_chain_client_c.clone();
					async move {
						state_chain_client
							.storage_map_values::<pallet_cf_ingress_egress::DepositChannelLookup<
								state_chain_runtime::Runtime,
								<Inner::Chain as PalletInstanceAlias>::Instance,
							>>(block_hash)
							.await
							.expect(STATE_CHAIN_CONNECTION)
					}
				},
				|index, addresses: &Addresses<Inner>| {
					addresses
						.iter()
						.filter(|details| details.opened_at <= index && index <= details.expires_at)
						.cloned()
						.collect()
				},
			)
			.await,
			self.parameters,
		)
	}
}
