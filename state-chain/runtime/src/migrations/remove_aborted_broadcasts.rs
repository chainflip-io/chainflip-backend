use cf_chains::instances::{ArbitrumInstance, EthereumInstance};
use frame_support::weights::Weight;
use pallet_cf_broadcast::AbortedBroadcasts;

use crate::*;

// Highest stale aborted broadcasts on mainnet as of 23/09/2024
const ETHEREUM_MAX_ABORTED_BROADCAST: BroadcastId = 11592;
const ARBITRUM_MAX_ABORTED_BROADCAST: BroadcastId = 426;

pub struct EthereumMigration;
pub struct ArbitrumMigration;

impl EthereumMigration {
	pub(super) fn on_runtime_upgrade() -> Weight {
		AbortedBroadcasts::<Runtime, EthereumInstance>::mutate(|aborted| {
			aborted.retain(|id| id > &ETHEREUM_MAX_ABORTED_BROADCAST);
		});
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	pub(super) fn post_upgrade() {
		let aborted_broadcasts = AbortedBroadcasts::<Runtime, EthereumInstance>::get();
		if let Some(first) = aborted_broadcasts.first() {
			assert!(
				*first > ETHEREUM_MAX_ABORTED_BROADCAST,
				"Aborted broadcast {first} was not removed"
			);
		}
	}
}

impl ArbitrumMigration {
	pub(super) fn on_runtime_upgrade() -> Weight {
		AbortedBroadcasts::<Runtime, ArbitrumInstance>::mutate(|aborted| {
			aborted.retain(|id| id > &ARBITRUM_MAX_ABORTED_BROADCAST);
		});
		Weight::zero()
	}

	#[cfg(feature = "try-runtime")]
	pub(super) fn post_upgrade() {
		let aborted_broadcasts = AbortedBroadcasts::<Runtime, ArbitrumInstance>::get();
		if let Some(first) = aborted_broadcasts.first() {
			assert!(
				*first > ARBITRUM_MAX_ABORTED_BROADCAST,
				"Aborted broadcast {first} was not removed"
			);
		}
	}
}
