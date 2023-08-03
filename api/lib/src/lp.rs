use anyhow::{anyhow, Context, Result};
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
		extrinsic_api::signed::{SignedExtrinsicApi, UntilFinalized},
		storage_api::StorageApi,
		StateChainClient,
	},
};
pub use core::ops::Range;
use futures::FutureExt;
pub use pallet_cf_pools::{utilities as pool_utilities, Order as BuyOrSellOrder, RangeOrderSize};
use serde::{Deserialize, Serialize};
use sp_core::H256;
use state_chain_runtime::RuntimeCall;
use std::{collections::BTreeMap, sync::Arc};
use utilities::{task_scope::task_scope, CachedStream};

use crate::connect_submit_and_get_events;

pub async fn register_emergency_withdrawal_address(
	state_chain_settings: &settings::StateChain,
	address: EncodedAddress,
) -> Result<H256> {
	task_scope(|scope| {
		async {
			let call =
				RuntimeCall::from(pallet_cf_lp::Call::register_emergency_withdrawal_address {
					address,
				});

			let (_, state_chain_client) = StateChainClient::connect_with_account(
				scope,
				&state_chain_settings.ws_endpoint,
				&state_chain_settings.signing_key_file,
				AccountRole::LiquidityProvider,
				false,
			)
			.await?;

			let (tx_hash, ..) = state_chain_client
				.submit_signed_extrinsic(call)
				.await
				.until_finalized()
				.await
				.context("Registration for Emergency Withdrawal address failed.")?;
			Ok(tx_hash)
		}
		.boxed()
	})
	.await
}

pub async fn request_liquidity_deposit_address(
	state_chain_settings: &settings::StateChain,
	asset: Asset,
) -> Result<EncodedAddress> {
	let (events, ..) = connect_submit_and_get_events(
		state_chain_settings,
		pallet_cf_lp::Call::request_liquidity_deposit_address { asset },
		AccountRole::LiquidityProvider,
	)
	.await?;

	Ok(events
		.into_iter()
		.find_map(|event| match event {
			state_chain_runtime::RuntimeEvent::LiquidityProvider(
				pallet_cf_lp::Event::LiquidityDepositAddressReady { deposit_address, .. },
			) => Some(deposit_address),
			_ => None,
		})
		.expect("DepositAddressReady must have been generated"))
}

pub async fn withdraw_asset(
	state_chain_settings: &settings::StateChain,
	amount: AssetAmount,
	asset: Asset,
	destination_address: EncodedAddress,
) -> Result<EgressId> {
	let (events, ..) = connect_submit_and_get_events(
		state_chain_settings,
		pallet_cf_lp::Call::withdraw_asset { amount, asset, destination_address },
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
) -> Result<BTreeMap<Asset, AssetAmount>> {
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

			let balances: Result<BTreeMap<Asset, AssetAmount>> =
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

#[derive(Serialize, Deserialize, Debug)]
pub struct MintRangeOrderReturn {
	assets_debited: SideMap<AssetAmount>,
	collected_fees: SideMap<AssetAmount>,
}

pub async fn mint_range_order(
	state_chain_settings: &settings::StateChain,
	asset: Asset,
	range: Range<Tick>,
	order_size: RangeOrderSize,
) -> Result<MintRangeOrderReturn> {
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
			let (_tx_hash, events, ..) = state_chain_client
				.submit_signed_extrinsic(pallet_cf_pools::Call::collect_and_mint_range_order {
					unstable_asset: asset,
					price_range_in_ticks: range,
					order_size,
				})
				.await
				.until_finalized()
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
					) => Some(MintRangeOrderReturn { assets_debited, collected_fees }),
					_ => None,
				})
				.expect("RangeOrderMinted must have been generated"))
		}
		.boxed()
	})
	.await
}

#[derive(Serialize, Deserialize)]
pub struct BurnRangeOrderReturn {
	assets_credited: SideMap<AssetAmount>,
	collected_fees: SideMap<AssetAmount>,
}

pub async fn burn_range_order(
	state_chain_settings: &settings::StateChain,
	asset: Asset,
	range: Range<Tick>,
	amount: AssetAmount,
) -> Result<BurnRangeOrderReturn> {
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
			// if get_liquidity_at_position(&state_chain_client, asset, range,
			// latest_block_hash) 	.await? < amount
			// {
			// 	bail!("Insufficient minted liquidity at position");
			// }

			// Submit the burn call
			let (_tx_hash, events, ..) = state_chain_client
				.submit_signed_extrinsic(pallet_cf_pools::Call::collect_and_burn_range_order {
					unstable_asset: asset,
					price_range_in_ticks: range,
					liquidity: amount,
				})
				.await
				.until_finalized()
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
					) => Some(BurnRangeOrderReturn { assets_credited, collected_fees }),
					_ => None,
				})
				.expect("RangeOrderBurned must have been generated"))
		}
		.boxed()
	})
	.await
}

#[derive(Serialize, Deserialize)]
pub struct MintLimitOrderReturn {
	assets_debited: AssetAmount,
	collected_fees: AssetAmount,
	swapped_liquidity: AssetAmount,
}

pub async fn mint_limit_order(
	state_chain_settings: &settings::StateChain,
	asset: Asset,
	order: BuyOrSellOrder,
	price: Tick,
	amount: AssetAmount,
) -> Result<MintLimitOrderReturn> {
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
			let (_tx_hash, events, ..) = state_chain_client
				.submit_signed_extrinsic(pallet_cf_pools::Call::collect_and_mint_limit_order {
					unstable_asset: asset,
					order,
					price_as_tick: price,
					amount,
				})
				.await
				.until_finalized()
				.await?;

			// Get some details from the emitted event
			Ok(events
				.into_iter()
				.find_map(|event| match event {
					state_chain_runtime::RuntimeEvent::LiquidityPools(
						pallet_cf_pools::Event::LimitOrderMinted {
							assets_debited,
							collected_fees,
							swapped_liquidity,
							..
						},
					) => Some(MintLimitOrderReturn {
						assets_debited,
						collected_fees,
						swapped_liquidity,
					}),
					_ => None,
				})
				.expect("LimitOrderMinted must have been generated"))
		}
		.boxed()
	})
	.await
}

#[derive(Serialize, Deserialize)]
pub struct BurnLimitOrderReturn {
	assets_credited: AssetAmount,
	collected_fees: AssetAmount,
	swapped_liquidity: AssetAmount,
}

pub async fn burn_limit_order(
	state_chain_settings: &settings::StateChain,
	asset: Asset,
	order: BuyOrSellOrder,
	price: Tick,
	amount: AssetAmount,
) -> Result<BurnLimitOrderReturn> {
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

			// TODO: get limit orders and see if there's enough liquidity to burn

			// Submit the burn order
			let (_tx_hash, events, ..) = state_chain_client
				.submit_signed_extrinsic(pallet_cf_pools::Call::collect_and_burn_limit_order {
					unstable_asset: asset,
					order,
					price_as_tick: price,
					amount,
				})
				.await
				.until_finalized()
				.await?;

			// Get some details from the emitted event
			Ok(events
				.into_iter()
				.find_map(|event| match event {
					state_chain_runtime::RuntimeEvent::LiquidityPools(
						pallet_cf_pools::Event::LimitOrderBurned {
							assets_credited,
							collected_fees,
							swapped_liquidity,
							..
						},
					) => Some(BurnLimitOrderReturn {
						assets_credited,
						collected_fees,
						swapped_liquidity,
					}),
					_ => None,
				})
				.expect("LimitOrderBurned must have been generated"))
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
