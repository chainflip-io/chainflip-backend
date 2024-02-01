use std::sync::Arc;

use cf_chains::{Chain, ChainCrypto};
use state_chain_runtime::PalletInstanceAlias;
use utilities::task_scope::Scope;

use crate::{
	state_chain_observer::client::{
		storage_api::StorageApi, stream_api::StreamApi, STATE_CHAIN_CONNECTION,
	},
	witness::common::{
		chunked_chain_source::chunked_by_vault::monitored_items::MonitoredSCItems, RuntimeHasChain,
	},
};

use super::{builder::ChunkedByVaultBuilder, ChunkedByVault};

pub type TxOutId<Inner> =
	<<<Inner as ChunkedByVault>::Chain as Chain>::ChainCrypto as ChainCrypto>::TransactionOutId;
pub type TxOutIds<Inner> = Vec<TxOutId<Inner>>;

pub type ChainBlockNumber<Inner> = <<Inner as ChunkedByVault>::Chain as Chain>::ChainBlockNumber;

pub type TxOutIdsInitiatedAt<Inner> = Vec<(TxOutId<Inner>, ChainBlockNumber<Inner>)>;

impl<Inner: ChunkedByVault> ChunkedByVaultBuilder<Inner> {
	pub async fn egress_items<'env, StateChainStream, StateChainClient, const IS_FINALIZED: bool>(
		self,
		scope: &Scope<'env, anyhow::Error>,
		state_chain_stream: StateChainStream,
		state_chain_client: Arc<StateChainClient>,
	) -> ChunkedByVaultBuilder<
		MonitoredSCItems<
			Inner,
			TxOutIdsInitiatedAt<Inner>,
			impl Fn(Inner::Index, &TxOutIdsInitiatedAt<Inner>) -> TxOutIdsInitiatedAt<Inner>
				+ Send
				+ Sync
				+ Clone
				+ 'static,
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
				state_chain_client.clone(),
				move |block_hash| {
					let state_chain_client = state_chain_client_c.clone();
					async move {
						state_chain_client
							.storage_map::<pallet_cf_broadcast::TransactionOutIdToBroadcastId<
								state_chain_runtime::Runtime,
								<Inner::Chain as PalletInstanceAlias>::Instance,
							>, Vec<_>>(block_hash)
							.await
							.expect(STATE_CHAIN_CONNECTION)
							.into_iter()
							.map(|(tx_out_id, (_broadcast_id, initiated_at))| {
								(tx_out_id, initiated_at)
							})
							.collect()
					}
				},
				|index, tx_out_ids: &TxOutIdsInitiatedAt<Inner>| {
					tx_out_ids
						.iter()
						.filter(|(_, initiated_at)| initiated_at <= &index)
						.cloned()
						.collect()
				},
			)
			.await,
			self.parameters,
		)
	}
}
