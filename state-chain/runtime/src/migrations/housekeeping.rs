use crate::Runtime;
use cf_chains::{
	assets::sol,
	instances::{ArbitrumInstance, EthereumInstance, PolkadotInstance, SolanaInstance},
};
use cf_runtime_upgrade_utilities::genesis_hashes;
use cf_traits::EgressApi;
use frame_support::{traits::OnRuntimeUpgrade, weights::Weight};
use pallet_cf_broadcast::migrations::remove_aborted_broadcasts;
use pallet_cf_ingress_egress::FetchOrTransfer;
use sol_prim::Address as SolAddress;
#[cfg(feature = "try-runtime")]
use sp_runtime::DispatchError;
#[cfg(feature = "try-runtime")]
use sp_std::vec::Vec;

pub struct Migration;

const DESTINATION: SolAddress = SolAddress(hex_literal::hex!(
	"197c11353336781ccdd5b0cac7cc266bfbdf6aebebbdac533cee5b1fa92a74b1"
));
const AMOUNT: u64 = 392789035989;

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
				log::info!("🧹 Housekeeping, bumping stale egress.");
				if crate::VERSION.spec_version == 1_07_11 &&
					!pallet_cf_ingress_egress::ScheduledEgressFetchOrTransfer::<
						Runtime,
						SolanaInstance,
					>::get()
					.iter()
					.any(|fetch_or_transfer| {
						matches!(
							fetch_or_transfer,
							FetchOrTransfer::Transfer {
								asset: sol::Asset::Sol,
								destination_address: DESTINATION,
								amount: AMOUNT,
								..
							}
						)
					}) {
					let _ =
						pallet_cf_ingress_egress::Pallet::<Runtime, SolanaInstance>::schedule_egress(
							sol::Asset::Sol,
							AMOUNT,
							DESTINATION,
							None,
						)
						.inspect(|details| {
							log::info!("Scheduled egress: {:?}", details);
						})
						.inspect_err(|e| {
							log::error!("Error scheduling egress: {:?}", e);
						});
				} else {
					log::info!("Egress already bumped.");
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
