use crate::{Runtime, Weight};
use cf_primitives::{AccountId, Asset, AssetAmount};
use cf_traits::PoolOrdersManager;
use codec::{Decode, Encode};
use frame_support::{ensure, traits::OnRuntimeUpgrade};
use pallet_cf_asset_balances::FreeBalances;
use pallet_cf_pools::AssetPair;
use sp_core::crypto::AccountId32;
use sp_runtime::traits::Zero;
#[cfg(feature = "try-runtime")]
use sp_runtime::TryRuntimeError;
use sp_std::{collections::btree_map::BTreeMap, vec::Vec};

pub struct PolkadotDeprecationMigration;

/// Allows deprecating Polkadot. This will trigger:
///    1. Canceling all Polkadot orders in the Dot/USDC pool
///    2. Converting all Polkadot balances to Assethub balances
///
/// Note that this call should be run after disabling Dot ingress using safemode and waiting
/// for all Dot deposit channels to expire
impl OnRuntimeUpgrade for PolkadotDeprecationMigration {
	fn on_runtime_upgrade() -> Weight {
		let mut weight: Weight = Weight::zero();

		let dot_usdc_pair = AssetPair::try_new::<Runtime>(Asset::Dot, Asset::Usdc)
			.expect("Failed to create Dot/USDC AssetPair");

		if pallet_cf_pools::pallet::Pools::<Runtime>::get(dot_usdc_pair).is_some() {
			log::info!("🍩 Cancelling all Polkadot orders");

			weight += <Runtime as frame_system::Config>::DbWeight::get().reads(1);
			match pallet_cf_pools::Pallet::<Runtime>::pool_orders(
				Asset::Dot,
				Asset::Usdc,
				None,
				false,
			) {
				Err(e) => log::error!("❗️ Failed to get DOT/USDC pool orders: {:?}", e),
				Ok(dot_orders) => {
					let dot_orders_count = dot_orders.limit_orders.asks.len() +
						dot_orders.limit_orders.bids.len() +
						dot_orders.range_orders.len();

					weight += <() as pallet_cf_pools::WeightInfo>::cancel_orders_batch(
						dot_orders_count as u32,
					);

					if let Err(e) = pallet_cf_pools::Pallet::<Runtime>::cancel_all_pool_orders(
						Asset::Dot,
						Asset::Usdc,
					) {
						log::error!("❗️ Failed to cancel Polkadot orders {:?}", e);
						return weight;
					};

					log::info!("🍩 Successfully canceled {} Polkadot orders", dot_orders_count);
				},
			};

			log::info!("🍩 Deleting the DOT/USDC pool");
			pallet_cf_pools::pallet::Pools::<Runtime>::remove(dot_usdc_pair)
		}

		log::info!("🍩 Transferring all Dot balances to HubDot balances");
		for (account_id, asset, amount) in FreeBalances::<Runtime>::iter() {
			if asset == Asset::Dot {
				if amount > AssetAmount::zero() {
					FreeBalances::<Runtime>::mutate(account_id.clone(), Asset::HubDot, |balance| {
						*balance = balance.saturating_add(amount);
					});
					weight += <Runtime as frame_system::Config>::DbWeight::get().reads_writes(1, 1);
				}

				FreeBalances::<Runtime>::mutate_exists(account_id, Asset::Dot, |balance| {
					*balance = None;
				});
				weight += <Runtime as frame_system::Config>::DbWeight::get().reads_writes(1, 1);
			}
		}
		log::info!("🍩 All Dot balances to transferred to HubDot balances");

		weight
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
		let mut balances: BTreeMap<AccountId32, (AssetAmount, AssetAmount)> = BTreeMap::new();

		for (account_id, asset, amount) in FreeBalances::<Runtime>::iter() {
			if asset == Asset::Dot && amount > AssetAmount::zero() {
				match balances.get(&account_id) {
					Some((_, hubdot_balance)) => {
						balances.insert(account_id.clone(), (amount, *hubdot_balance));
					},
					None => {
						balances.insert(account_id.clone(), (amount, AssetAmount::zero()));
					},
				}
			}

			if asset == Asset::HubDot && amount > AssetAmount::zero() {
				match balances.get(&account_id) {
					Some((dot_balance, _)) => {
						balances.insert(account_id.clone(), (*dot_balance, amount));
					},
					None => {
						balances.insert(account_id.clone(), (AssetAmount::zero(), amount));
					},
				}
			}
		}

		Ok(balances.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), TryRuntimeError> {
		use frame_support::ensure;

		let old_balances =
			BTreeMap::<AccountId, (AssetAmount, AssetAmount)>::decode(&mut state.as_slice())
				.map_err(|_| TryRuntimeError::from("Failed to decode old balances state"))?;

		for (account_id, (old_dot_balance, old_hubdot_balance)) in old_balances.iter() {
			ensure!(
				FreeBalances::<Runtime>::get(account_id, Asset::Dot) == 0,
				"Expected all Dot balances to be zero after migration"
			);

			ensure!(
				FreeBalances::<Runtime>::get(account_id, Asset::HubDot) == old_hubdot_balance.saturating_add(*old_dot_balance),
				"Expected all HubDot balances to be increased by the old Dot balance after migration"
			);

			ensure!(
				pallet_cf_pools::pallet::Pools::<Runtime>::get(AssetPair::try_new::<Runtime>(
					Asset::Dot,
					Asset::Usdc
				)?)
				.is_none(),
				"Expected the Dot/USDC pool to be deleted after migration"
			);
		}

		Ok(())
	}
}
