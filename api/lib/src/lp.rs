use std::collections::HashMap;

use anyhow::{anyhow, bail, Result};
use cf_primitives::{
	AccountRole, AmmRange, Asset, AssetAmount, EgressId, ForeignChainAddress, Liquidity,
	PoolAssetMap, Tick,
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
			let (latest_block_hash, _, state_chain_client) = StateChainClient::new(
				scope,
				state_chain_settings,
				AccountRole::LiquidityProvider,
				false,
			)
			.await?;

			let asset_list = vec![Asset::Eth, Asset::Flip, Asset::Usdc, Asset::Dot, Asset::Btc];

			let balances: HashMap<Asset, AssetAmount> =
				futures::future::join_all(asset_list.iter().map(|asset| async {
					(
						*asset,
						state_chain_client
							.storage_double_map_entry::<pallet_cf_lp::FreeBalances<state_chain_runtime::Runtime>>(
								latest_block_hash,
								&state_chain_client.account_id(),
								asset,
							)
							.await
							.expect("Failed to request free balance")
							.unwrap_or(0),
					)
				}))
				.await
				.into_iter()
				.collect();

			Ok(balances)
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

			let asset_list = vec![Asset::Eth, Asset::Flip, Asset::Usdc, Asset::Dot, Asset::Btc];

			let positions: HashMap<Asset, Vec<(Tick, Tick, Liquidity)>> =
				futures::future::join_all(asset_list.iter().map(|asset| async {
					(
						*asset,
						state_chain_client
							.base_rpc_client
							.pool_minted_positions(
								state_chain_client.account_id(),
								*asset,
								Some(latest_block_hash),
							)
							.await
							.expect("Failed to request minted positions"),
					)
				}))
				.await
				.into_iter()
				.collect();

			Ok(positions)
		}
		.boxed()
	})
	.await
}

#[derive(Serialize)]
pub struct MintPositionReturn {
	assets_debited: PoolAssetMap<u128>,
	fees_harvested: PoolAssetMap<u128>,
}

pub async fn mint_position(
	state_chain_settings: &settings::StateChain,
	asset: Asset,
	range: AmmRange,
	amount: Liquidity,
) -> Result<MintPositionReturn> {
	task_scope(|scope| {
		async {
			let (latest_block_hash, block_stream, state_chain_client) = StateChainClient::new(
				scope,
				state_chain_settings,
				AccountRole::LiquidityProvider,
				false,
			)
			.await?;

			let asset_positions = state_chain_client
				.base_rpc_client
				.pool_minted_positions(
					state_chain_client.account_id(),
					asset,
					Some(latest_block_hash),
				)
				.await
				.expect("Failed to request minted positions");

			let liquidity_target = if let Some((_, _, current_amount)) = asset_positions
				.iter()
				.find(|(lower, upper, _)| lower == &range.lower && upper == &range.upper)
			{
				// Calculate the new target
				amount.saturating_add(*current_amount)
			} else {
				// Mint a new position
				amount
			};

			let mut block_stream = Box::new(block_stream);

			let call = pallet_cf_lp::Call::update_position { asset, range, liquidity_target };
			let (_tx_hash, events) =
				submit_and_ensure_success(&state_chain_client, block_stream.as_mut(), call).await?;

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
	assets_returned: PoolAssetMap<u128>,
	fees_harvested: PoolAssetMap<u128>,
}

pub async fn burn_position(
	state_chain_settings: &settings::StateChain,
	asset: Asset,
	range: AmmRange,
	amount: Liquidity,
) -> Result<BurnPositionReturn> {
	task_scope(|scope| {
		async {
			let (latest_block_hash, block_stream, state_chain_client) = StateChainClient::new(
				scope,
				state_chain_settings,
				AccountRole::LiquidityProvider,
				false,
			)
			.await?;

			let asset_positions = state_chain_client
				.base_rpc_client
				.pool_minted_positions(
					state_chain_client.account_id(),
					asset,
					Some(latest_block_hash),
				)
				.await
				.expect("Failed to request minted positions");

			let liquidity_target = if let Some((_, _, current_amount)) = asset_positions
				.iter()
				.find(|(lower, upper, _)| lower == &range.lower && upper == &range.upper)
			{
				// Calculate the new target
				current_amount
					.checked_sub(amount)
					.ok_or("Insufficient minted liquidity at position")
					.map_err(|e| anyhow!("{e}"))?
			} else {
				bail!("No position found");
			};

			let mut block_stream = Box::new(block_stream);

			let call = pallet_cf_lp::Call::update_position { asset, range, liquidity_target };
			let (_tx_hash, events) =
				submit_and_ensure_success(&state_chain_client, block_stream.as_mut(), call).await?;

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
