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

use cf_amm::{common::AssetPair, input_amount_from_fee};
use cf_primitives::{AccountId, OrderId};
use cf_rpc_apis::{OrderFilled, OrderFills};
use pallet_cf_pools::Pool;
use state_chain_runtime::{chainflip::get_header_timestamp, Runtime};

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

	let pools: BTreeMap<_, _> = StorageQueryApi::new(client)
		.collect_from_storage_map::<pallet_cf_pools::Pools<Runtime>, _, _, _>(hash)?;

	let prev_pools: BTreeMap<_, _> = StorageQueryApi::new(client)
		.collect_from_storage_map::<pallet_cf_pools::Pools<Runtime>, _, _, _>(header.parent_hash)?;

	// Pools present now but missing from the previous block can't yield a fill delta, so they're
	// skipped below. Log why rather than dropping them silently: across a runtime upgrade the
	// parent block's pool storage may not decode under the current `Pool` type.
	let new_or_undecodable_pools = pools
		.keys()
		.filter(|pair| !prev_pools.contains_key(*pair))
		.copied()
		.collect::<Vec<_>>();
	if !new_or_undecodable_pools.is_empty() {
		// Key present in the parent but value missing from `prev_pools` => decode failure;
		// key absent => pool newly created. Only scan keys in this rare branch.
		let prev_pool_keys = StorageQueryApi::new(client)
			.collect_keys_from_storage_map::<pallet_cf_pools::Pools<Runtime>, _, _, HashSet<_>>(
				header.parent_hash,
			)?;
		for pair in new_or_undecodable_pools {
			if prev_pool_keys.contains(&pair) {
				log::warn!(
					"order_fills: previous pool state for {pair:?} failed to decode at block #{} \
					 ({hash:?}); skipping its order fills for this block.",
					header.number,
				);
			} else {
				// Strictly speaking it might be logically possible that there are fills in the
				// first block of a pool's existence, but is so unlikely that we can assume it won't
				// happen in practice.
				log::info!("order_fills: pool {pair:?} newly created at block #{}.", header.number,);
			}
		}
	}

	let lp_events = client.runtime_api().cf_lp_events(hash).map_err(CfApiError::from)?;

	Ok(BlockUpdate::<OrderFills> {
		block_hash: hash,
		block_number: header.number,
		timestamp: get_header_timestamp(&header).unwrap_or_default(),
		data: order_fills_from_block_updates(&prev_pools, &pools, lp_events),
	})
}

/// Limit order fills are reported by the pool as they happen, since a swap pays the proceeds out
/// there and then rather than leaving them on the order to be collected.
fn limit_order_fills_from_events(
	events: &[pallet_cf_pools::Event<Runtime>],
) -> impl Iterator<Item = OrderFilled> + '_ {
	events.iter().filter_map(|event| match event {
		pallet_cf_pools::Event::LimitOrderFilled {
			lp,
			base_asset,
			quote_asset,
			side,
			id,
			tick,
			sold_amount,
			bought_amount,
			remaining_amount,
		} => Some(OrderFilled::LimitOrder {
			lp: lp.clone(),
			base_asset: *base_asset,
			quote_asset: *quote_asset,
			side: *side,
			id: (*id).into(),
			tick: *tick,
			sold: (*sold_amount).into(),
			bought: (*bought_amount).into(),
			fees: Default::default(),
			remaining: (*remaining_amount).into(),
		}),
		_ => None,
	})
}

fn range_order_fills_for_pool<'a>(
	asset_pair: &'a AssetPair,
	pool: &'a Pool<Runtime>,
	previous_pool: &'a Pool<Runtime>,
	updated_range_orders: &'a HashSet<(AccountId, AssetPair, OrderId)>,
) -> impl IntoIterator<Item = OrderFilled> + 'a {
	pool.pool_state
		.range_orders()
		.filter_map(move |((lp, id), range, collected, position_info)| {
			let fees = {
				let option_previous_order_state =
					if updated_range_orders.contains(&(lp.clone(), *asset_pair, id)) {
						None
					} else {
						previous_pool.pool_state.range_order(&(lp.clone(), id), range.clone()).ok()
					};

				if let Some((previous_collected, _)) = option_previous_order_state {
					collected
						.fees
						.zip(previous_collected.fees)
						.map(|(fees, previous_fees)| fees.overflowing_sub(previous_fees).0)
				} else {
					Default::default()
				}
			};

			let fee_hundredth_pips = pool.pool_state.range_order_fee();

			if fees == Default::default() {
				None
			} else {
				Some(OrderFilled::RangeOrder {
					lp: lp.clone(),
					base_asset: asset_pair.base(),
					quote_asset: asset_pair.quote(),
					id: id.into(),
					bought_amounts: fees.map(|amount| {
						input_amount_from_fee(amount, fee_hundredth_pips).unwrap_or_default()
					}),
					range: range.clone(),
					fees: fees.map(|fees| fees),
					liquidity: position_info.liquidity.into(),
				})
			}
		})
}

pub fn order_fills_from_block_updates(
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

	let order_fills = limit_order_fills_from_events(&events)
		.chain(
			pools
				.iter()
				.filter_map(|(asset_pair, pool)| {
					Some((asset_pair, pool, previous_pools.get(asset_pair)?))
				})
				.flat_map(|(asset_pair, pool, previous_pool)| {
					range_order_fills_for_pool(
						asset_pair,
						pool,
						previous_pool,
						&updated_range_orders,
					)
				}),
		)
		.collect::<Vec<_>>();

	OrderFills { fills: order_fills }
}
