use crate::{chainflip::witnessing::bitcoin_elections, Runtime};
use cf_chains::{btc::BitcoinTrackedData, instances::BitcoinInstance, ChainState};
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
use pallet_cf_elections::ElectionPalletStatus;
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

/// Resets Bitcoin-related pallet state in preparation for a testnet3 → testnet4 migration.
///
/// This migration:
/// - Clears all Bitcoin elections state (the pallet must be re-initialized via governance)
/// - Resets the Bitcoin chain tracking block height to 0
/// - Resets `ProcessedUpTo` in Bitcoin ingress-egress to 0
/// - Resets all Bitcoin broadcast timeouts to empty
///
/// Prerequisites: Bitcoin elections should be paused and ingresses disabled via governance
/// before this migration runs.
pub struct BitcoinTestnet4Migration;

impl OnRuntimeUpgrade for BitcoinTestnet4Migration {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
		use codec::Encode;
		// Verify that bitcoin elections exist (i.e. we have state to clear)
		let status = pallet_cf_elections::Status::<Runtime, BitcoinInstance>::get();
		log::info!("🔧 BTC testnet4 pre_upgrade: elections status = {:?}", status.is_some());
		Ok(().encode())
	}

	fn on_runtime_upgrade() -> Weight {
		use cf_runtime_utilities::genesis_hashes;

		// Only run on testnets (Perseverance and Sisyphos), not on mainnet (Berghain).
		match genesis_hashes::genesis_hash::<Runtime>() {
			genesis_hashes::BERGHAIN => {
				log::info!("🔧 BitcoinTestnet4Migration: Skipping on Berghain (mainnet).");
				return Weight::zero();
			},
			genesis_hashes::PERSEVERANCE | genesis_hashes::SISYPHOS => {
				log::info!(
					"🔧 BitcoinTestnet4Migration: Running on testnet - resetting BTC state."
				);
			},
			_ => {
				log::info!(
					"🔧 BitcoinTestnet4Migration: Running on dev/localnet - resetting BTC state."
				);
			},
		}

		// 0. Clear all Bitcoin elections state
		pallet_cf_elections::Pallet::<Runtime, BitcoinInstance>::clear_all_storage();
		log::info!("🔧 BitcoinTestnet4Migration: Cleared Bitcoin elections state.");

		// 1. Initialize with a clean state and immediately pause
		let _ = pallet_cf_elections::Pallet::<Runtime, BitcoinInstance>::internally_initialize(
			bitcoin_elections::initial_state(),
		);
		pallet_cf_elections::Status::<Runtime, BitcoinInstance>::put(
			ElectionPalletStatus::Paused { detected_corrupt_storage: false },
		);
		log::info!(
			"🔧 BitcoinTestnet4Migration: Bitcoin elections state re-initialized and paused."
		);

		// 2. Reset chain tracking block height to 0
		pallet_cf_chain_tracking::CurrentChainState::<Runtime, BitcoinInstance>::put(ChainState {
			block_height: 0u64,
			tracked_data: BitcoinTrackedData::default(),
		});
		log::info!("🔧 BitcoinTestnet4Migration: Reset Bitcoin chain tracking to height 0.");

		// 3. Reset ProcessedUpTo in Bitcoin ingress-egress to 0
		pallet_cf_ingress_egress::ProcessedUpTo::<Runtime, BitcoinInstance>::set(0u64);
		log::info!("🔧 BitcoinTestnet4Migration: Reset Bitcoin ProcessedUpTo to 0.");

		// 4. Set all Bitcoin broadcast timeouts to 0 so they trigger immediately
		pallet_cf_broadcast::Timeouts::<Runtime, BitcoinInstance>::mutate(|timeouts| {
			for (expiry_block, _, _) in timeouts.iter_mut() {
				*expiry_block = 0;
			}
		});
		log::info!("🔧 BitcoinTestnet4Migration: Reset Bitcoin broadcast timeouts to 0.");

		// 5. Set all Bitcoin deposit channel expiries to 0 so they expire immediately
		let mut count = 0u32;
		pallet_cf_ingress_egress::DepositChannelLookup::<Runtime, BitcoinInstance>::translate_values(
			|mut details: pallet_cf_ingress_egress::DepositChannelDetails<Runtime, BitcoinInstance>| {
				details.expires_at = 0;
				count += 1;
				Some(details)
			},
		);
		log::info!(
			"🔧 BitcoinTestnet4Migration: Set {} Bitcoin deposit channels to expire immediately.",
			count
		);

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		use cf_runtime_utilities::genesis_hashes;

		if genesis_hashes::genesis_hash::<Runtime>() == genesis_hashes::BERGHAIN {
			return Ok(());
		}

		// Verify elections pallet is now paused
		frame_support::ensure!(
			pallet_cf_elections::Status::<Runtime, BitcoinInstance>::get()
				.is_some_and(|status| status ==
					ElectionPalletStatus::Paused { detected_corrupt_storage: false }),
			"Bitcoin elections should be uninitialized after migration"
		);

		// Verify chain tracking is at height 0
		let chain_state =
			pallet_cf_chain_tracking::CurrentChainState::<Runtime, BitcoinInstance>::get();
		frame_support::ensure!(
			chain_state.map(|s| s.block_height) == Some(0),
			"Bitcoin chain tracking should be at height 0"
		);

		// Verify ProcessedUpTo is 0
		let processed_up_to =
			pallet_cf_ingress_egress::ProcessedUpTo::<Runtime, BitcoinInstance>::get();
		frame_support::ensure!(processed_up_to == 0, "Bitcoin ProcessedUpTo should be 0");

		// Verify broadcast timeouts all have expiry_block == 0
		let timeouts = pallet_cf_broadcast::Timeouts::<Runtime, BitcoinInstance>::get();
		frame_support::ensure!(
			timeouts.iter().all(|(expiry_block, _, _)| *expiry_block == 0),
			"Bitcoin broadcast timeouts should all have expiry_block 0"
		);

		// Verify all deposit channels have expires_at == 0
		frame_support::ensure!(
			pallet_cf_ingress_egress::DepositChannelLookup::<Runtime, BitcoinInstance>::iter_values()
				.all(|details| details.expires_at == 0),
			"All Bitcoin deposit channels should have expires_at 0"
		);

		log::info!("🔧 BitcoinTestnet4Migration: post_upgrade checks passed.");
		Ok(())
	}
}
