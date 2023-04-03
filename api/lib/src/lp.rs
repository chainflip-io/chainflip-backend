use std::{collections::HashMap, sync::Arc};

use anyhow::{anyhow, Result};
use cf_chains::{btc::BitcoinNetwork, ForeignChainAddress};
use cf_primitives::{
	AccountRole, AmmRange, Asset, AssetAmount, EgressId, Liquidity, PoolAssetMap, Tick,
};
use chainflip_engine::{
	settings,
	state_chain_observer::client::{
		base_rpc_api::BaseRpcApi, extrinsic_api::ExtrinsicApi, storage_api::StorageApi,
		StateChainClient,
	},
	task_scope::task_scope,
};
use futures::FutureExt;
use serde::Serialize;

use crate::{connect_submit_and_get_events, submit_and_ensure_success};

pub async fn liquidity_deposit(
	state_chain_settings: &settings::StateChain,
	asset: Asset,
) -> Result<ForeignChainAddress> {
	let events = connect_submit_and_get_events(
		state_chain_settings,
		pallet_cf_lp::Call::request_deposit_address { asset },
		AccountRole::LiquidityProvider,
	)
	.await?;

	Ok(events
		.into_iter()
		.find_map(|event| match event {
			state_chain_runtime::RuntimeEvent::LiquidityProvider(
				pallet_cf_lp::Event::DepositAddressReady { ingress_address, .. },
			) => Some(ingress_address),
			_ => None,
		})
		.expect("DepositAddressReady must have been generated"))
}

pub async fn withdraw_asset(
	state_chain_settings: &settings::StateChain,
	amount: AssetAmount,
	asset: Asset,
	egress_address: ForeignChainAddress,
) -> Result<EgressId> {
	let events = connect_submit_and_get_events(
		state_chain_settings,
		pallet_cf_lp::Call::withdraw_asset { amount, asset, egress_address },
		AccountRole::LiquidityProvider,
	)
	.await?;

	Ok(events
		.into_iter()
		.find_map(|event| match event {
			state_chain_runtime::RuntimeEvent::LiquidityProvider(
				pallet_cf_lp::Event::WithdrawalEgressScheduled { egress_id, .. },
			) => Some(egress_id),
			_ => None,
		})
		.expect("WithdrawalEgressScheduled must have been generated"))
}

pub async fn get_balances(
	state_chain_settings: &settings::StateChain,
) -> Result<HashMap<Asset, AssetAmount>> {
	task_scope(|scope| {
		async {
			let (latest_block_hash, _, state_chain_client) =
				StateChainClient::new(scope, state_chain_settings, AccountRole::None, false)
					.await?;

			let balances: Result<HashMap<Asset, AssetAmount>> =
				futures::future::join_all(Asset::all().iter().map(|asset| async {
					Ok((
						*asset,
						state_chain_client
							.storage_double_map_entry::<pallet_cf_lp::FreeBalances<state_chain_runtime::Runtime>>(
								latest_block_hash,
								&state_chain_client.account_id(),
								asset,
							)
							.await?
							.unwrap_or_default(),
					))
				}))
				.await
				.into_iter()
				.collect();

			balances
		}
		.boxed()
	})
	.await
}

pub async fn get_positions(
	state_chain_settings: &settings::StateChain,
) -> Result<HashMap<Asset, Vec<(Tick, Tick, Liquidity)>>> {
	task_scope(|scope| {
		async {
			let (latest_block_hash, _, state_chain_client) = StateChainClient::new(
				scope,
				state_chain_settings,
				AccountRole::LiquidityProvider,
				false,
			)
			.await?;

			futures::future::join_all(Asset::all().iter().map(|asset| async {
				Ok((
					*asset,
					state_chain_client
						.base_rpc_client
						.pool_minted_positions(
							state_chain_client.account_id(),
							*asset,
							latest_block_hash,
						)
						.await?,
				))
			}))
			.await
			.into_iter()
			.collect()
		}
		.boxed()
	})
	.await
}

#[derive(Serialize)]
pub struct MintPositionReturn {
	assets_debited: PoolAssetMap<AssetAmount>,
	fees_harvested: PoolAssetMap<AssetAmount>,
}

pub async fn mint_position(
	state_chain_settings: &settings::StateChain,
	asset: Asset,
	range: AmmRange,
	amount: Liquidity,
) -> Result<MintPositionReturn> {
	task_scope(|scope| {
		async {
			// Connect to State Chain
			let (latest_block_hash, block_stream, state_chain_client) = StateChainClient::new(
				scope,
				state_chain_settings,
				AccountRole::LiquidityProvider,
				false,
			)
			.await?;

			// Find the current position and calculate new target amount
			let liquidity_target =
				get_liquidity_at_position(&state_chain_client, asset, range, latest_block_hash)
					.await
					.unwrap_or_default()
					.saturating_add(amount);

			// Submit the update of the position
			let call = pallet_cf_lp::Call::update_position { asset, range, liquidity_target };
			let (_tx_hash, events) = submit_and_ensure_success(
				&state_chain_client,
				Box::new(block_stream).as_mut(),
				call,
			)
			.await?;

			// Get some details from the emitted event
			Ok(events
				.into_iter()
				.find_map(|event| match event {
					state_chain_runtime::RuntimeEvent::LiquidityPools(
						pallet_cf_pools::Event::LiquidityMinted {
							assets_debited,
							fees_harvested,
							..
						},
					) => Some(MintPositionReturn { assets_debited, fees_harvested }),
					_ => None,
				})
				.expect("LiquidityMinted must have been generated"))
		}
		.boxed()
	})
	.await
}

#[derive(Serialize)]
pub struct BurnPositionReturn {
	assets_returned: PoolAssetMap<AssetAmount>,
	fees_harvested: PoolAssetMap<AssetAmount>,
}

pub async fn burn_position(
	state_chain_settings: &settings::StateChain,
	asset: Asset,
	range: AmmRange,
	amount: Liquidity,
) -> Result<BurnPositionReturn> {
	task_scope(|scope| {
		async {
			// Connect to State Chain
			let (latest_block_hash, block_stream, state_chain_client) = StateChainClient::new(
				scope,
				state_chain_settings,
				AccountRole::LiquidityProvider,
				false,
			)
			.await?;

			// Find the current position and calculate new target amount
			let liquidity_target =
				get_liquidity_at_position(&state_chain_client, asset, range, latest_block_hash)
					.await?
					.checked_sub(amount)
					.ok_or(anyhow!("Insufficient minted liquidity at position"))?;

			// Submit the update of the position
			let call = pallet_cf_lp::Call::update_position { asset, range, liquidity_target };
			let (_tx_hash, events) = submit_and_ensure_success(
				&state_chain_client,
				Box::new(block_stream).as_mut(),
				call,
			)
			.await?;

			// Get some details from the emitted event
			Ok(events
				.into_iter()
				.find_map(|event| match event {
					state_chain_runtime::RuntimeEvent::LiquidityPools(
						pallet_cf_pools::Event::LiquidityBurned {
							assets_returned,
							fees_harvested,
							..
						},
					) => Some(BurnPositionReturn { assets_returned, fees_harvested }),
					_ => None,
				})
				.expect("LiquidityBurned must have been generated"))
		}
		.boxed()
	})
	.await
}

async fn get_liquidity_at_position(
	state_chain_client: &Arc<StateChainClient>,
	asset: Asset,
	range: AmmRange,
	at: state_chain_runtime::Hash,
) -> Result<AssetAmount> {
	state_chain_client
		.base_rpc_client
		.pool_minted_positions(state_chain_client.account_id(), asset, at)
		.await?
		.iter()
		.find_map(|(lower, upper, current_amount)| {
			if lower == &range.lower && upper == &range.upper {
				Some(*current_amount)
			} else {
				None
			}
		})
		.ok_or(anyhow!("No position found"))
}

pub async fn get_btc_network(
	state_chain_settings: &settings::StateChain,
) -> Result<BitcoinNetwork> {
	task_scope(|scope| {
		async {
			let (latest_block_hash, _, state_chain_client) =
				StateChainClient::new(scope, state_chain_settings, AccountRole::None, false)
					.await?;

			state_chain_client
				.storage_value::<pallet_cf_environment::BitcoinNetworkSelection<state_chain_runtime::Runtime>>(
					latest_block_hash,
				)
				.await
				.map_err(|_| anyhow!("Failed to get Bitcoin Network Selection"))
		}
		.boxed()
	})
	.await
}
