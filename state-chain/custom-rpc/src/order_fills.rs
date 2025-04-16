// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;

use super::*;

use cf_primitives::AccountId;
use cf_rpc_apis::{OrderFilled, OrderFills};
use pallet_cf_pools::{AssetPair, OrderId, Pool};
use state_chain_runtime::Runtime;

pub(crate) fn order_fills_for_block<C, B, BE>(
	client: &C,
	hash: Hash,
) -> RpcResult<BlockUpdate<OrderFills>>
where
	B: BlockT<Hash = Hash, Header = state_chain_runtime::Header>,
	B::Header: Unpin,
	BE: Send + Sync + 'static + Backend<B>,
	C: sp_api::ProvideRuntimeApi<B>
		+ Send
		+ Sync
		+ 'static
		+ BlockBackend<B>
		+ ExecutorProvider<B>
		+ HeaderBackend<B>
		+ HeaderMetadata<B, Error = sc_client_api::blockchain::Error>
		+ BlockchainEvents<B>
		+ CallApiAt<B>
		+ StorageProvider<B, BE>,
	C::Api: CustomRuntimeApi<B>,
{
	let header = client
		.header(hash)
		.map_err(|e| call_error(e, CfErrorCode::OtherError))?
		.ok_or_else(|| {
			internal_error(format!("Could not fetch block header for block {:?}", hash))
		})?;

	let pools = StorageQueryApi::new(client)
		.collect_from_storage_map::<pallet_cf_pools::Pools<_>, _, _, _>(hash)?;

	let prev_pools = StorageQueryApi::new(client)
		.collect_from_storage_map::<pallet_cf_pools::Pools<_>, _, _, _>(header.parent_hash)?;

	let lp_events = client.runtime_api().cf_lp_events(hash).map_err(CfApiError::from)?;

	Ok(BlockUpdate::<OrderFills> {
		block_hash: hash,
		block_number: header.number,
		data: order_fills_from_block_updates(&prev_pools, &pools, lp_events),
	})
}

fn order_fills_for_pool<'a>(
	asset_pair: &'a AssetPair,
	pool: &'a Pool<Runtime>,
	previous_pool: Option<&'a Pool<Runtime>>,
	updated_range_orders: &'a HashSet<(AccountId, AssetPair, OrderId)>,
	updated_limit_orders: &'a HashSet<(AccountId, AssetPair, Side, OrderId)>,
) -> impl IntoIterator<Item = OrderFilled> + 'a {
	[Side::Sell, Side::Buy]
		.into_iter()
		.flat_map(move |side| {
			pool.pool_state.limit_orders(side).filter_map(
				move |((lp, id), tick, collected, position_info)| {
					let (fees, sold, bought) = {
						let option_previous_order_state = if updated_limit_orders.contains(&(
							lp.clone(),
							*asset_pair,
							side,
							id,
						)) {
							None
						} else {
							previous_pool.and_then(|pool| {
								pool.pool_state.limit_order(&(lp.clone(), id), side, tick).ok()
							})
						};

						if let Some((previous_collected, _)) = option_previous_order_state {
							(
								collected.fees - previous_collected.fees,
								collected
									.sold_amount
									.checked_sub(previous_collected.sold_amount)
									.unwrap_or_else(|| {
										log::info!(
															"Ignored dust sold_amount underflow. Current: {}, Previous: {}",
															collected.sold_amount,
															previous_collected.sold_amount
														);
										0.into()
									}),
								collected.bought_amount - previous_collected.bought_amount,
							)
						} else {
							(collected.fees, collected.sold_amount, collected.bought_amount)
						}
					};

					if fees.is_zero() && sold.is_zero() && bought.is_zero() {
						None
					} else {
						Some(OrderFilled::LimitOrder {
							lp,
							base_asset: asset_pair.assets().base,
							quote_asset: asset_pair.assets().quote,
							side,
							id: id.into(),
							tick,
							sold,
							bought,
							fees,
							remaining: position_info.amount,
						})
					}
				},
			)
		})
		.chain(pool.pool_state.range_orders().filter_map(
			move |((lp, id), range, collected, position_info)| {
				let fees = {
					let option_previous_order_state =
						if updated_range_orders.contains(&(lp.clone(), *asset_pair, id)) {
							None
						} else {
							previous_pool.and_then(|pool| {
								pool.pool_state.range_order(&(lp.clone(), id), range.clone()).ok()
							})
						};

					if let Some((previous_collected, _)) = option_previous_order_state {
						collected
							.fees
							.zip(previous_collected.fees)
							.map(|(fees, previous_fees)| fees - previous_fees)
					} else {
						collected.fees
					}
				};

				if fees == Default::default() {
					None
				} else {
					Some(OrderFilled::RangeOrder {
						lp: lp.clone(),
						base_asset: asset_pair.assets().base,
						quote_asset: asset_pair.assets().quote,
						id: id.into(),
						range: range.clone(),
						fees: fees.map(|fees| fees),
						liquidity: position_info.liquidity.into(),
					})
				}
			},
		))
}

fn order_fills_from_block_updates(
	previous_pools: &BTreeMap<AssetPair, Pool<Runtime>>,
	pools: &BTreeMap<AssetPair, Pool<Runtime>>,
	events: Vec<pallet_cf_pools::Event<Runtime>>,
) -> OrderFills {
	let updated_range_orders = events
		.iter()
		.filter_map(|event| match event {
			pallet_cf_pools::Event::RangeOrderUpdated {
				lp, base_asset, quote_asset, id, ..
			} => Some((lp.clone(), AssetPair::new(*base_asset, *quote_asset).unwrap(), *id)),
			_ => None,
		})
		.collect::<HashSet<_>>();

	let updated_limit_orders = events
		.iter()
		.filter_map(|event| match event {
			pallet_cf_pools::Event::LimitOrderUpdated {
				lp,
				base_asset,
				quote_asset,
				side,
				id,
				..
			} => Some((lp.clone(), AssetPair::new(*base_asset, *quote_asset).unwrap(), *side, *id)),
			_ => None,
		})
		.collect::<HashSet<_>>();

	let order_fills = pools
		.iter()
		.flat_map(|(asset_pair, pool)| {
			order_fills_for_pool(
				asset_pair,
				pool,
				previous_pools.get(asset_pair),
				&updated_range_orders,
				&updated_limit_orders,
			)
		})
		.collect::<Vec<_>>();

	OrderFills { fills: order_fills }
}
