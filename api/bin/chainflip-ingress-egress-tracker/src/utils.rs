use cf_chains::{
	address::EncodedAddress,
	instances::{ChainInstanceAlias, ChainInstanceFor},
	ForeignChainAddress,
};
use cf_primitives::{AffiliateShortId, Affiliates, Beneficiary, BroadcastId, NetworkEnvironment};
use chainflip_engine::state_chain_observer::client::{
	chain_api::ChainApi, storage_api::StorageApi, STATE_CHAIN_CONNECTION,
};
use pallet_cf_broadcast::TransactionOutIdFor;
use sp_core::crypto::AccountId32;

use anyhow::anyhow;
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

pub fn destination_address_from_encoded_address(
	address: EncodedAddress,
	network: NetworkEnvironment,
) -> anyhow::Result<ForeignChainAddress> {
	cf_chains::address::try_from_encoded_address(address, || network)
		.map_err(|_| anyhow!("Invalid destination address"))
}

// Get a list of registered affiliates for the given broker and map the short ids to their account
// ids
pub async fn map_affiliates<
	StateChainApi: crate::witnessing::state_chain::TrackerStateChainApi<StateChain>,
	StateChain,
>(
	state_chain_client: &StateChainApi,
	affiliates: Affiliates<AffiliateShortId>,
	broker_id: AccountId32,
) -> Affiliates<AccountId32>
where
	StateChain: StorageApi + ChainApi + 'static + Send + Sync,
{
	if affiliates.is_empty() {
		return Affiliates::default();
	}

	let registered_affiliates = state_chain_client
		.get_affiliates(broker_id.clone())
		.await
		.expect(STATE_CHAIN_CONNECTION);

	affiliates
		.into_iter()
		.map(|affiliate| {
			let account_id = registered_affiliates
				.iter()
				.find(|(short_id, _)| short_id == &affiliate.account)
				.map(|(_, account_id)| account_id.clone())
				.unwrap_or_else(|| {
					log::warn!(
						"Affiliate not found for short id {} on broker {}",
						affiliate.account,
						broker_id
					);
					AccountId32::from([0; 32])
				});
			Beneficiary { account: account_id, bps: affiliate.bps }
		})
		.collect::<Vec<Beneficiary<AccountId32>>>()
		.try_into()
		.expect("Number of affiliates should always fit")
}
