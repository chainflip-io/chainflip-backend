use std::{collections::HashMap, sync::Arc};

use anyhow::{anyhow, Result};
pub use cf_amm::{
	common::{SideMap, Tick},
	range_orders::Liquidity,
};
use cf_chains::address::EncodedAddress;
use cf_primitives::{AccountRole, Asset, AssetAmount, EgressId};
use chainflip_engine::{
	settings,
	state_chain_observer::client::{
		base_rpc_api::BaseRpcApi,
		extrinsic_api::signed::{SignedExtrinsicApi, Watch},
		storage_api::StorageApi,
		StateChainClient,
	},
};
pub use core::ops::Range;
use futures::FutureExt;
use serde::Serialize;
use utilities::{task_scope::task_scope, CachedStream};

use crate::connect_submit_and_get_events;

pub async fn liquidity_deposit(
	state_chain_settings: &settings::StateChain,
	asset: Asset,
) -> Result<EncodedAddress> {
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
	egress_address: EncodedAddress,
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
			let (state_chain_stream, state_chain_client) = StateChainClient::connect_with_account(
				scope,
				&state_chain_settings.ws_endpoint,
				&state_chain_settings.signing_key_file,
				AccountRole::None,
				false,
			)
			.await?;

			let balances: Result<HashMap<Asset, AssetAmount>> =
				futures::future::join_all(Asset::all().iter().map(|asset| async {
					Ok((
						*asset,
						state_chain_client
							.storage_double_map_entry::<pallet_cf_lp::FreeBalances<state_chain_runtime::Runtime>>(
								state_chain_stream.cache().block_hash,
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

pub async fn get_range_orders(
	state_chain_settings: &settings::StateChain,
) -> Result<HashMap<Asset, Vec<(Tick, Tick, Liquidity)>>> {
	task_scope(|scope| {
		async {
			let (state_chain_stream, state_chain_client) = StateChainClient::connect_with_account(
				scope,
				&state_chain_settings.ws_endpoint,
				&state_chain_settings.signing_key_file,
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
							state_chain_stream.cache().block_hash,
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
	assets_debited: SideMap<AssetAmount>,
	collected_fees: SideMap<AssetAmount>,
}

pub async fn mint_range_order(
	state_chain_settings: &settings::StateChain,
	asset: Asset,
	range: Range<Tick>,
	amount: Liquidity,
) -> Result<MintPositionReturn> {
	task_scope(|scope| {
		async {
			// Connect to State Chain
			let (_state_chain_stream, state_chain_client) = StateChainClient::connect_with_account(
				scope,
				&state_chain_settings.ws_endpoint,
				&state_chain_settings.signing_key_file,
				AccountRole::LiquidityProvider,
				false,
			)
			.await?;

			// Submit the mint order
			let (_tx_hash, events, _dispatch_info) = state_chain_client
				.submit_signed_extrinsic(pallet_cf_pools::Call::collect_and_mint_range_order {
					unstable_asset: asset,
					price_range_in_ticks: range,
					liquidity: amount,
				})
				.await
				.watch()
				.await?;

			// Get some details from the emitted event
			Ok(events
				.into_iter()
				.find_map(|event| match event {
					state_chain_runtime::RuntimeEvent::LiquidityPools(
						pallet_cf_pools::Event::RangeOrderMinted {
							assets_debited,
							collected_fees,
							..
						},
					) => Some(MintPositionReturn { assets_debited, collected_fees }),
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
	assets_credited: SideMap<AssetAmount>,
	collected_fees: SideMap<AssetAmount>,
}

pub async fn burn_range_order(
	state_chain_settings: &settings::StateChain,
	asset: Asset,
	range: Range<Tick>,
	amount: Liquidity,
) -> Result<BurnPositionReturn> {
	task_scope(|scope| {
		async {
			// Connect to State Chain
			let (_state_chain_stream, state_chain_client) = StateChainClient::connect_with_account(
				scope,
				&state_chain_settings.ws_endpoint,
				&state_chain_settings.signing_key_file,
				AccountRole::LiquidityProvider,
				false,
			)
			.await?;

			// TODO: Re-enable this check after #3082 in implemented
			// Find the current position and calculate new target amount
			// if get_liquidity_at_position(&state_chain_client, asset, range, latest_block_hash)
			// 	.await? < amount
			// {
			// 	bail!("Insufficient minted liquidity at position");
			// }

			// Submit the burn call
			let (_tx_hash, events, _dispatch_info) = state_chain_client
				.submit_signed_extrinsic(pallet_cf_pools::Call::collect_and_burn_range_order {
					unstable_asset: asset,
					price_range_in_ticks: range,
					liquidity: amount,
				})
				.await
				.watch()
				.await?;

			// Get some details from the emitted event
			Ok(events
				.into_iter()
				.find_map(|event| match event {
					state_chain_runtime::RuntimeEvent::LiquidityPools(
						pallet_cf_pools::Event::RangeOrderBurned {
							assets_credited,
							collected_fees,
							..
						},
					) => Some(BurnPositionReturn { assets_credited, collected_fees }),
					_ => None,
				})
				.expect("LiquidityBurned must have been generated"))
		}
		.boxed()
	})
	.await
}

#[allow(dead_code)]
async fn get_liquidity_at_position(
	state_chain_client: &Arc<StateChainClient>,
	asset: Asset,
	range: Range<Tick>,
	at: state_chain_runtime::Hash,
) -> Result<AssetAmount> {
	state_chain_client
		.base_rpc_client
		.pool_minted_positions(state_chain_client.account_id(), asset, at)
		.await?
		.iter()
		.find_map(|(start, end, current_amount)| {
			if start == &range.start && end == &range.end {
				Some(*current_amount)
			} else {
				None
			}
		})
		.ok_or(anyhow!("No position found"))
}
