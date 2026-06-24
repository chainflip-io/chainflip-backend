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

//! A multi-producer, single-consumer channel that bounds the number of
//! in-flight messages *per sender identity* rather than globally.
//!
//! Each message is attributed to the identity (`K`) of the peer it came from.
//! A peer may have at most `per_peer_capacity` messages in flight (sent but
//! not yet received) at once; once it reaches that limit, further messages
//! from *that peer* are dropped while other peers are unaffected. This stops a
//! single flooding peer from crowding honest peers out of a shared queue.

use std::{
	collections::HashMap,
	hash::Hash,
	sync::{Arc, Mutex},
};

use tokio::sync::mpsc;

/// Per-peer count of messages currently in flight (sent but not yet received),
/// shared between the sender and the receiver.
type InFlightCounts<K> = Arc<Mutex<HashMap<K, usize>>>;

/// Reason a message was not accepted by [`FairSender::try_send`].
#[derive(Debug, PartialEq, Eq)]
pub enum FairSendError {
	/// The sending peer already has `per_peer_capacity` messages in flight.
	PeerQuotaExceeded,
	/// The receiving end has been dropped.
	Closed,
}

/// Create a fair channel in which each peer may have at most
/// `per_peer_capacity` messages in flight at any one time.
pub fn fair_channel<K, T>(per_peer_capacity: usize) -> (FairSender<K, T>, FairReceiver<K, T>) {
	assert!(per_peer_capacity > 0, "per_peer_capacity must be non-zero");
	// The underlying channel is unbounded: the total number of in-flight
	// messages is already bounded by `per_peer_capacity` times the number of
	// distinct senders, and it is the per-peer accounting that we rely on.
	let (sender, receiver) = mpsc::unbounded_channel();
	let counts: InFlightCounts<K> = Arc::new(Mutex::new(HashMap::new()));
	(
		FairSender { sender, counts: counts.clone(), per_peer_capacity },
		FairReceiver { receiver, counts },
	)
}

/// Sending half of a [`fair_channel`].
pub struct FairSender<K, T> {
	sender: mpsc::UnboundedSender<(K, T)>,
	counts: InFlightCounts<K>,
	per_peer_capacity: usize,
}

impl<K: Eq + Hash + Clone, T> FairSender<K, T> {
	/// Send a message attributed to peer `key`, or drop it (returning an error)
	/// if that peer already has `per_peer_capacity` messages in flight. Never
	/// blocks.
	pub fn try_send(&self, key: K, value: T) -> Result<(), FairSendError> {
		{
			let mut counts = self.counts.lock().unwrap();
			// Check the quota without cloning the key, so the over-limit drop
			// path (the adversarial flood case) does no work beyond a lookup.
			// Only a peer's *first* in-flight message has to insert (and so
			// clone) a key.
			match counts.get_mut(&key) {
				Some(in_flight) => {
					if *in_flight >= self.per_peer_capacity {
						return Err(FairSendError::PeerQuotaExceeded);
					}
					*in_flight += 1;
				},
				// `per_peer_capacity` is always >= 1, so a peer's first
				// in-flight message is always within quota.
				None => {
					counts.insert(key.clone(), 1);
				},
			}
		}
		// A leaked count here is harmless: `Closed` only happens once the
		// receiver is gone, i.e. the whole channel is being torn down.
		self.sender.send((key, value)).map_err(|_| FairSendError::Closed)
	}

	/// Send a message attributed to peer `key`, running `on_dropped` if the
	/// peer is over its in-flight limit.
	pub fn try_send_or_drop(&self, key: K, value: T, on_dropped: impl FnOnce()) {
		match self.try_send(key, value) {
			Ok(()) | Err(FairSendError::Closed) => {},
			Err(FairSendError::PeerQuotaExceeded) => on_dropped(),
		}
	}
}

/// Receiving half of a [`fair_channel`].
pub struct FairReceiver<K, T> {
	receiver: mpsc::UnboundedReceiver<(K, T)>,
	counts: InFlightCounts<K>,
}

impl<K: Eq + Hash, T> FairReceiver<K, T> {
	/// Receive the next message, freeing up one in-flight slot for its sender.
	///
	/// Cancel-safe: if the returned future is dropped before it completes, no
	/// message is consumed and no count is changed.
	pub async fn recv(&mut self) -> Option<(K, T)> {
		let (key, value) = self.receiver.recv().await?;
		{
			let mut counts = self.counts.lock().unwrap();
			if let Some(in_flight) = counts.get_mut(&key) {
				*in_flight = in_flight.saturating_sub(1);
				if *in_flight == 0 {
					counts.remove(&key);
				}
			}
		}
		Some((key, value))
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[tokio::test]
	async fn drops_messages_once_peer_is_over_quota() {
		let (sender, mut receiver) = fair_channel::<&str, u32>(2);

		// A peer can fill its own quota...
		assert_eq!(sender.try_send("alice", 1), Ok(()));
		assert_eq!(sender.try_send("alice", 2), Ok(()));
		// ...but not exceed it.
		assert_eq!(sender.try_send("alice", 3), Err(FairSendError::PeerQuotaExceeded));

		// A different peer is unaffected by the first peer's flood.
		assert_eq!(sender.try_send("bob", 10), Ok(()));

		// Receiving frees up a slot, so the peer can send again.
		assert_eq!(receiver.recv().await, Some(("alice", 1)));
		assert_eq!(sender.try_send("alice", 4), Ok(()));
	}

	#[tokio::test]
	async fn in_flight_count_is_released_after_receive() {
		let (sender, mut receiver) = fair_channel::<&str, u32>(1);

		for i in 0..100 {
			assert_eq!(sender.try_send("alice", i), Ok(()));
			assert_eq!(receiver.recv().await, Some(("alice", i)));
		}
	}
}
