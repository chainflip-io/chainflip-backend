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

//! Adapter types that bridge the generic P2P layer with the multisig protocol.
//!
//! The generic P2P layer deals with `OutgoingMessage` and `IncomingMessage` types,
//! while the multisig layer uses `OutgoingMultisigStageMessages` and
//! `VersionedCeremonyMessage`. This module provides the glue between them.

use std::marker::PhantomData;

use cf_chains::{
	btc::BitcoinCrypto, dot::PolkadotCrypto, evm::EvmCrypto, sol::SolanaCrypto, ChainCrypto,
};
use cf_primitives::AccountId;
use engine_p2p::{FairReceiver, OutgoingMessage, ProtocolHandle, Topic, TopicMuxer};
use multisig::{p2p::OutgoingMultisigStageMessages, ChainTag};
use tokio::sync::mpsc::{Receiver, UnboundedSender};

/// Bounded channel capacity feeding the ceremony manager. The ceremony manager is the slowest
/// consumer on the inbound path, so this stage uses a bounded channel with backpressure rather
/// than the per-peer fair channel used at earlier stages.
const FORWARDER_BUFFER_SIZE: usize = 4;

/// Wrapper around ChainTag that implements the Topic trait.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MultisigTopic(pub ChainTag);

impl Topic for MultisigTopic {
	fn topic_id(&self) -> engine_p2p::TopicId {
		self.0 as u16
	}
}

/// Sender for multisig messages, typed by the chain crypto.
///
/// This wraps a P2P protocol handle and translates multisig-specific message
/// types to the generic P2P format.
pub struct MultisigMessageSender<C: ChainCrypto> {
	sender: UnboundedSender<OutgoingMultisigStageMessages>,
	_phantom: PhantomData<C>,
}

impl<C: ChainCrypto> MultisigMessageSender<C> {
	pub fn new(sender: UnboundedSender<OutgoingMultisigStageMessages>) -> Self {
		Self { sender, _phantom: PhantomData }
	}

	/// Access the underlying sender for passing to the CeremonyManager
	pub fn inner(&self) -> UnboundedSender<OutgoingMultisigStageMessages> {
		self.sender.clone()
	}
}

/// Receiver for multisig messages, typed by the chain crypto.
pub struct MultisigMessageReceiver<C: ChainCrypto> {
	pub receiver: Receiver<(AccountId, multisig::p2p::VersionedCeremonyMessage)>,
	_phantom: PhantomData<C>,
}

impl<C: ChainCrypto> MultisigMessageReceiver<C> {
	pub fn new(receiver: Receiver<(AccountId, multisig::p2p::VersionedCeremonyMessage)>) -> Self {
		Self { receiver, _phantom: PhantomData }
	}

	/// Take ownership of the underlying receiver for passing to CeremonyManager
	pub fn into_inner(
		self,
	) -> Receiver<(AccountId, multisig::p2p::VersionedCeremonyMessage)> {
		self.receiver
	}
}

/// Create sender/receiver pair for a multisig protocol on a specific chain.
///
/// This sets up the channels and spawn tasks to translate between the generic
/// P2P message format and the multisig-specific message format.
pub fn create_multisig_channels<C: ChainCrypto>(
	handle: ProtocolHandle,
) -> (MultisigMessageSender<C>, MultisigMessageReceiver<C>) {
	// Channel for outgoing multisig messages (before adding topic header)
	let (outgoing_multisig_tx, mut outgoing_multisig_rx) =
		tokio::sync::mpsc::unbounded_channel::<OutgoingMultisigStageMessages>();

	// Bounded channel feeding the ceremony manager. Backpressure here lets the fair channels
	// upstream apply their per-peer drop policy rather than buffering unboundedly.
	let (incoming_multisig_tx, incoming_multisig_rx) =
		tokio::sync::mpsc::channel::<(AccountId, multisig::p2p::VersionedCeremonyMessage)>(
			FORWARDER_BUFFER_SIZE,
		);

	let outgoing_sender = handle.outgoing_sender;

	// Task to translate outgoing multisig messages to generic P2P format
	tokio::spawn(async move {
		while let Some(msg) = outgoing_multisig_rx.recv().await {
			let generic_msg = match msg {
				OutgoingMultisigStageMessages::Broadcast(recipients, payload) =>
					OutgoingMessage::Broadcast { recipients, payload },
				OutgoingMultisigStageMessages::Private(messages) =>
					OutgoingMessage::Private { messages },
			};
			if outgoing_sender.send(generic_msg).is_err() {
				break;
			}
		}
	});

	let mut incoming_receiver: FairReceiver<AccountId, _> = handle.incoming_receiver;

	// Task to translate incoming generic P2P messages to multisig format, with backpressure
	// to the fair channel upstream.
	tokio::spawn(async move {
		while let Some((sender, msg)) = incoming_receiver.recv().await {
			let versioned_msg = multisig::p2p::VersionedCeremonyMessage {
				version: msg.version,
				payload: msg.payload,
			};
			if incoming_multisig_tx.send((sender, versioned_msg)).await.is_err() {
				break;
			}
		}
	});

	(
		MultisigMessageSender::new(outgoing_multisig_tx),
		MultisigMessageReceiver::new(incoming_multisig_rx),
	)
}

/// All multisig topics for registering with the TopicMuxer.
const MULTISIG_TOPICS: [MultisigTopic; 4] = [
	MultisigTopic(ChainTag::Ethereum),
	MultisigTopic(ChainTag::Polkadot),
	MultisigTopic(ChainTag::Bitcoin),
	MultisigTopic(ChainTag::Solana),
];

/// Holds all per-chain multisig message channels.
///
/// This struct encapsulates the sender/receiver pairs for all supported chains,
/// providing type-safe access to the appropriate channels for each chain.
pub struct MultisigChannels {
	pub eth: (MultisigMessageSender<EvmCrypto>, MultisigMessageReceiver<EvmCrypto>),
	pub dot: (MultisigMessageSender<PolkadotCrypto>, MultisigMessageReceiver<PolkadotCrypto>),
	pub btc: (MultisigMessageSender<BitcoinCrypto>, MultisigMessageReceiver<BitcoinCrypto>),
	pub sol: (MultisigMessageSender<SolanaCrypto>, MultisigMessageReceiver<SolanaCrypto>),
}

impl MultisigChannels {
	/// Create multisig channels from the TopicMuxer.
	///
	/// This registers all multisig topics with the muxer and returns the channels
	/// along with the muxer future that must be spawned.
	pub fn new(
		incoming_receiver: FairReceiver<AccountId, Vec<u8>>,
		outgoing_sender: UnboundedSender<OutgoingMessage>,
	) -> (Self, impl std::future::Future<Output = ()>) {
		let (muxer_future, mut handles) =
			TopicMuxer::start(incoming_receiver, outgoing_sender, MULTISIG_TOPICS);

		let eth = create_multisig_channels::<EvmCrypto>(
			handles.remove(&MultisigTopic(ChainTag::Ethereum)).expect("topic registered"),
		);
		let dot = create_multisig_channels::<PolkadotCrypto>(
			handles.remove(&MultisigTopic(ChainTag::Polkadot)).expect("topic registered"),
		);
		let btc = create_multisig_channels::<BitcoinCrypto>(
			handles.remove(&MultisigTopic(ChainTag::Bitcoin)).expect("topic registered"),
		);
		let sol = create_multisig_channels::<SolanaCrypto>(
			handles.remove(&MultisigTopic(ChainTag::Solana)).expect("topic registered"),
		);

		(Self { eth, dot, btc, sol }, muxer_future)
	}
}
