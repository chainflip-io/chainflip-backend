use super::*;
use cf_chains::{address::ToHumanreadableAddress, Chain};
use cf_primitives::{chains::assets::any, AssetAmount};
use chainflip_engine::state_chain_observer::client::{
	chain_api::ChainApi, storage_api::StorageApi,
};
pub use pallet_cf_pools::Pool;
use serde::Deserialize;
use state_chain_runtime::PalletInstanceAlias;
use std::{collections::BTreeMap, sync::Arc};
use tracing::log;
use utilities::task_scope;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwapChannelInfo<C: Chain> {
	deposit_address: <C::ChainAccount as ToHumanreadableAddress>::Humanreadable,
	source_asset: any::Asset,
	destination_asset: any::Asset,
	expiry_block: state_chain_runtime::BlockNumber,
}

pub struct QueryApi {
	pub(crate) state_chain_client: Arc<StateChainClient>,
}

impl QueryApi {
	pub async fn connect<'a>(
		scope: &task_scope::Scope<'a, anyhow::Error>,
		state_chain_settings: &settings::StateChain,
	) -> Result<QueryApi> {
		log::debug!("Connecting to state chain at: {}", state_chain_settings.ws_endpoint);

		let (_state_chain_stream, state_chain_client) = StateChainClient::connect_with_account(
			scope,
			&state_chain_settings.ws_endpoint,
			&state_chain_settings.signing_key_file,
			AccountRole::None,
			false,
		)
		.await?;

		Ok(Self { state_chain_client })
	}

	pub async fn get_open_swap_channels<C: Chain + PalletInstanceAlias>(
		&self,
		block_hash: Option<state_chain_runtime::Hash>,
	) -> Result<Vec<SwapChannelInfo<C>>, anyhow::Error>
	where
		state_chain_runtime::Runtime:
			pallet_cf_ingress_egress::Config<C::Instance, TargetChain = C>,
	{
		let block_hash =
			block_hash.unwrap_or_else(|| self.state_chain_client.latest_finalized_hash());

		let (channel_details, channel_actions, network_environment) = tokio::try_join!(
				self.state_chain_client
					.storage_map::<pallet_cf_ingress_egress::DepositChannelLookup<
						state_chain_runtime::Runtime,
						C::Instance,
					>, Vec<_>>(block_hash)
					.map(|result| {
						result.map(|channels| channels.into_iter().collect::<BTreeMap<_, _>>())
					}),
				self.state_chain_client.storage_map::<pallet_cf_ingress_egress::ChannelActions<
					state_chain_runtime::Runtime,
					C::Instance,
				>, Vec<_>>(block_hash,),
				self.state_chain_client
					.storage_value::<pallet_cf_environment::ChainflipNetworkEnvironment<
						state_chain_runtime::Runtime,
					>>(block_hash),
			)?;

		Ok(channel_actions
			.iter()
			.filter_map(|(address, action)| {
				match action {
					pallet_cf_ingress_egress::ChannelAction::Swap { destination_asset, .. } |
					pallet_cf_ingress_egress::ChannelAction::CcmTransfer {
						destination_asset,
						..
					} => Some(destination_asset),
					_ => None,
				}
				.and_then(|destination_asset| {
					channel_details.get(address).map(|details| {
						(destination_asset, details.deposit_channel.clone(), details.expires_at)
					})
				})
				.map(|(&destination_asset, deposit_channel, expiry)| SwapChannelInfo {
					deposit_address: deposit_channel.address.to_humanreadable(network_environment),
					source_asset: deposit_channel.asset.into(),
					destination_asset,
					expiry_block: expiry,
				})
			})
			.collect::<Vec<_>>())
	}

	pub async fn get_balances(
		&self,
		block_hash: Option<state_chain_runtime::Hash>,
	) -> Result<BTreeMap<Asset, AssetAmount>> {
		let block_hash =
			block_hash.unwrap_or_else(|| self.state_chain_client.latest_finalized_hash());

		futures::future::join_all(Asset::all().iter().map(|asset| async {
			Ok((
				*asset,
				self.state_chain_client
					.storage_double_map_entry::<pallet_cf_lp::FreeBalances<state_chain_runtime::Runtime>>(
						block_hash,
						&self.state_chain_client.account_id(),
						asset,
					)
					.await?
					.unwrap_or_default(),
			))
		}))
		.await
		.into_iter()
		.collect()
	}

	pub async fn get_range_orders(
		&self,
		block_hash: Option<state_chain_runtime::Hash>,
		account_id: Option<state_chain_runtime::AccountId>,
	) -> Result<BTreeMap<Asset, Vec<RangeOrderPosition>>, anyhow::Error> {
		let block_hash =
			block_hash.unwrap_or_else(|| self.state_chain_client.latest_finalized_hash());
		let account_id = account_id.unwrap_or_else(|| self.state_chain_client.account_id());

		Ok(self
			.state_chain_client
			.storage_map::<pallet_cf_pools::Pools<state_chain_runtime::Runtime>, Vec<_>>(block_hash)
			.await?
			.into_iter()
			.map(|(asset, pool)| {
				(
					asset,
					pool.pool_state
						.range_orders
						.positions()
						.into_iter()
						.filter_map(|((owner, lower_tick, upper_tick), liquidity)| {
							if owner == account_id {
								Some(RangeOrderPosition { lower_tick, upper_tick, liquidity })
							} else {
								None
							}
						})
						.collect(),
				)
			})
			.collect())
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RangeOrderPosition {
	pub lower_tick: i32,
	pub upper_tick: i32,
	#[serde(with = "utilities::serde_helpers::number_or_hex")]
	pub liquidity: u128,
}
