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
use crate::Runtime;
use cf_primitives::{Asset, AssetAmount};
use cf_runtime_utilities::genesis_hashes;
use cf_traits::BalanceApi;
use frame_support::{pallet_prelude::ValueQuery, traits::OnRuntimeUpgrade, weights::Weight};
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;
use sp_runtime::{helpers_128bit::multiply_by_rational_with_rounding, Rounding};
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

pub mod liveness_election_state;
pub mod reap_old_accounts;
pub mod solana_remove_unused_channels_state;

pub type Migration = (
	NetworkSpecificHousekeeping,
	reap_old_accounts::Migration,
	// Can be removed once Solana address re-use is activated.
	solana_remove_unused_channels_state::SolanaRemoveUnusedChannelsState,
	liveness_election_state::LivenessElectionStateMigration,
);

/// The pallet's own declaration was removed in this release, so the migration reads the
/// leftover data through an alias. Remove it once the migration is dropped.
#[frame_support::storage_alias]
pub type TrxUsdtExploitSnapshot<T: pallet_cf_asset_balances::Config> = StorageValue<
	pallet_cf_asset_balances::Pallet<T>,
	sp_std::collections::btree_map::BTreeMap<<T as frame_system::Config>::AccountId, AssetAmount>,
	ValueQuery,
>;

/// The trxUSDT left in the vault once the exploit was contained, in the asset's six decimals:
/// 10724.384418 trxUSDT. That is the vault balance immediately after the 2.2.13 refund egresses
/// went out, at Tron block 86270197, and before any deposit made since.
const REMAINING_TRX_USDT: AssetAmount = 10_724_384_418;

/// Distributes the trxUSDT taken out of circulation by the 2.2.13 housekeeping.
///
/// `TrxUsdtExploitSnapshot` records what each account's free trxUSDT balance was when it was
/// taken. What is left of the trxUSDT is distributed pro rata, and the rest of each recorded
/// balance is credited in USDC: both are six-decimal USD stablecoins, so the amounts carry over
/// one for one.
///
/// Takes the snapshot rather than reading it, so the storage entry is deleted as it is credited
/// and a repeat run is a no-op.
fn distribute_trx_usdt_snapshot() {
	let snapshot = TrxUsdtExploitSnapshot::<Runtime>::take();
	let total = snapshot.values().copied().fold(0, AssetAmount::saturating_add);
	if total == 0 {
		return;
	}

	// Defensive: Capped so that no account can be credited more trxUSDT than its recorded
	// balance.
	let distributable = core::cmp::min(REMAINING_TRX_USDT, total);

	log::info!(
		"🧹 Distributing {} trxUSDT across {} accounts: {} pro rata in trxUSDT, the rest in USDC.",
		total,
		snapshot.len(),
		distributable,
	);

	for (account, balance) in snapshot {
		// Rounded down, so the shares can never add up to more trxUSDT than is actually held.
		// `unwrap_or_default` cannot be hit: `total` is non-zero and the product of two asset
		// amounts fits in a u128. Crediting the full balance in USDC would be the safe outcome if
		// it ever were.
		let in_trx_usdt =
			multiply_by_rational_with_rounding(balance, distributable, total, Rounding::Down)
				.unwrap_or_default();

		for (asset, amount) in
			[(Asset::TrxUsdt, in_trx_usdt), (Asset::Usdc, balance.saturating_sub(in_trx_usdt))]
		{
			// Skipped for zero amounts, and emits an `AccountCredited` event for the rest.
			pallet_cf_asset_balances::Pallet::<Runtime>::credit_account(&account, asset, amount);
		}
	}
}

pub struct NetworkSpecificHousekeeping;

impl OnRuntimeUpgrade for NetworkSpecificHousekeeping {
	fn on_runtime_upgrade() -> Weight {
		match genesis_hashes::genesis_hash::<Runtime>() {
			genesis_hashes::BERGHAIN => {
				distribute_trx_usdt_snapshot();
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
		use codec::Encode;
		use pallet_cf_asset_balances::FreeBalances;

		if !matches!(genesis_hashes::genesis_hash::<Runtime>(), genesis_hashes::BERGHAIN) {
			return Ok(Default::default());
		}

		// Each account in the snapshot, its recorded balance, and the balances it already holds in
		// both assets.
		Ok(TrxUsdtExploitSnapshot::<Runtime>::get()
			.into_iter()
			.map(|(account, balance)| {
				let trx_usdt = FreeBalances::<Runtime>::get(&account, Asset::TrxUsdt);
				let usdc = FreeBalances::<Runtime>::get(&account, Asset::Usdc);
				(account, balance, trx_usdt, usdc)
			})
			.collect::<Vec<_>>()
			.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
		use codec::Decode;
		use pallet_cf_asset_balances::FreeBalances;
		use sp_runtime::AccountId32;

		if !matches!(genesis_hashes::genesis_hash::<Runtime>(), genesis_hashes::BERGHAIN) {
			return Ok(());
		}

		let before =
			Vec::<(AccountId32, AssetAmount, AssetAmount, AssetAmount)>::decode(&mut &state[..])
				.map_err(|_| DispatchError::Other("bad pre_upgrade state"))?;

		let accounts = before.len();
		let (mut trx_usdt_credited, mut usdc_credited) = (0 as AssetAmount, 0 as AssetAmount);

		log::info!("🧹 trxUSDT exploit snapshot distribution, balances given as trxUSDT / USDC:");
		for (account, balance, trx_usdt_before, usdc_before) in before {
			let trx_usdt_after = FreeBalances::<Runtime>::get(&account, Asset::TrxUsdt);
			let usdc_after = FreeBalances::<Runtime>::get(&account, Asset::Usdc);
			let in_trx_usdt = trx_usdt_after.saturating_sub(trx_usdt_before);
			let in_usdc = usdc_after.saturating_sub(usdc_before);

			log::info!(
				"🧹   {:?} recorded {}: credited {} + {}, {} / {} -> {} / {}",
				account,
				balance,
				in_trx_usdt,
				in_usdc,
				trx_usdt_before,
				usdc_before,
				trx_usdt_after,
				usdc_after,
			);

			// Whichever way the balance was split between the two assets, it is credited in full.
			assert_eq!(
				trx_usdt_after.saturating_add(usdc_after),
				trx_usdt_before.saturating_add(usdc_before).saturating_add(balance),
				"{account:?} was not credited {balance}",
			);
			trx_usdt_credited = trx_usdt_credited.saturating_add(in_trx_usdt);
			usdc_credited = usdc_credited.saturating_add(in_usdc);
		}
		log::info!(
			"🧹 Credited {} of {} remaining trxUSDT, plus {} USDC, across {} accounts.",
			trx_usdt_credited,
			REMAINING_TRX_USDT,
			usdc_credited,
			accounts,
		);

		// The trxUSDT credited has to be backed by what is actually held.
		frame_support::ensure!(
			trx_usdt_credited <= REMAINING_TRX_USDT,
			"credited more trxUSDT than remains"
		);
		frame_support::ensure!(
			TrxUsdtExploitSnapshot::<Runtime>::get().is_empty(),
			"TrxUsdt snapshot was not cleared"
		);
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use pallet_cf_asset_balances::FreeBalances;
	use sp_runtime::AccountId32;
	use sp_std::collections::btree_map::BTreeMap;

	// Recorded balances far larger than what is left, so the pro-rata split is the interesting
	// case.
	const BALANCE_A: AssetAmount = 30_000_000_000;
	const BALANCE_C: AssetAmount = 70_000_000_000;
	// 30% and 70% of REMAINING_TRX_USDT, each rounded down.
	const TRX_USDT_A: AssetAmount = 3_217_315_325;
	const TRX_USDT_C: AssetAmount = 7_507_069_092;

	#[test]
	fn reads_the_storage_written_by_2_2_13() {
		// The key the snapshot is held under on Berghain. If this changes, the migration reads an
		// empty snapshot and credits nothing.
		assert_eq!(
			TrxUsdtExploitSnapshot::<Runtime>::hashed_key(),
			hex_literal::hex!("11aa255003507417d11e3ec7527befeca66e76628b27a9791c743b40e2e54d33"),
		);
	}

	#[test]
	fn distributes_pro_rata_in_trx_usdt_and_the_rest_in_usdc() {
		sp_io::TestExternalities::default().execute_with(|| {
			// Events are dropped at block 0.
			frame_system::Pallet::<Runtime>::set_block_number(1);
			let a = AccountId32::from([1u8; 32]);
			let b = AccountId32::from([2u8; 32]);
			let c = AccountId32::from([3u8; 32]);

			// `b` has a zero balance, `c` already holds some USDC.
			TrxUsdtExploitSnapshot::<Runtime>::put(BTreeMap::from([
				(a.clone(), BALANCE_A),
				(b.clone(), 0 as AssetAmount),
				(c.clone(), BALANCE_C),
			]));
			FreeBalances::<Runtime>::insert(&c, Asset::Usdc, 7 as AssetAmount);

			distribute_trx_usdt_snapshot();

			// The remaining trxUSDT is split in proportion to each account's recorded balance.
			assert_eq!(FreeBalances::<Runtime>::get(&a, Asset::TrxUsdt), TRX_USDT_A);
			assert_eq!(FreeBalances::<Runtime>::get(&c, Asset::TrxUsdt), TRX_USDT_C);

			// USDC covers the rest, on top of any balance already held.
			assert_eq!(FreeBalances::<Runtime>::get(&a, Asset::Usdc), BALANCE_A - TRX_USDT_A);
			assert_eq!(FreeBalances::<Runtime>::get(&c, Asset::Usdc), BALANCE_C - TRX_USDT_C + 7);

			// Rounding down never credits more trxUSDT than is held, and leaves only dust.
			let credited = TRX_USDT_A + TRX_USDT_C;
			assert!(credited <= REMAINING_TRX_USDT);
			assert_eq!(REMAINING_TRX_USDT - credited, 1);

			// An account with a zero balance is credited nothing, in either asset.
			for asset in [Asset::TrxUsdt, Asset::Usdc] {
				assert_eq!(FreeBalances::<Runtime>::get(&b, asset), 0);
			}

			// Every credit is visible as an event, and the zero amounts are not credited.
			let events = frame_system::Pallet::<Runtime>::events();
			for (account, asset, amount, new_balance) in [
				(&a, Asset::TrxUsdt, TRX_USDT_A, TRX_USDT_A),
				(&a, Asset::Usdc, BALANCE_A - TRX_USDT_A, BALANCE_A - TRX_USDT_A),
				(&c, Asset::TrxUsdt, TRX_USDT_C, TRX_USDT_C),
				(&c, Asset::Usdc, BALANCE_C - TRX_USDT_C, BALANCE_C - TRX_USDT_C + 7),
			] {
				assert!(
					events.iter().any(|record| {
						record.event ==
							crate::RuntimeEvent::AssetBalances(
								pallet_cf_asset_balances::Event::AccountCredited {
									account_id: account.clone(),
									asset,
									amount_credited: amount,
									new_balance,
								},
							)
					}),
					"missing {amount} {asset:?} credit for {account:?}",
				);
			}
			assert_eq!(events.len(), 4);

			// The snapshot is cleared, so a repeat run credits nothing.
			assert!(TrxUsdtExploitSnapshot::<Runtime>::get().is_empty());
			distribute_trx_usdt_snapshot();
			assert_eq!(FreeBalances::<Runtime>::get(&a, Asset::TrxUsdt), TRX_USDT_A);
			assert_eq!(FreeBalances::<Runtime>::get(&a, Asset::Usdc), BALANCE_A - TRX_USDT_A);
			assert_eq!(events.len(), 4);
		});
	}

	#[test]
	fn snapshot_within_the_remaining_trx_usdt_is_credited_in_full_in_trx_usdt() {
		sp_io::TestExternalities::default().execute_with(|| {
			let a = AccountId32::from([1u8; 32]);
			let b = AccountId32::from([2u8; 32]);
			TrxUsdtExploitSnapshot::<Runtime>::put(BTreeMap::from([
				(a.clone(), 100 as AssetAmount),
				(b.clone(), 250 as AssetAmount),
			]));

			distribute_trx_usdt_snapshot();

			// No account is credited more than its recorded balance, and no USDC is needed.
			for (account, balance) in [(&a, 100), (&b, 250)] {
				assert_eq!(FreeBalances::<Runtime>::get(account, Asset::TrxUsdt), balance);
				assert_eq!(FreeBalances::<Runtime>::get(account, Asset::Usdc), 0);
			}
		});
	}
}
