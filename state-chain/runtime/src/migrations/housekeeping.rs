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
use crate::{Runtime, VERSION};
#[cfg(feature = "try-runtime")]
use cf_chains::instances::{AssethubInstance, EthereumInstance, SolanaInstance, TronInstance};
use cf_runtime_utilities::genesis_hashes;
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
#[cfg(feature = "try-runtime")]
use pallet_cf_ingress_egress::ScheduledEgressFetchOrTransfer;
use sp_runtime::AccountId32;
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;
use sp_std::vec::Vec;

/// Pending egress queue lengths for every chain the refunds touch.
///
/// All three refund modules append to these queues, so the deltas can only be checked once they
/// have all run — an individual module cannot verify its own in isolation.
#[cfg(feature = "try-runtime")]
fn egress_queue_lengths() -> [u32; 4] {
	[
		ScheduledEgressFetchOrTransfer::<Runtime, EthereumInstance>::decode_len().unwrap_or(0)
			as u32,
		ScheduledEgressFetchOrTransfer::<Runtime, TronInstance>::decode_len().unwrap_or(0) as u32,
		ScheduledEgressFetchOrTransfer::<Runtime, SolanaInstance>::decode_len().unwrap_or(0) as u32,
		ScheduledEgressFetchOrTransfer::<Runtime, AssethubInstance>::decode_len().unwrap_or(0)
			as u32,
	]
}

/// What each chain's queue is expected to grow by, summed across the three modules.
#[cfg(feature = "try-runtime")]
const EXPECTED_EGRESS_DELTAS: [u32; 4] = [
	refunds::ETHEREUM_EGRESSES,
	refunds::TRON_EGRESSES + overcharged_gas::TRON_EGRESSES,
	stuck_channels::SOLANA_EGRESSES,
	stuck_channels::ASSETHUB_EGRESSES,
];

#[cfg(feature = "try-runtime")]
const CHAIN_NAMES: [&str; 4] = ["Ethereum", "Tron", "Solana", "Assethub"];

pub mod liveness_election_state;
pub mod overcharged_gas;
pub mod reap_old_accounts;
pub mod refunds;
pub mod solana_remove_unused_channels_state;
pub mod stuck_channels;

// One-shot gate for the batched refunds. Must equal the runtime's spec_version at the moment the
// migration ships, so a release that forgets to remove it cannot pay the refunds out twice. Update
// this in lock-step with VERSION.spec_version, and remove the constant along with the `refunds` and
// `stuck_channels` modules in the release that follows.
const REFUNDS_SPEC_VERSION: u32 = 2_02_13;

pub type Migration = (
	NetworkSpecificHousekeeping,
	reap_old_accounts::Migration,
	// Can be removed once Solana address re-use is activated.
	solana_remove_unused_channels_state::SolanaRemoveUnusedChannelsState,
	liveness_election_state::LivenessElectionStateMigration,
);

/// Closes all TrxUsdt trading strategies, returning their funds to the owning LPs, and cancels all
/// TrxUsdt/Usdc pool orders. Then takes every LP's free TrxUsdt balance into
/// `TrxUsdtExploitSnapshot`, recording the amount per account. Lending supply is left untouched.
fn snapshot_trx_usdt_balances() {
	use cf_primitives::{Asset, AssetAmount};
	use cf_traits::PoolOrdersManager;
	use frame_support::storage::with_storage_layer;
	use pallet_cf_asset_balances::{FreeBalances, TrxUsdtExploitSnapshot};
	use pallet_cf_trading_strategy::Strategies;
	use sp_std::collections::btree_map::BTreeMap;

	let strategies: Vec<(AccountId32, AccountId32)> = Strategies::<Runtime>::iter()
		.filter_map(|(lp, strategy_id, strategy)| {
			strategy
				.supported_assets()
				.contains(&Asset::TrxUsdt)
				.then_some((lp, strategy_id))
		})
		.collect();
	for (lp, strategy_id) in strategies {
		if let Err(e) = with_storage_layer(|| {
			pallet_cf_trading_strategy::Pallet::<Runtime>::close_strategy_inner(&lp, &strategy_id)
		}) {
			log::error!("🧹 Failed to close TrxUsdt strategy {strategy_id:?}: {e:?}");
		}
	}

	// Returns both assets of every order, including filled amounts and fees, to free balances.
	if let Err(e) = <crate::LiquidityPools as PoolOrdersManager>::cancel_all_pool_orders(
		Asset::TrxUsdt,
		Asset::Usdc,
	) {
		log::error!("🧹 Failed to cancel TrxUsdt pool orders: {e:?}");
	}

	let accounts: Vec<AccountId32> = FreeBalances::<Runtime>::iter()
		.filter_map(|(account, asset, _)| (asset == Asset::TrxUsdt).then_some(account))
		.collect();

	let snapshot: BTreeMap<AccountId32, AssetAmount> = accounts
		.into_iter()
		.filter_map(|account| {
			let amount = FreeBalances::<Runtime>::take(&account, Asset::TrxUsdt);
			if amount > 0 {
				frame_system::Pallet::<Runtime>::deposit_event(crate::RuntimeEvent::AssetBalances(
					pallet_cf_asset_balances::Event::AccountDebited {
						account_id: account.clone(),
						asset: Asset::TrxUsdt,
						amount_debited: amount,
						new_balance: 0,
					},
				));
				Some((account, amount))
			} else {
				None
			}
		})
		.collect();

	log::info!(
		"🧹 Snapshot of {} TrxUsdt across {} accounts.",
		snapshot.values().copied().fold(0, AssetAmount::saturating_add),
		snapshot.len()
	);
	TrxUsdtExploitSnapshot::<Runtime>::put(snapshot);
}

pub struct NetworkSpecificHousekeeping;

impl OnRuntimeUpgrade for NetworkSpecificHousekeeping {
	fn on_runtime_upgrade() -> Weight {
		match genesis_hashes::genesis_hash::<Runtime>() {
			genesis_hashes::BERGHAIN =>
				if VERSION.spec_version == REFUNDS_SPEC_VERSION {
					// Must run first: it credits the recovered channel balances that the refund
					// egresses below are paid out of.
					stuck_channels::Migration::on_runtime_upgrade();
					refunds::Migration::on_runtime_upgrade();
					overcharged_gas::Migration::on_runtime_upgrade();
					log::info!("🧹 Berghain: scheduled batched refunds.");
					snapshot_trx_usdt_balances();
				} else {
					log::info!(
						"🧹 Skipping refunds: spec_version is {} (expected {}).",
						VERSION.spec_version,
						REFUNDS_SPEC_VERSION,
					);
				},
			genesis_hashes::PERSEVERANCE => {
				log::info!("🧹 No housekeeping required for Perseverance.");
			},
			genesis_hashes::SISYPHOS => {
				log::info!("🧹 No housekeeping required for Sisyphos.");
			},
			_ => {},
		}

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		if matches!(genesis_hashes::genesis_hash::<Runtime>(), genesis_hashes::BERGHAIN) &&
			VERSION.spec_version == REFUNDS_SPEC_VERSION
		{
			Ok(egress_queue_lengths().iter().flat_map(|len| len.to_be_bytes()).collect())
		} else {
			Ok(Default::default())
		}
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		use cf_primitives::Asset;
		use pallet_cf_asset_balances::FreeBalances;

		if !matches!(genesis_hashes::genesis_hash::<Runtime>(), genesis_hashes::BERGHAIN) ||
			VERSION.spec_version != REFUNDS_SPEC_VERSION
		{
			return Ok(());
		}

		if state.len() != 16 {
			return Err(DispatchError::Other("bad pre_upgrade state"));
		}

		let after = egress_queue_lengths();
		for (i, chunk) in state.chunks_exact(4).enumerate() {
			let before = u32::from_be_bytes(
				chunk.try_into().map_err(|_| DispatchError::Other("bad pre_upgrade state"))?,
			);
			assert_eq!(
				after[i],
				before + EXPECTED_EGRESS_DELTAS[i],
				"unexpected {} egress queue delta",
				CHAIN_NAMES[i],
			);
		}

		frame_support::ensure!(
			pallet_cf_trading_strategy::Strategies::<Runtime>::iter()
				.all(|(_, _, strategy)| !strategy.supported_assets().contains(&Asset::TrxUsdt)),
			"TrxUsdt trading strategies remain"
		);
		frame_support::ensure!(
			pallet_cf_pools::Pools::<Runtime>::iter().all(|(pair, pool)| {
				pair.assets().base != Asset::TrxUsdt ||
					(pool.range_orders_cache.is_empty() &&
						pool.limit_orders_cache
							.as_ref()
							.into_iter()
							.all(|(_, orders)| orders.is_empty()))
			}),
			"TrxUsdt pool still has open orders"
		);
		frame_support::ensure!(
			FreeBalances::<Runtime>::iter()
				.all(|(_, asset, amount)| asset != Asset::TrxUsdt || amount == 0),
			"TrxUsdt free balance remains"
		);
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use cf_primitives::{Asset, AssetAmount};
	use pallet_cf_asset_balances::{FreeBalances, TrxUsdtExploitSnapshot};
	use pallet_cf_trading_strategy::{Strategies, TradingStrategy};
	use sp_std::collections::btree_map::BTreeMap;

	#[test]
	fn snapshots_all_trx_usdt_and_closes_trx_usdt_strategies() {
		sp_io::TestExternalities::default().execute_with(|| {
			// Events are dropped at block 0.
			frame_system::Pallet::<Runtime>::set_block_number(1);
			let a = AccountId32::from([1u8; 32]);
			let b = AccountId32::from([2u8; 32]);
			let c = AccountId32::from([3u8; 32]);
			let strategy_owner = AccountId32::from([4u8; 32]);
			let trx_usdt_strategy = AccountId32::from([5u8; 32]);
			let btc_strategy = AccountId32::from([6u8; 32]);
			FreeBalances::<Runtime>::insert(&a, Asset::TrxUsdt, 100 as AssetAmount);
			FreeBalances::<Runtime>::insert(&a, Asset::Usdc, 5 as AssetAmount);
			FreeBalances::<Runtime>::insert(&b, Asset::TrxUsdt, 0 as AssetAmount);
			FreeBalances::<Runtime>::insert(&c, Asset::TrxUsdt, 250 as AssetAmount);
			Strategies::<Runtime>::insert(
				&strategy_owner,
				&trx_usdt_strategy,
				TradingStrategy::TickZeroCentered { spread_tick: 1, base_asset: Asset::TrxUsdt },
			);
			FreeBalances::<Runtime>::insert(&trx_usdt_strategy, Asset::TrxUsdt, 40 as AssetAmount);
			FreeBalances::<Runtime>::insert(&trx_usdt_strategy, Asset::Usdc, 9 as AssetAmount);
			let btc = TradingStrategy::TickZeroCentered { spread_tick: 1, base_asset: Asset::Btc };
			Strategies::<Runtime>::insert(&a, &btc_strategy, btc.clone());

			snapshot_trx_usdt_balances();

			// All TrxUsdt is taken.
			for account in [&a, &c, &strategy_owner, &trx_usdt_strategy] {
				assert_eq!(FreeBalances::<Runtime>::get(account, Asset::TrxUsdt), 0);
			}
			assert_eq!(FreeBalances::<Runtime>::get(&a, Asset::Usdc), 5);

			// The TrxUsdt strategy is closed and its funds returned to the owner first.
			assert!(Strategies::<Runtime>::get(&strategy_owner, &trx_usdt_strategy).is_none());
			assert_eq!(FreeBalances::<Runtime>::get(&strategy_owner, Asset::Usdc), 9);
			assert_eq!(FreeBalances::<Runtime>::get(&trx_usdt_strategy, Asset::Usdc), 0);
			assert_eq!(Strategies::<Runtime>::get(&a, &btc_strategy), Some(btc));

			// Every debit is visible as an event.
			let events = frame_system::Pallet::<Runtime>::events();
			for (account, amount) in [(&a, 100), (&c, 250), (&strategy_owner, 40)] {
				assert!(events.iter().any(|record| {
					record.event ==
						crate::RuntimeEvent::AssetBalances(
							pallet_cf_asset_balances::Event::AccountDebited {
								account_id: account.clone(),
								asset: Asset::TrxUsdt,
								amount_debited: amount,
								new_balance: 0,
							},
						)
				}));
			}

			assert_eq!(
				TrxUsdtExploitSnapshot::<Runtime>::get(),
				BTreeMap::from([(a, 100), (c, 250), (strategy_owner, 40)])
			);
		});
	}
}
