use core::str::FromStr as _;

use crate::{Runtime, SolEnvironment};
use cf_chains::{
	instances::{ArbitrumInstance, EthereumInstance, PolkadotInstance, SolanaInstance},
	sol::{api::SolanaApi, SolAddress, SolAsset},
	ForeignChain, Solana, TransferAssetParams,
};
use cf_runtime_upgrade_utilities::genesis_hashes;
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
use pallet_cf_broadcast::migrations::remove_aborted_broadcasts;
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

pub struct Migration;

impl OnRuntimeUpgrade for Migration {
	fn on_runtime_upgrade() -> Weight {
		match genesis_hashes::genesis_hash::<Runtime>() {
			genesis_hashes::BERGHAIN => {
				log::info!("🧹 Housekeeping, removing stale aborted broadcasts");
				remove_aborted_broadcasts::remove_stale_and_all_older::<Runtime, EthereumInstance>(
					remove_aborted_broadcasts::ETHEREUM_MAX_ABORTED_BROADCAST_BERGHAIN,
				);
				remove_aborted_broadcasts::remove_stale_and_all_older::<Runtime, ArbitrumInstance>(
					remove_aborted_broadcasts::ARBITRUM_MAX_ABORTED_BROADCAST_BERGHAIN,
				);
				if crate::VERSION.spec_version == 175 {
					log::info!("🧹 Housekeeping, bumping Solana refund");
					if let Ok(to) =
						SolAddress::from_str("9RbaLSDtScGDur9UGAiBQFYPiKK9xDZLuD39xkEqa5Zw")
					{
						if let Ok(mut calls) =
							SolanaApi::<SolEnvironment>::transfer(sp_std::vec![(
								TransferAssetParams::<Solana> {
									asset: SolAsset::SolUsdc,
									amount: 162740593954,
									to,
								},
								(ForeignChain::Solana, 0)
							)]) {
							if calls.len() == 1 {
								let _ = <pallet_cf_broadcast::Pallet<Runtime, SolanaInstance> as cf_traits::Broadcaster<
											Solana,
										>>::threshold_sign_and_broadcast(
											calls.pop().expect("Checked for 1 call.").0,
										);
							} else {
								log::error!("Expected 1 call, got {}", calls.len());
							}
						} else {
							log::error!("Failed to build Solana transaction");
						}
					} else {
						log::error!("Invalid Solana address");
					}
				}
			},
			genesis_hashes::PERSEVERANCE => {
				log::info!("🧹 Housekeeping, removing stale aborted broadcasts");
				remove_aborted_broadcasts::remove_stale_and_all_older::<Runtime, EthereumInstance>(
					remove_aborted_broadcasts::ETHEREUM_MAX_ABORTED_BROADCAST_PERSEVERANCE,
				);
				remove_aborted_broadcasts::remove_stale_and_all_older::<Runtime, ArbitrumInstance>(
					remove_aborted_broadcasts::ARBITRUM_MAX_ABORTED_BROADCAST_PERSEVERANCE,
				);
				remove_aborted_broadcasts::remove_stale_and_all_older::<Runtime, PolkadotInstance>(
					remove_aborted_broadcasts::POLKADOT_MAX_ABORTED_BROADCAST_PERSEVERANCE,
				);
			},
			genesis_hashes::SISYPHOS => {
				log::info!("🧹 No housekeeping required for Sisyphos.");
			},
			_ => {},
		}

		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: Vec<u8>) -> Result<(), DispatchError> {
		match genesis_hashes::genesis_hash::<Runtime>() {
			genesis_hashes::BERGHAIN => {
				log::info!(
					"Housekeeping post_upgrade, checking stale aborted broadcasts are removed."
				);
				remove_aborted_broadcasts::assert_removed::<Runtime, EthereumInstance>(
					remove_aborted_broadcasts::ETHEREUM_MAX_ABORTED_BROADCAST_BERGHAIN,
				);
				remove_aborted_broadcasts::assert_removed::<Runtime, ArbitrumInstance>(
					remove_aborted_broadcasts::ARBITRUM_MAX_ABORTED_BROADCAST_BERGHAIN,
				);
			},
			genesis_hashes::PERSEVERANCE => {
				log::info!(
					"Housekeeping post_upgrade, checking stale aborted broadcasts are removed."
				);
				remove_aborted_broadcasts::assert_removed::<Runtime, EthereumInstance>(
					remove_aborted_broadcasts::ETHEREUM_MAX_ABORTED_BROADCAST_PERSEVERANCE,
				);
				remove_aborted_broadcasts::assert_removed::<Runtime, ArbitrumInstance>(
					remove_aborted_broadcasts::ARBITRUM_MAX_ABORTED_BROADCAST_PERSEVERANCE,
				);
				remove_aborted_broadcasts::assert_removed::<Runtime, PolkadotInstance>(
					remove_aborted_broadcasts::POLKADOT_MAX_ABORTED_BROADCAST_PERSEVERANCE,
				);
			},
			genesis_hashes::SISYPHOS => {
				log::info!("Skipping housekeeping post_upgrade for Sisyphos.");
			},
			_ => {},
		}
		Ok(())
	}
}
