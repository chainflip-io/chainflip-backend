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

use std::collections::HashMap;

use crate::AccountId;
use anyhow::{anyhow, Result};
use futures::Future;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tracing::{info_span, trace, Instrument};

use crate::message::{
	IncomingMessage, OutgoingMessage, ProtocolVersion, TopicId, CURRENT_PROTOCOL_VERSION,
};
use cf_utilities::metrics::P2P_BAD_MSG;

/// Trait for types that represent P2P protocol topics.
///
/// Implement this trait to define a set of topics for your protocol.
/// The topic ID is used for message routing on the wire.
pub trait Topic: Copy {
	fn topic_id(&self) -> TopicId;
}

/// A handle for a registered protocol to send/receive messages
pub struct ProtocolHandle {
	pub topic: TopicId,
	pub outgoing_sender: UnboundedSender<OutgoingMessage>,
	pub incoming_receiver: UnboundedReceiver<IncomingMessage>,
}

/// Generic topic-based message router.
///
/// Routes messages between the raw P2P layer and protocol-specific handlers.
/// Each protocol registers for a topic and gets a dedicated channel pair.
pub struct TopicMuxer {
	all_incoming_receiver: UnboundedReceiver<(AccountId, Vec<u8>)>,
	all_outgoing_sender: UnboundedSender<OutgoingMessage>,
	topic_senders: HashMap<TopicId, UnboundedSender<IncomingMessage>>,
	topic_receivers: Vec<(TopicId, UnboundedReceiver<OutgoingMessage>)>,
}

/// Top-level protocol message, encapsulates all others
struct VersionedMessage<'a> {
	version: ProtocolVersion,
	payload: &'a [u8],
}

fn split_header<const HEADER_LEN: usize>(buffer: &[u8]) -> Result<(&[u8; HEADER_LEN], &[u8])> {
	if buffer.len() >= HEADER_LEN {
		let (header, payload) = buffer.split_at(HEADER_LEN);
		let header: &[u8; HEADER_LEN] = header.try_into().expect("unexpected size");
		Ok((header, payload))
	} else {
		Err(anyhow!("unexpected buffer len: {}", buffer.len()))
	}
}

impl<'a> VersionedMessage<'a> {
	fn serialize(&self) -> Vec<u8> {
		[&self.version.to_be_bytes()[..], self.payload].concat()
	}

	fn deserialize(bytes: &'a [u8]) -> Result<Self> {
		const VERSION_LEN: usize = std::mem::size_of::<ProtocolVersion>();

		let (version, payload) = split_header::<VERSION_LEN>(bytes)?;

		Ok(VersionedMessage { version: ProtocolVersion::from_be_bytes(*version), payload })
	}
}

/// Messages in protocol version 1 have this payload
struct TopicMessage<'a> {
	topic: TopicId,
	payload: &'a [u8],
}

impl<'a> TopicMessage<'a> {
	fn serialize(&self) -> Vec<u8> {
		[&self.topic.to_be_bytes()[..], self.payload].concat()
	}

	fn deserialize(bytes: &'a [u8]) -> Result<Self> {
		const TOPIC_LEN: usize = std::mem::size_of::<TopicId>();

		let (topic, payload) = split_header::<TOPIC_LEN>(bytes)?;

		Ok(TopicMessage { topic: u16::from_be_bytes(*topic), payload })
	}
}

fn add_topic_and_current_version(data: &[u8], topic: TopicId) -> Vec<u8> {
	let with_topic = TopicMessage { topic, payload: data }.serialize();

	VersionedMessage { version: CURRENT_PROTOCOL_VERSION, payload: &with_topic }.serialize()
}

impl TopicMuxer {
	/// Create a new TopicMuxer and register topics.
	///
	/// Returns the muxer future and a map of topics to protocol handles.
	/// The map is keyed by the topic type itself, allowing type-safe handle retrieval.
	pub fn start<T: Topic + Eq + std::hash::Hash>(
		all_incoming_receiver: UnboundedReceiver<(AccountId, Vec<u8>)>,
		all_outgoing_sender: UnboundedSender<OutgoingMessage>,
		topics: impl IntoIterator<Item = T>,
	) -> (impl Future<Output = ()>, HashMap<T, ProtocolHandle>) {
		let mut topic_senders = HashMap::new();
		let mut topic_receivers = Vec::new();
		let mut handles = HashMap::new();

		for topic in topics {
			let topic_id = topic.topic_id();
			let (outgoing_sender, outgoing_receiver) = tokio::sync::mpsc::unbounded_channel();
			let (incoming_sender, incoming_receiver) = tokio::sync::mpsc::unbounded_channel();

			topic_senders.insert(topic_id, incoming_sender);
			topic_receivers.push((topic_id, outgoing_receiver));

			handles.insert(
				topic,
				ProtocolHandle { topic: topic_id, outgoing_sender, incoming_receiver },
			);
		}

		let muxer = TopicMuxer {
			all_incoming_receiver,
			all_outgoing_sender,
			topic_senders,
			topic_receivers,
		};

		let muxer_fut = muxer.run().instrument(info_span!("TopicMuxer"));

		(muxer_fut, handles)
	}

	async fn process_incoming(&mut self, account_id: AccountId, data: Vec<u8>) {
		if let Ok(VersionedMessage { version, payload }) = VersionedMessage::deserialize(&data) {
			// only version 1 is expected/supported
			if version == CURRENT_PROTOCOL_VERSION {
				match TopicMessage::deserialize(payload) {
					Ok(TopicMessage { topic, payload }) => {
						if let Some(sender) = self.topic_senders.get(&topic) {
							let message = IncomingMessage {
								sender: account_id,
								topic,
								version,
								payload: payload.to_vec(),
							};
							if sender.send(message).is_err() {
								trace!("Topic {topic} receiver dropped");
							}
						} else {
							P2P_BAD_MSG.inc(&["unknown_topic"]);
							trace!("Received message for unknown topic: {topic}");
						}
					},
					Err(e) => {
						P2P_BAD_MSG.inc(&["deserialization_topic_msg"]);
						trace!("Could not deserialize topic p2p message: {e:?}",);
					},
				}
			} else {
				P2P_BAD_MSG.inc(&["unexpected_version"]);
				trace!("ignoring p2p message with unexpected version: {version}",);
			}
		} else {
			P2P_BAD_MSG.inc(&["deserialization_versioned_msg"]);
		}
	}

	fn process_outgoing(&self, topic: TopicId, mut outgoing: OutgoingMessage) {
		match &mut outgoing {
			OutgoingMessage::Broadcast { payload, .. } => {
				*payload = add_topic_and_current_version(payload, topic);
			},
			OutgoingMessage::Private { messages } =>
				for (_, data) in messages {
					*data = add_topic_and_current_version(data, topic);
				},
		};

		if self.all_outgoing_sender.send(outgoing).is_err() {
			trace!("Outgoing transport receiver dropped; discarding message");
		}
	}

	pub async fn run(mut self) {
		/// Poll all topic receivers and return the first available message
		async fn poll_topic_receivers(
			receivers: &mut [(TopicId, UnboundedReceiver<OutgoingMessage>)],
		) -> Option<(TopicId, OutgoingMessage)> {
			use futures::future::poll_fn;
			use std::task::Poll;

			poll_fn(|cx| {
				for (topic, receiver) in receivers.iter_mut() {
					match receiver.poll_recv(cx) {
						Poll::Ready(Some(msg)) => return Poll::Ready(Some((*topic, msg))),
						// A closed channel means that topic's protocol has shut down. Skip it
						// rather than returning `None`, which would disable the whole
						// `select!` arm and stop outgoing routing for every other topic too.
						Poll::Ready(None) => continue,
						Poll::Pending => continue,
					}
				}
				Poll::Pending
			})
			.await
		}

		loop {
			tokio::select! {
				Some((topic, msg)) = poll_topic_receivers(&mut self.topic_receivers) => {
					self.process_outgoing(topic, msg);
				}
				Some((account_id, data)) = self.all_incoming_receiver.recv() => {
					self.process_incoming(account_id, data).await;
				}
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use cf_utilities::testing::{expect_recv_with_timeout, recv_with_timeout};

	use super::*;

	const ACC_1: AccountId = AccountId::new([b'A'; 32]);
	const ACC_2: AccountId = AccountId::new([b'B'; 32]);

	const DATA_1: &[u8] = &[0, 1, 2];
	const DATA_2: &[u8] = &[3, 4, 5];

	const TOPIC_ETH: TopicId = 0x0000;
	const TOPIC_DOT: TopicId = 0x0001;
	const VERSION_PREFIX: &[u8] = &CURRENT_PROTOCOL_VERSION.to_be_bytes();

	/// Test topic type for unit tests
	#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
	struct TestTopic(TopicId);

	impl Topic for TestTopic {
		fn topic_id(&self) -> TopicId {
			self.0
		}
	}

	const ETH_TOPIC: TestTopic = TestTopic(TOPIC_ETH);
	const DOT_TOPIC: TestTopic = TestTopic(TOPIC_DOT);

	fn topic_prefix(topic: TopicId) -> [u8; 2] {
		topic.to_be_bytes()
	}

	fn bad_msg_count(reason: &str) -> u64 {
		P2P_BAD_MSG
			.prom_metric
			.get_metric_with_label_values(&[reason])
			.expect("metric label exists")
			.get()
	}

	async fn wait_for_bad_msg_increment(reason: &str, before: u64) {
		let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(1);
		loop {
			if bad_msg_count(reason) >= before + 1 {
				return;
			}
			if tokio::time::Instant::now() >= deadline {
				panic!("Metric did not increment: {reason}");
			}
			tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;
		}
	}

	#[tokio::test]
	async fn correctly_prepends_topic_broadcast() {
		let (p2p_outgoing_sender, mut p2p_outgoing_receiver) =
			tokio::sync::mpsc::unbounded_channel();
		let (_incoming_sender, p2p_incoming_receiver) = tokio::sync::mpsc::unbounded_channel();

		let (muxer_future, mut handles) =
			TopicMuxer::start(p2p_incoming_receiver, p2p_outgoing_sender, [ETH_TOPIC]);

		let _jh = tokio::task::spawn(muxer_future);

		let eth_handle = handles.remove(&ETH_TOPIC).unwrap();

		let message =
			OutgoingMessage::Broadcast { recipients: vec![ACC_1, ACC_2], payload: DATA_1.to_vec() };

		eth_handle.outgoing_sender.send(message).unwrap();

		let received = expect_recv_with_timeout(&mut p2p_outgoing_receiver).await;

		let expected_data = [VERSION_PREFIX, &topic_prefix(TOPIC_ETH), DATA_1].concat();

		let expected =
			OutgoingMessage::Broadcast { recipients: vec![ACC_1, ACC_2], payload: expected_data };

		assert_eq!(expected, received);
	}

	#[tokio::test]
	async fn correctly_prepends_topic_private() {
		let (p2p_outgoing_sender, mut p2p_outgoing_receiver) =
			tokio::sync::mpsc::unbounded_channel();
		let (_incoming_sender, p2p_incoming_receiver) = tokio::sync::mpsc::unbounded_channel();

		let (muxer_future, mut handles) =
			TopicMuxer::start(p2p_incoming_receiver, p2p_outgoing_sender, [ETH_TOPIC]);

		let _jh = tokio::task::spawn(muxer_future);

		let eth_handle = handles.remove(&ETH_TOPIC).unwrap();

		let message = OutgoingMessage::Private {
			messages: vec![(ACC_1.clone(), DATA_1.to_vec()), (ACC_2.clone(), DATA_2.to_vec())],
		};

		let expected = OutgoingMessage::Private {
			messages: vec![
				(ACC_1, [VERSION_PREFIX, &topic_prefix(TOPIC_ETH), DATA_1].concat()),
				(ACC_2, [VERSION_PREFIX, &topic_prefix(TOPIC_ETH), DATA_2].concat()),
			],
		};

		eth_handle.outgoing_sender.send(message).unwrap();

		let received = expect_recv_with_timeout(&mut p2p_outgoing_receiver).await;

		assert_eq!(expected, received);
	}

	/// Ensure that topic and version serialization produces the exact
	/// bytes that we expect
	#[tokio::test]
	async fn check_topic_and_version_serialization() {
		let res = add_topic_and_current_version(DATA_1, TOPIC_ETH);

		let version_bytes: [u8; 2] = CURRENT_PROTOCOL_VERSION.to_be_bytes();
		let topic_bytes = [0x00, 0x00];

		assert_eq!(res, [&version_bytes, &topic_bytes, DATA_1].concat());
	}

	#[tokio::test]
	async fn should_parse_and_remove_headers() {
		let (p2p_outgoing_sender, _p2p_outgoing_receiver) = tokio::sync::mpsc::unbounded_channel();
		let (p2p_incoming_sender, p2p_incoming_receiver) = tokio::sync::mpsc::unbounded_channel();

		let (muxer_future, mut handles) =
			TopicMuxer::start(p2p_incoming_receiver, p2p_outgoing_sender, [ETH_TOPIC]);

		tokio::spawn(muxer_future);

		let mut eth_handle = handles.remove(&ETH_TOPIC).unwrap();

		let bytes = [VERSION_PREFIX, &topic_prefix(TOPIC_ETH), DATA_1].concat();

		p2p_incoming_sender.send((ACC_1, bytes)).unwrap();

		let received = expect_recv_with_timeout(&mut eth_handle.incoming_receiver).await;

		assert_eq!(received.sender, ACC_1);
		assert_eq!(received.payload, DATA_1.to_vec());
		assert_eq!(received.topic, TOPIC_ETH);
	}

	#[tokio::test]
	async fn unknown_topic_is_dropped_and_counted() {
		let (p2p_outgoing_sender, _p2p_outgoing_receiver) = tokio::sync::mpsc::unbounded_channel();
		let (p2p_incoming_sender, p2p_incoming_receiver) = tokio::sync::mpsc::unbounded_channel();

		let (muxer_future, mut handles) =
			TopicMuxer::start(p2p_incoming_receiver, p2p_outgoing_sender, [ETH_TOPIC]);
		tokio::spawn(muxer_future);

		let mut eth_handle = handles.remove(&ETH_TOPIC).unwrap();

		let before = bad_msg_count("unknown_topic");
		let unknown_topic = TOPIC_DOT;
		let bytes = add_topic_and_current_version(DATA_1, unknown_topic);

		p2p_incoming_sender.send((ACC_1, bytes)).unwrap();

		assert!(recv_with_timeout(&mut eth_handle.incoming_receiver).await.is_none());
		wait_for_bad_msg_increment("unknown_topic", before).await;
	}

	#[tokio::test]
	async fn unexpected_version_is_dropped_and_counted() {
		let (p2p_outgoing_sender, _p2p_outgoing_receiver) = tokio::sync::mpsc::unbounded_channel();
		let (p2p_incoming_sender, p2p_incoming_receiver) = tokio::sync::mpsc::unbounded_channel();

		let (muxer_future, mut handles) =
			TopicMuxer::start(p2p_incoming_receiver, p2p_outgoing_sender, [ETH_TOPIC]);
		tokio::spawn(muxer_future);

		let mut eth_handle = handles.remove(&ETH_TOPIC).unwrap();

		let before = bad_msg_count("unexpected_version");
		let mut bytes = add_topic_and_current_version(DATA_1, TOPIC_ETH);
		let wrong_version = CURRENT_PROTOCOL_VERSION.saturating_add(1).to_be_bytes();
		bytes[0..2].copy_from_slice(&wrong_version);

		p2p_incoming_sender.send((ACC_1, bytes)).unwrap();

		assert!(recv_with_timeout(&mut eth_handle.incoming_receiver).await.is_none());
		wait_for_bad_msg_increment("unexpected_version", before).await;
	}

	#[tokio::test]
	async fn malformed_version_header_is_counted() {
		let (p2p_outgoing_sender, _p2p_outgoing_receiver) = tokio::sync::mpsc::unbounded_channel();
		let (p2p_incoming_sender, p2p_incoming_receiver) = tokio::sync::mpsc::unbounded_channel();

		let (muxer_future, mut handles) =
			TopicMuxer::start(p2p_incoming_receiver, p2p_outgoing_sender, [ETH_TOPIC]);
		tokio::spawn(muxer_future);

		let mut eth_handle = handles.remove(&ETH_TOPIC).unwrap();

		let before = bad_msg_count("deserialization_versioned_msg");
		p2p_incoming_sender.send((ACC_1, vec![0x00])).unwrap();

		assert!(recv_with_timeout(&mut eth_handle.incoming_receiver).await.is_none());
		wait_for_bad_msg_increment("deserialization_versioned_msg", before).await;
	}

	#[tokio::test]
	async fn malformed_topic_header_is_counted() {
		let (p2p_outgoing_sender, _p2p_outgoing_receiver) = tokio::sync::mpsc::unbounded_channel();
		let (p2p_incoming_sender, p2p_incoming_receiver) = tokio::sync::mpsc::unbounded_channel();

		let (muxer_future, mut handles) =
			TopicMuxer::start(p2p_incoming_receiver, p2p_outgoing_sender, [ETH_TOPIC]);
		tokio::spawn(muxer_future);

		let mut eth_handle = handles.remove(&ETH_TOPIC).unwrap();

		let before = bad_msg_count("deserialization_topic_msg");
		let bytes = CURRENT_PROTOCOL_VERSION.to_be_bytes().to_vec();

		p2p_incoming_sender.send((ACC_1, bytes)).unwrap();

		assert!(recv_with_timeout(&mut eth_handle.incoming_receiver).await.is_none());
		wait_for_bad_msg_increment("deserialization_topic_msg", before).await;
	}

	#[tokio::test]
	async fn routes_outgoing_messages_for_multiple_topics() {
		let (p2p_outgoing_sender, mut p2p_outgoing_receiver) =
			tokio::sync::mpsc::unbounded_channel();
		let (_incoming_sender, p2p_incoming_receiver) = tokio::sync::mpsc::unbounded_channel();

		let (muxer_future, mut handles) =
			TopicMuxer::start(p2p_incoming_receiver, p2p_outgoing_sender, [ETH_TOPIC, DOT_TOPIC]);
		tokio::task::spawn(muxer_future);

		let eth_handle = handles.remove(&ETH_TOPIC).unwrap();
		let dot_handle = handles.remove(&DOT_TOPIC).unwrap();

		eth_handle
			.outgoing_sender
			.send(OutgoingMessage::Broadcast { recipients: vec![ACC_1], payload: DATA_1.to_vec() })
			.unwrap();
		dot_handle
			.outgoing_sender
			.send(OutgoingMessage::Broadcast { recipients: vec![ACC_2], payload: DATA_2.to_vec() })
			.unwrap();

		let first = expect_recv_with_timeout(&mut p2p_outgoing_receiver).await;
		let second = expect_recv_with_timeout(&mut p2p_outgoing_receiver).await;

		let expected_eth = OutgoingMessage::Broadcast {
			recipients: vec![ACC_1],
			payload: [VERSION_PREFIX, &topic_prefix(TOPIC_ETH), DATA_1].concat(),
		};
		let expected_dot = OutgoingMessage::Broadcast {
			recipients: vec![ACC_2],
			payload: [VERSION_PREFIX, &topic_prefix(TOPIC_DOT), DATA_2].concat(),
		};

		assert!(
			(first == expected_eth && second == expected_dot) ||
				(first == expected_dot && second == expected_eth)
		);
	}

	/// A protocol shutting down closes its topic's outgoing channel. This must
	/// not stop the muxer from routing outgoing messages for other topics.
	#[tokio::test]
	async fn closing_one_topic_does_not_stop_other_topics() {
		let (p2p_outgoing_sender, mut p2p_outgoing_receiver) =
			tokio::sync::mpsc::unbounded_channel();
		let (_incoming_sender, p2p_incoming_receiver) = tokio::sync::mpsc::unbounded_channel();

		let (muxer_future, mut handles) =
			TopicMuxer::start(p2p_incoming_receiver, p2p_outgoing_sender, [ETH_TOPIC, DOT_TOPIC]);
		tokio::task::spawn(muxer_future);

		let eth_handle = handles.remove(&ETH_TOPIC).unwrap();
		let dot_handle = handles.remove(&DOT_TOPIC).unwrap();

		// Simulate the ETH protocol shutting down: dropping its handle closes the
		// ETH topic's outgoing channel.
		drop(eth_handle);

		// The DOT topic must still be able to send.
		dot_handle
			.outgoing_sender
			.send(OutgoingMessage::Broadcast { recipients: vec![ACC_2], payload: DATA_2.to_vec() })
			.unwrap();

		let received = expect_recv_with_timeout(&mut p2p_outgoing_receiver).await;
		let expected = OutgoingMessage::Broadcast {
			recipients: vec![ACC_2],
			payload: [VERSION_PREFIX, &topic_prefix(TOPIC_DOT), DATA_2].concat(),
		};
		assert_eq!(received, expected);
	}

	/// If the downstream transport receiver is dropped (e.g. the transport task
	/// exited), sending an outgoing message must not panic the muxer.
	#[tokio::test]
	async fn dropped_outgoing_receiver_does_not_crash_muxer() {
		let (p2p_outgoing_sender, p2p_outgoing_receiver) = tokio::sync::mpsc::unbounded_channel();
		let (p2p_incoming_sender, p2p_incoming_receiver) = tokio::sync::mpsc::unbounded_channel();

		let (muxer_future, mut handles) =
			TopicMuxer::start(p2p_incoming_receiver, p2p_outgoing_sender, [ETH_TOPIC]);
		let muxer_handle = tokio::task::spawn(muxer_future);

		let mut eth_handle = handles.remove(&ETH_TOPIC).unwrap();

		// The downstream transport goes away.
		drop(p2p_outgoing_receiver);

		// Sending an outgoing message must not panic the muxer task.
		eth_handle
			.outgoing_sender
			.send(OutgoingMessage::Broadcast { recipients: vec![ACC_1], payload: DATA_1.to_vec() })
			.unwrap();

		// Give the muxer time to process the outgoing message (and, with the bug, panic).
		tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

		// The muxer should still be alive and routing incoming messages.
		let bytes = [VERSION_PREFIX, &topic_prefix(TOPIC_ETH), DATA_1].concat();
		p2p_incoming_sender.send((ACC_1, bytes)).unwrap();
		let received = expect_recv_with_timeout(&mut eth_handle.incoming_receiver).await;
		assert_eq!(received.payload, DATA_1.to_vec());

		assert!(!muxer_handle.is_finished(), "muxer task should still be running");
	}
}
