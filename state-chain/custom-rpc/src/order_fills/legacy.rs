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

//! Order fills from blocks whose pools predate the removal of fixed pools.
//!
//! Such a block holds its pools in a shape the current `Pool` cannot decode, and has no
//! `LimitOrderFilled` events to read fills off: back then a pool did not report a fill, it only
//! recorded, per order, how much of it had been bought since its lp last collected. A fill was the
//! difference between that and what the previous block recorded. Both the old storage shape and
//! that subtraction are reproduced here.
//!
//! Range orders are untouched by all this — the upgrade left them alone — so [Pool::split] hands
//! them straight back to the caller's usual path.

use super::*;

use cf_amm::{
	common::{Pairs, PoolPairsMap},
	limit_orders::legacy_support::{CollectedBefore, PoolStateV10},
	math::{Amount, Tick},
	range_orders, PoolState,
};
use codec::{Decode, Encode};
use frame_support::{pallet_prelude::OptionQuery, traits::StorageVersion, Twox64Concat};
use std::ops::Range;

/// An order's owner, as the amm knows it.
type LpId = (AccountId, OrderId);

/// The pools storage version from which limit orders are held in the current shape (fixed pool
/// removal). Anything below it is read by this module.
const LIMIT_ORDERS_RESHAPED: u16 = 11;

/// Whether the pools at this block predate the reshaping, and so have to be read here.
pub(super) fn pools_are_legacy(version: StorageVersion) -> bool {
	version < StorageVersion::new(LIMIT_ORDERS_RESHAPED)
}

/// The pallet's `Pools` as it was. Same key and hasher; only the value's limit orders differ.
#[frame_support::storage_alias]
pub type Pools = StorageMap<LiquidityPools, Twox64Concat, AssetPair, Pool, OptionQuery>;

/// The pallet's `Pool` as it was. Only `pool_state.limit_orders` differs from the current shape,
/// but every field has to be spelled out for the value to decode.
#[derive(Encode, Decode)]
pub struct Pool {
	pub range_orders_cache: BTreeMap<AccountId, BTreeMap<OrderId, Range<Tick>>>,
	pub limit_orders_cache: PoolPairsMap<BTreeMap<AccountId, BTreeMap<OrderId, Tick>>>,
	pub pool_state: AmmPoolState,
}

#[derive(Encode, Decode)]
pub struct AmmPoolState {
	pub limit_orders: PoolStateV10<LpId>,
	pub range_orders: range_orders::PoolState<LpId>,
}

impl Pool {
	/// Splits the pool into the two halves fills are worked out from: its range orders, as the
	/// current representation holds them, and its limit orders, as the old one did.
	///
	/// The range order half comes back as a whole [PoolState] because that is what reports range
	/// order fills, and it is the one piece of it the upgrade did not touch. Its limit orders are
	/// left empty rather than converted: the old ones cannot be expressed there, and nothing reads
	/// them off this value.
	pub(super) fn split(self) -> (PoolState<LpId>, PoolStateV10<LpId>) {
		(
			PoolState {
				limit_orders: Default::default(),
				range_orders: self.pool_state.range_orders,
			},
			self.pool_state.limit_orders,
		)
	}
}

/// The limit order fills of a block, as the difference between what each order had collected by
/// the end of it and what it had collected by the end of the previous block.
pub(super) fn limit_order_fills(
	previous_pools: &BTreeMap<AssetPair, PoolStateV10<LpId>>,
	pools: &BTreeMap<AssetPair, PoolStateV10<LpId>>,
	events: &[pallet_cf_pools::Event<Runtime>],
) -> Vec<OrderFilled> {
	// An order its lp updated this block had its earnings collected and paid out as part of the
	// update, so what it records now cannot be held against the previous block.
	let updated_orders = events
		.iter()
		.filter_map(|event| match event {
			pallet_cf_pools::Event::LimitOrderUpdated {
				lp,
				base_asset,
				quote_asset,
				side,
				id,
				..
			} => Some((lp.clone(), AssetPair::new(*base_asset, *quote_asset)?, *side, *id)),
			_ => None,
		})
		.collect::<HashSet<_>>();

	let mut fills = Vec::new();

	for (asset_pair, limit_orders) in pools {
		// A pool missing from the previous block yields no difference to report. Strictly it is
		// possible for a pool to be filled in the first block of its existence, but not in
		// practice.
		let Some(previous_limit_orders) = previous_pools.get(asset_pair) else { continue };
		let previous_collected = collected_by_order(previous_limit_orders);

		let collected = limit_orders.collected();
		for sold_pair in [Pairs::Base, Pairs::Quote] {
			let side = sold_pair.sell_order();

			for order in &collected[sold_pair] {
				let (lp, id) = &order.lp;

				let previous = (!updated_orders.contains(&(lp.clone(), *asset_pair, side, *id)))
					.then(|| previous_collected[sold_pair].get(&(order.tick, order.lp.clone())))
					.flatten();

				let (sold, bought) = match previous {
					Some(previous) => (
						order.sold_amount.checked_sub(previous.sold_amount).unwrap_or_else(|| {
							log::info!(
								"Ignored dust sold_amount underflow. Current: {}, Previous: {}",
								order.sold_amount,
								previous.sold_amount
							);
							Amount::zero()
						}),
						// Saturating for the same reason: rounding can leave the two a dust
						// amount apart, and an rpc must not panic over it.
						order.bought_amount.saturating_sub(previous.bought_amount),
					),
					None => (order.sold_amount, order.bought_amount),
				};

				if sold.is_zero() && bought.is_zero() {
					continue
				}

				fills.push(OrderFilled::LimitOrder {
					lp: lp.clone(),
					base_asset: asset_pair.base(),
					quote_asset: asset_pair.quote(),
					side,
					id: (*id).into(),
					tick: order.tick,
					sold,
					bought,
					fees: Default::default(),
					remaining: order.remaining_amount,
				});
			}
		}
	}

	fills
}

/// Every order's collected amounts, laid out for lookup by the pair it sells, its price, and whose
/// it is.
fn collected_by_order(
	limit_orders: &PoolStateV10<LpId>,
) -> PoolPairsMap<BTreeMap<(Tick, LpId), CollectedBefore<LpId>>> {
	limit_orders.collected().map(|orders| {
		orders
			.into_iter()
			.map(|order| ((order.tick, order.lp.clone()), order))
			.collect()
	})
}

#[cfg(test)]
mod tests {
	use super::*;

	use cf_amm::{
		limit_orders::legacy_support::{FixedPool, FloatBetweenZeroAndOne, PositionV10},
		math::SqrtPrice,
	};
	use cf_primitives::Asset;
	use cf_utilities::assert_matches;

	const ORDER_ID: OrderId = 7;
	const TICK: Tick = 0;

	fn lp(id: u8) -> AccountId {
		AccountId::new([id; 32])
	}

	fn asset_pair() -> AssetPair {
		AssetPair::new(Asset::Eth, Asset::Usdc).unwrap()
	}

	/// A pool holding one sell order of `amount`, of which `numerator / denominator` is still
	/// unsold. Tick zero, so a price of one, which makes what the order bought equal to what it
	/// sold and keeps the arithmetic under test visible.
	fn pool_with_partly_filled_order(
		owner: AccountId,
		amount: u128,
		(numerator, denominator): (u128, u128),
	) -> PoolStateV10<LpId> {
		let sqrt_price = SqrtPrice::from_tick(TICK);
		let remaining =
			FloatBetweenZeroAndOne::max().mul_div_ceil(numerator.into(), denominator.into());

		PoolStateV10::from_parts(
			PoolPairsMap {
				base: [(sqrt_price, FixedPool::from_parts(0, amount.into(), remaining))].into(),
				quote: Default::default(),
			},
			PoolPairsMap {
				base: [(
					(sqrt_price, (owner, ORDER_ID)),
					PositionV10::from_parts(
						0,
						amount.into(),
						FloatBetweenZeroAndOne::max(),
						amount.into(),
					),
				)]
				.into(),
				quote: Default::default(),
			},
		)
	}

	fn fills(
		previous: PoolStateV10<LpId>,
		current: PoolStateV10<LpId>,
		events: &[pallet_cf_pools::Event<Runtime>],
	) -> Vec<OrderFilled> {
		limit_order_fills(
			&[(asset_pair(), previous)].into(),
			&[(asset_pair(), current)].into(),
			events,
		)
	}

	/// The old storage shape is read back out from under the live pallet's keys, so the alias has
	/// to hash to exactly what the pallet does. Getting this wrong reads nothing rather than
	/// failing.
	#[test]
	fn legacy_alias_addresses_the_pallets_storage() {
		assert_eq!(
			Pools::hashed_key_for(asset_pair()),
			pallet_cf_pools::Pools::<Runtime>::hashed_key_for(asset_pair()),
		);
	}

	/// A fill is what the order collected this block less what it had collected by the end of the
	/// previous one. Half the order is bought over the block, but the old representation rounds
	/// an order's remainder up, so each block's collected amount sits a unit below the round
	/// number and the difference a unit above it.
	#[test]
	fn fill_is_the_difference_between_the_two_blocks() {
		assert_eq!(
			fills(
				pool_with_partly_filled_order(lp(1), 1_000, (3, 4)),
				pool_with_partly_filled_order(lp(1), 1_000, (1, 4)),
				&[],
			),
			vec![OrderFilled::LimitOrder {
				lp: lp(1),
				base_asset: Asset::Eth,
				quote_asset: Asset::Usdc,
				side: Side::Sell,
				id: ORDER_ID.into(),
				tick: TICK,
				sold: 501.into(),
				bought: 501.into(),
				fees: Default::default(),
				remaining: 250.into(),
			}],
		);
	}

	#[test]
	fn an_untouched_order_has_no_fill() {
		let unchanged = || pool_with_partly_filled_order(lp(1), 1_000, (3, 4));
		assert_eq!(fills(unchanged(), unchanged(), &[]), vec![]);
	}

	/// An order its lp updated this block was collected as part of the update, so what it records
	/// afterwards stands on its own rather than against the previous block.
	#[test]
	fn an_updated_order_is_not_held_against_the_previous_block() {
		let updated = pallet_cf_pools::Event::LimitOrderUpdated {
			lp: lp(1),
			base_asset: Asset::Eth,
			quote_asset: Asset::Usdc,
			side: Side::Sell,
			id: ORDER_ID,
			tick: TICK,
			sell_amount_change: None,
			sell_amount_total: 0,
			collected_fees: 0,
			bought_amount: 0,
		};

		assert_matches!(
			fills(
				pool_with_partly_filled_order(lp(1), 1_000, (3, 4)),
				pool_with_partly_filled_order(lp(1), 1_000, (1, 4)),
				&[updated],
			)
			.as_slice(),
			[OrderFilled::LimitOrder { sold, .. }] if *sold == 750.into(),
		);
	}

	/// Checks this module against real mainnet blocks: their pool storage is read the way it reads
	/// it, and the fills worked out from that are held against what the archive node itself serves
	/// for the same block — a node running the code this module replaces.
	///
	/// Ignored by default because it needs the archive node. Run it by hand with:
	///
	/// ```sh
	/// cargo nextest run -p custom-rpc --run-ignored all online_test_order_fills
	/// ```
	///
	/// `CF_ORDER_FILLS_BLOCKS` pins the blocks to a comma-separated list instead of scanning back
	/// from the finalized head, which is how to reach pre-upgrade blocks once mainnet has upgraded
	/// past them. `CF_ARCHIVE_NODE` points it at a different node.
	mod online_tests {
		use super::*;

		use frame_support::storage::StoragePrefixedMap;
		use serde_json::{json, Value};

		const NODE_URL: &str = "https://mainnet-archive.chainflip.io";
		/// How many blocks that have fills to check.
		const BLOCKS_TO_CHECK: usize = 10;
		/// How far back to look for them before giving up.
		const MAX_BLOCKS_SCANNED: u32 = 500;

		struct Node {
			client: reqwest::blocking::Client,
			url: String,
		}

		impl Node {
			fn call(&self, method: &str, params: Value) -> Value {
				let body: Value = self
					.client
					.post(&self.url)
					.json(&json!({ "id": 1, "jsonrpc": "2.0", "method": method, "params": params }))
					.send()
					.unwrap_or_else(|e| panic!("{method} request failed: {e}"))
					.json()
					.unwrap_or_else(|e| panic!("{method} response is not json: {e}"));

				assert!(body.get("error").is_none(), "{method} returned {}", body["error"]);
				body["result"].clone()
			}

			/// A `0x`-prefixed result as bytes. `None` where the storage entry is empty.
			fn bytes(&self, method: &str, params: Value) -> Option<Vec<u8>> {
				let result = self.call(method, params);
				let hex = result.as_str()?;
				Some(hex::decode(hex.trim_start_matches("0x")).expect("result is hex"))
			}
		}

		fn hex_key(key: impl AsRef<[u8]>) -> String {
			format!("0x{}", hex::encode(key))
		}

		/// The fills the node itself reports for a block.
		fn served_fills(node: &Node, hash: &str) -> Vec<OrderFilled> {
			serde_json::from_value(
				node.call("cf_lp_get_order_fills", json!([hash]))["fills"].clone(),
			)
			.expect("served fills parse")
		}

		/// Every pool at a block, read in the shape that predates the removal of fixed pools.
		///
		/// A pool is identified by the `AssetPair` bytes at the tail of its storage key rather than
		/// by the asset names the rpc reports, which do not map onto enum variants one to one:
		/// `USDT` alone does not say whether it is Ethereum's or Tron's.
		fn pools_at(node: &Node, hash: &str) -> BTreeMap<AssetPair, Pool> {
			let prefix = hex_key(pallet_cf_pools::Pools::<Runtime>::final_prefix());
			let keys = node.call("state_getKeysPaged", json!([prefix, 100, prefix, hash]));

			keys.as_array()
				.expect("pool keys")
				.iter()
				.filter_map(|key| {
					let key = key.as_str().unwrap();
					// Past the map prefix and the twox64 half of the key's Twox64Concat hash.
					let raw_pair = hex::decode(&key[2 + 64 + 16..]).expect("key is hex");
					let pair =
						AssetPair::decode(&mut &raw_pair[..]).expect("key holds an asset pair");

					let bytes = node.bytes("state_getStorage", json!([key, hash]))?;
					let mut slice = &bytes[..];
					let pool = Pool::decode(&mut slice)
						.unwrap_or_else(|e| panic!("{pair:?} at {hash} failed to decode: {e:?}"));
					assert!(slice.is_empty(), "{pair:?} at {hash} left {} bytes", slice.len());

					Some((pair, pool))
				})
				.collect()
		}

		/// Whether a block's pools predate the removal, and so belong to this module at all.
		fn pools_are_legacy_at(node: &Node, hash: &str) -> bool {
			let key = hex_key(StorageVersion::storage_key::<pallet_cf_pools::Pallet<Runtime>>());
			let version = node
				.bytes("state_getStorage", json!([key, hash]))
				.map(|bytes| {
					StorageVersion::decode(&mut &bytes[..]).expect("storage version decodes")
				})
				.unwrap_or_default();

			pools_are_legacy(version)
		}

		/// A block's fills are a set, and the two sides disagree on order: the rpc reports every
		/// limit order fill before every range order fill, where the node groups them by pool.
		fn sorted(fills: Vec<OrderFilled>) -> Vec<String> {
			let mut fills = fills
				.iter()
				.map(|fill| serde_json::to_value(fill).unwrap().to_string())
				.collect::<Vec<_>>();
			fills.sort();
			fills
		}

		/// The blocks to look at, and how many of them are expected to have fills. Every pinned
		/// block has to; of a scanned range only [BLOCKS_TO_CHECK] do, the rest being passed over
		/// because a block with no fills at all says nothing either way.
		fn blocks_to_check(node: &Node) -> (Vec<u32>, usize) {
			match std::env::var("CF_ORDER_FILLS_BLOCKS") {
				Ok(blocks) => {
					let blocks = blocks
						.split(',')
						.map(|block| {
							block.trim().parse().expect("CF_ORDER_FILLS_BLOCKS holds numbers")
						})
						.collect::<Vec<u32>>();
					let pinned = blocks.len();
					(blocks, pinned)
				},
				Err(_) => {
					let head = node.call("chain_getFinalizedHead", json!([]));
					let head = node.call("chain_getHeader", json!([head]));
					let head = u32::from_str_radix(
						head["number"].as_str().expect("head number").trim_start_matches("0x"),
						16,
					)
					.expect("head number is hex");

					(
						(head.saturating_sub(MAX_BLOCKS_SCANNED)..=head).rev().collect(),
						BLOCKS_TO_CHECK,
					)
				},
			}
		}

		#[ignore = "requires access to archive node"]
		#[test]
		fn online_test_order_fills_for_pre_upgrade_blocks() {
			// The fills the node serves spell lps in ss58.
			cf_primitives::use_chainflip_account_id_encoding();

			let node = Node {
				client: reqwest::blocking::Client::new(),
				url: std::env::var("CF_ARCHIVE_NODE").unwrap_or_else(|_| NODE_URL.to_string()),
			};

			let (blocks, required) = blocks_to_check(&node);
			let (mut checked, mut fills_checked, mut pools_read) = (0usize, 0usize, 0usize);
			for number in blocks {
				if checked == required {
					break
				}

				let hash = node.call("chain_getBlockHash", json!([number]));
				let hash = hash.as_str().expect("block hash").to_string();

				let expected = served_fills(&node, &hash);
				if expected.is_empty() {
					continue
				}
				assert!(
				pools_are_legacy_at(&node, &hash),
				"block #{number} is past the removal of fixed pools, so it doesn't exercise this \
				 module; pin pre-upgrade blocks with CF_ORDER_FILLS_BLOCKS",
			);

				let parent = node.call("chain_getHeader", json!([hash]))["parentHash"]
					.as_str()
					.expect("parent hash")
					.to_string();

				let pools = pools_at(&node, &hash);
				pools_read += pools.len();
				let previous_pools = pools_at(&node, &parent);

				let lp_events = node
					.bytes("state_call", json!(["CustomRuntimeApi_cf_lp_events", "0x", hash]))
					.expect("lp events");

				let fills = order_fills_from_block_updates(
					&PoolsAtBlock::legacy(previous_pools),
					&PoolsAtBlock::legacy(pools),
					Vec::<pallet_cf_pools::Event<Runtime>>::decode(&mut &lp_events[..])
						.expect("lp events decode"),
				);

				assert_eq!(
					sorted(fills.fills),
					sorted(expected.clone()),
					"mismatch at block #{number}"
				);
				checked += 1;
				fills_checked += expected.len();
			}

			assert_eq!(
				checked, required,
				"only {checked} of the blocks looked at had fills to check"
			);
			println!("{checked} blocks, {pools_read} pools read, {fills_checked} fills matched");
		}
	}
}
