use cf_chains::instances::{ChainInstanceAlias, ChainInstanceFor};
use cf_primitives::BroadcastId;
use chainflip_engine::state_chain_observer::client::{
	chain_api::ChainApi, storage_api::StorageApi, STATE_CHAIN_CONNECTION,
};
use pallet_cf_broadcast::TransactionOutIdFor;
use tracing::log;

pub fn hex_encode_bytes(bytes: &[u8]) -> String {
	format!("0x{}", hex::encode(bytes))
}

pub async fn get_broadcast_id<I, StateChainClient>(
	state_chain_client: &StateChainClient,
	tx_out_id: &TransactionOutIdFor<state_chain_runtime::Runtime, ChainInstanceFor<I>>,
) -> Option<BroadcastId>
where
	state_chain_runtime::Runtime: pallet_cf_broadcast::Config<ChainInstanceFor<I>>,
	I: ChainInstanceAlias + 'static,
	StateChainClient: StorageApi + ChainApi + 'static + Send + Sync,
{
	let id = state_chain_client
		.storage_map_entry::<pallet_cf_broadcast::TransactionOutIdToBroadcastId<
			state_chain_runtime::Runtime,
			ChainInstanceFor<I>,
		>>(state_chain_client.latest_unfinalized_block().hash, tx_out_id)
		.await
		.expect(STATE_CHAIN_CONNECTION)
		.map(|(broadcast_id, _)| broadcast_id);

	if id.is_none() {
		log::warn!("Broadcast ID not found for {:?}", tx_out_id);
	}

	id
}
