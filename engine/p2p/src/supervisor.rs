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

//! Transport supervisor.
//!
//! The supervisor owns the stable channels that connect the [`crate::muxer::TopicMuxer`]
//! (and therefore the multisig clients above it) to whichever transport is currently
//! running. The transport itself (ZMQ or QUIC) is restartable: when the network-wide
//! transport selection changes, the supervisor gracefully shuts the current transport
//! down — releasing its port — and starts the newly-selected one, without disturbing the
//! muxer or the multisig ceremonies layered on top.
//!
//! To make a new transport pick up where the old one left off, the supervisor maintains a
//! registry of the current peer set (seeded from the initial peers and kept up to date
//! from the peer-update stream) and hands each transport incarnation a fresh snapshot.

use std::{collections::BTreeMap, future::Future};

use cf_utilities::{
	metrics::P2P_BAD_MSG,
	Port,
};
use tokio::sync::{
	mpsc::{UnboundedReceiver, UnboundedSender},
	oneshot,
};

use crate::{
	fair_channel::FairSender,
	message::{AccountId, OutgoingMessage},
	peer::{PeerInfo, PeerUpdate},
	P2PKey, Transport,
};

/// The per-incarnation channel ends handed to a single transport run. A fresh set is
/// created for every (re)start so the previous transport's channels can be torn down
/// cleanly.
pub struct TransportChannels {
	/// The transport sends messages it receives from peers here.
	pub incoming_message_sender: UnboundedSender<(AccountId, Vec<u8>)>,
	/// The transport reads messages destined for peers from here.
	pub outgoing_message_receiver: UnboundedReceiver<OutgoingMessage>,
	/// The transport reads peer registration/deregistration updates from here.
	pub peer_update_receiver: UnboundedReceiver<PeerUpdate>,
	/// Fires when the supervisor wants the transport to shut down (and release its port).
	pub shutdown: oneshot::Receiver<()>,
}

/// Apply a peer update to the registry that tracks the current peer set.
fn apply_peer_update(registry: &mut BTreeMap<AccountId, PeerInfo>, update: &PeerUpdate) {
	match update {
		PeerUpdate::Registered(info) => {
			registry.insert(info.account_id.clone(), info.clone());
		},
		PeerUpdate::Deregistered(account_id, _) => {
			registry.remove(account_id);
		},
	}
}

/// Supervise a restartable transport, parameterised over how a transport is started so the
/// restart logic can be tested without real sockets.
pub(crate) async fn run_transport_supervisor_with<R, Fut>(
	initial_transport: Transport,
	initial_peers: Vec<PeerInfo>,
	incoming_message_sender: FairSender<AccountId, Vec<u8>>,
	outgoing_message_receiver: UnboundedReceiver<OutgoingMessage>,
	peer_update_receiver: UnboundedReceiver<PeerUpdate>,
	restart_receiver: UnboundedReceiver<Transport>,
	run_transport: R,
) -> anyhow::Result<()>
where
	R: Fn(Transport, Vec<PeerInfo>, TransportChannels) -> Fut,
	Fut: Future<Output = anyhow::Result<()>>,
{
	let mut registry: BTreeMap<AccountId, PeerInfo> =
		initial_peers.into_iter().map(|p| (p.account_id.clone(), p)).collect();
	let mut current_transport = initial_transport;

	let mut outgoing_message_receiver = outgoing_message_receiver;
	let mut peer_update_receiver = peer_update_receiver;
	let mut restart_receiver = restart_receiver;

	loop {
		// Fresh channels for this transport incarnation; the previous incarnation's channels
		// are dropped at the end of the loop body.
		let (transport_incoming_sender, mut transport_incoming_receiver) =
			tokio::sync::mpsc::unbounded_channel();
		let (transport_outgoing_sender, transport_outgoing_receiver) =
			tokio::sync::mpsc::unbounded_channel();
		let (transport_peer_update_sender, transport_peer_update_receiver) =
			tokio::sync::mpsc::unbounded_channel();
		let (shutdown_sender, shutdown_receiver) = oneshot::channel();

		let snapshot: Vec<PeerInfo> = registry.values().cloned().collect();

		let transport_fut = run_transport(
			current_transport,
			snapshot,
			TransportChannels {
				incoming_message_sender: transport_incoming_sender,
				outgoing_message_receiver: transport_outgoing_receiver,
				peer_update_receiver: transport_peer_update_receiver,
				shutdown: shutdown_receiver,
			},
		);
		tokio::pin!(transport_fut);

		enum Outcome {
			/// Governance changed the transport; restart with this one.
			Restart(Transport),
			/// The transport returned on its own (an error, or unexpected completion).
			Exited(anyhow::Result<()>),
		}

		let outcome = loop {
			tokio::select! {
				biased;
				// The transport stopped on its own (it normally runs forever, so this means
				// it failed). Propagate the result.
				result = &mut transport_fut => break Outcome::Exited(result),
				// Governance changed the network-wide transport. A closed channel disables
				// this branch, leaving the current transport running.
				Some(new_transport) = restart_receiver.recv() => break Outcome::Restart(new_transport),
				// Track and forward peer updates.
				Some(update) = peer_update_receiver.recv() => {
					apply_peer_update(&mut registry, &update);
					let _ = transport_peer_update_sender.send(update);
				},
				// Forward outgoing messages from the muxer to the transport.
				Some(message) = outgoing_message_receiver.recv() => {
					let _ = transport_outgoing_sender.send(message);
				},
				// Forward incoming messages from the transport to the muxer, applying
				// per-peer fair queueing so a flooding peer cannot exhaust engine memory.
				Some((account_id, payload)) = transport_incoming_receiver.recv() => {
					incoming_message_sender.try_send_or_drop(account_id, payload, || {
						P2P_BAD_MSG.inc(&["incoming_per_peer_limit"]);
					});
				},
			}
		};

		match outcome {
			Outcome::Exited(result) => return result,
			Outcome::Restart(new_transport) => {
				// Drain any already-queued peer updates so the next transport starts from the
				// freshest peer set, regardless of the order the select arms fired in.
				while let Ok(update) = peer_update_receiver.try_recv() {
					apply_peer_update(&mut registry, &update);
				}
				// Ask the transport to shut down and wait for it to finish, so its port is
				// released before the next transport tries to bind it.
				let _ = shutdown_sender.send(());
				let _ = transport_fut.await;
				current_transport = new_transport;
			},
		}
	}
}

/// Run the transport supervisor, (re)starting the real transports as the network-wide
/// selection changes. The supervisor owns the muxer-facing channels for the whole process;
/// each transport incarnation is handed a clone of the node identity and the current peer
/// snapshot.
#[allow(clippy::too_many_arguments)]
pub async fn run_transport_supervisor(
	initial_transport: Transport,
	p2p_key: P2PKey,
	port: Port,
	our_account_id: AccountId,
	initial_peers: Vec<PeerInfo>,
	incoming_message_sender: FairSender<AccountId, Vec<u8>>,
	outgoing_message_receiver: UnboundedReceiver<OutgoingMessage>,
	peer_update_receiver: UnboundedReceiver<PeerUpdate>,
	restart_receiver: UnboundedReceiver<Transport>,
) -> anyhow::Result<()> {
	run_transport_supervisor_with(
		initial_transport,
		initial_peers,
		incoming_message_sender,
		outgoing_message_receiver,
		peer_update_receiver,
		restart_receiver,
		move |transport, peers, channels: TransportChannels| {
			let p2p_key = p2p_key.clone();
			let our_account_id = our_account_id.clone();
			async move {
				crate::start_transport(
					transport,
					p2p_key,
					port,
					peers,
					our_account_id,
					channels.incoming_message_sender,
					channels.outgoing_message_receiver,
					channels.peer_update_receiver,
					channels.shutdown,
				)
				.await
			}
		},
	)
	.await
}

#[cfg(test)]
mod tests {
	use std::net::Ipv6Addr;

	use tokio::sync::mpsc;

	use super::*;
	use crate::{fair_channel::fair_channel, INCOMING_MESSAGE_PER_PEER_LIMIT};

	fn test_peer(seed: u8) -> PeerInfo {
		PeerInfo {
			account_id: AccountId::new([seed; 32]),
			ed_pubkey: [seed; 32],
			ip: Ipv6Addr::LOCALHOST,
			port: 1000 + seed as u16,
		}
	}

	#[tokio::test]
	async fn restart_starts_new_transport_with_current_peer_snapshot() {
		let (incoming_sender, _incoming_receiver) = fair_channel(INCOMING_MESSAGE_PER_PEER_LIMIT);
		let (_outgoing_sender, outgoing_receiver) = mpsc::unbounded_channel();
		let (peer_update_sender, peer_update_receiver) = mpsc::unbounded_channel();
		let (restart_sender, restart_receiver) = mpsc::unbounded_channel();

		// The fake transport reports the transport type and peer set it was started with,
		// then runs until it is asked to shut down.
		let (started_sender, mut started_receiver) = mpsc::unbounded_channel();
		let run_transport = move |transport, peers: Vec<PeerInfo>, channels: TransportChannels| {
			let started_sender = started_sender.clone();
			async move {
				let account_ids: Vec<AccountId> =
					peers.iter().map(|p| p.account_id.clone()).collect();
				started_sender.send((transport, account_ids)).unwrap();
				// Keep the channels alive and run until told to shut down.
				let _ = channels.shutdown.await;
				Ok(())
			}
		};

		let peer_a = test_peer(1);
		let peer_b = test_peer(2);

		let supervisor = tokio::spawn(run_transport_supervisor_with(
			Transport::Zmq,
			vec![peer_a.clone()],
			incoming_sender,
			outgoing_receiver,
			peer_update_receiver,
			restart_receiver,
			run_transport,
		));

		// First incarnation: ZMQ, with the initial peer set.
		let (transport0, peers0) = started_receiver.recv().await.unwrap();
		assert_eq!(transport0, Transport::Zmq);
		assert_eq!(peers0, vec![peer_a.account_id.clone()]);

		// A new peer registers, then governance switches the transport to QUIC.
		peer_update_sender.send(PeerUpdate::Registered(peer_b.clone())).unwrap();
		restart_sender.send(Transport::Quic).unwrap();

		// Second incarnation: QUIC, with the updated peer set.
		let (transport1, mut peers1) = started_receiver.recv().await.unwrap();
		peers1.sort();
		let mut expected = vec![peer_a.account_id.clone(), peer_b.account_id.clone()];
		expected.sort();
		assert_eq!(transport1, Transport::Quic);
		assert_eq!(peers1, expected);

		supervisor.abort();
	}

	#[tokio::test]
	async fn deregistered_peer_is_dropped_from_the_snapshot() {
		let (incoming_sender, _incoming_receiver) = fair_channel(INCOMING_MESSAGE_PER_PEER_LIMIT);
		let (_outgoing_sender, outgoing_receiver) = mpsc::unbounded_channel();
		let (peer_update_sender, peer_update_receiver) = mpsc::unbounded_channel();
		let (restart_sender, restart_receiver) = mpsc::unbounded_channel();

		let (started_sender, mut started_receiver) = mpsc::unbounded_channel();
		let run_transport = move |_transport, peers: Vec<PeerInfo>, channels: TransportChannels| {
			let started_sender = started_sender.clone();
			async move {
				let account_ids: Vec<AccountId> =
					peers.iter().map(|p| p.account_id.clone()).collect();
				started_sender.send(account_ids).unwrap();
				let _ = channels.shutdown.await;
				Ok(())
			}
		};

		let peer_a = test_peer(1);
		let peer_b = test_peer(2);

		let supervisor = tokio::spawn(run_transport_supervisor_with(
			Transport::Zmq,
			vec![peer_a.clone(), peer_b.clone()],
			incoming_sender,
			outgoing_receiver,
			peer_update_receiver,
			restart_receiver,
			run_transport,
		));

		// First incarnation sees both peers.
		let mut peers0 = started_receiver.recv().await.unwrap();
		peers0.sort();
		let mut expected = vec![peer_a.account_id.clone(), peer_b.account_id.clone()];
		expected.sort();
		assert_eq!(peers0, expected);

		// A deregisters, then restart.
		peer_update_sender
			.send(PeerUpdate::Deregistered(
				peer_a.account_id.clone(),
				crate::EdPublicKey::from_raw(peer_a.ed_pubkey),
			))
			.unwrap();
		restart_sender.send(Transport::Zmq).unwrap();

		// Second incarnation sees only B.
		let peers1 = started_receiver.recv().await.unwrap();
		assert_eq!(peers1, vec![peer_b.account_id.clone()]);

		supervisor.abort();
	}

	#[tokio::test]
	async fn a_transport_error_shuts_the_supervisor_down() {
		let (incoming_sender, _incoming_receiver) = fair_channel(INCOMING_MESSAGE_PER_PEER_LIMIT);
		let (_outgoing_sender, outgoing_receiver) = mpsc::unbounded_channel();
		let (_peer_update_sender, peer_update_receiver) = mpsc::unbounded_channel();
		let (_restart_sender, restart_receiver) = mpsc::unbounded_channel();

		let run_transport = move |_transport, _peers: Vec<PeerInfo>, _channels: TransportChannels| async move {
			anyhow::bail!("transport blew up")
		};

		let result = run_transport_supervisor_with(
			Transport::Zmq,
			vec![],
			incoming_sender,
			outgoing_receiver,
			peer_update_receiver,
			restart_receiver,
			run_transport,
		)
		.await;

		assert!(result.unwrap_err().to_string().contains("transport blew up"));
	}

	#[tokio::test]
	async fn messages_are_forwarded_in_both_directions() {
		let (incoming_sender, mut muxer_incoming_receiver) =
			fair_channel::<AccountId, Vec<u8>>(INCOMING_MESSAGE_PER_PEER_LIMIT);
		let (muxer_outgoing_sender, outgoing_receiver) = mpsc::unbounded_channel();
		let (_peer_update_sender, peer_update_receiver) = mpsc::unbounded_channel();
		let (_restart_sender, restart_receiver) = mpsc::unbounded_channel();

		// Hand the transport-side incoming sender back to the test, and surface every
		// outgoing message the transport receives.
		let (incoming_handle_sender, mut incoming_handle_receiver) = mpsc::unbounded_channel();
		let (observed_outgoing_sender, mut observed_outgoing_receiver) = mpsc::unbounded_channel();

		let run_transport = move |_transport, _peers: Vec<PeerInfo>, channels: TransportChannels| {
			let incoming_handle_sender = incoming_handle_sender.clone();
			let observed_outgoing_sender = observed_outgoing_sender.clone();
			async move {
				incoming_handle_sender.send(channels.incoming_message_sender.clone()).unwrap();
				let mut outgoing_message_receiver = channels.outgoing_message_receiver;
				let mut shutdown = channels.shutdown;
				loop {
					tokio::select! {
						Some(message) = outgoing_message_receiver.recv() => {
							observed_outgoing_sender.send(message).unwrap();
						},
						_ = &mut shutdown => break,
					}
				}
				Ok(())
			}
		};

		let supervisor = tokio::spawn(run_transport_supervisor_with(
			Transport::Zmq,
			vec![],
			incoming_sender,
			outgoing_receiver,
			peer_update_receiver,
			restart_receiver,
			run_transport,
		));

		let transport_incoming_sender = incoming_handle_receiver.recv().await.unwrap();

		// Outgoing: muxer -> supervisor -> transport.
		let message = OutgoingMessage::Broadcast {
			recipients: vec![AccountId::new([9; 32])],
			payload: vec![1, 2, 3],
		};
		muxer_outgoing_sender.send(message.clone()).unwrap();
		assert_eq!(observed_outgoing_receiver.recv().await.unwrap(), message);

		// Incoming: transport -> supervisor -> muxer.
		transport_incoming_sender.send((AccountId::new([7; 32]), vec![4, 5, 6])).unwrap();
		assert_eq!(
			muxer_incoming_receiver.recv().await.unwrap(),
			(AccountId::new([7; 32]), vec![4, 5, 6])
		);

		supervisor.abort();
	}
}
