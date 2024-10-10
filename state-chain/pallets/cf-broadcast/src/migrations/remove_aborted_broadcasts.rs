use crate::*;

// Highest stale aborted broadcasts as of 3/10/2024:
// Mainnet
pub const ETHEREUM_MAX_ABORTED_BROADCAST_BERGHAIN: BroadcastId = 11592;
pub const ARBITRUM_MAX_ABORTED_BROADCAST_BERGHAIN: BroadcastId = 426;
// Perseverance testnet
pub const ETHEREUM_MAX_ABORTED_BROADCAST_PERSEVERANCE: BroadcastId = 1609;
pub const ARBITRUM_MAX_ABORTED_BROADCAST_PERSEVERANCE: BroadcastId = 665;
pub const POLKADOT_MAX_ABORTED_BROADCAST_PERSEVERANCE: BroadcastId = 634;

pub fn remove_stale_and_all_older<T: Config<I>, I: 'static>(latest_stale_broadcast: BroadcastId) {
	AbortedBroadcasts::<T, I>::mutate(|aborted| {
		aborted.retain(|id| id > &latest_stale_broadcast);
	});
}

#[cfg(feature = "try-runtime")]
pub fn assert_removed<T: Config<I>, I: 'static>(latest_stale_broadcast: BroadcastId) {
	let aborted_broadcasts = AbortedBroadcasts::<T, I>::get();
	if let Some(first) = aborted_broadcasts.first() {
		assert!(*first > latest_stale_broadcast, "Aborted broadcast {first} was not removed");
	}
}
