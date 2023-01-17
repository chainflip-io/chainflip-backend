use anyhow::{anyhow, Result};
use futures::Future;
use state_chain_runtime::AccountId;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::{
	logging::COMPONENT_KEY,
	multisig::{eth::EthSigning, polkadot::PolkadotSigning, ChainTag},
	p2p::{MultisigMessageReceiver, MultisigMessageSender, OutgoingMultisigStageMessages},
};

pub struct P2PMuxer {
	all_incoming_receiver: UnboundedReceiver<(AccountId, Vec<u8>)>,
	all_outgoing_sender: UnboundedSender<OutgoingMultisigStageMessages>,
	eth_incoming_sender: UnboundedSender<(AccountId, Vec<u8>)>,
	eth_outgoing_receiver: UnboundedReceiver<OutgoingMultisigStageMessages>,
	dot_incoming_sender: UnboundedSender<(AccountId, Vec<u8>)>,
	dot_outgoing_receiver: UnboundedReceiver<OutgoingMultisigStageMessages>,
	logger: slog::Logger,
}

/// Top-level protocol message, encapsulates all others
struct VersionedMessage<'a> {
	version: u16,
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
		const VERSION_LEN: usize = std::mem::size_of::<u16>();

		let (version, payload) = split_header::<VERSION_LEN>(bytes)?;

		Ok(VersionedMessage { version: u16::from_be_bytes(*version), payload })
	}
}

/// Messages in protocol version 1 have this payload
struct TagPlusMessage<'a> {
	tag: ChainTag,
	payload: &'a [u8],
}

impl<'a> TagPlusMessage<'a> {
	fn serialize(&self) -> Vec<u8> {
		[&self.tag.to_bytes()[..], self.payload].concat()
	}

	fn deserialize(bytes: &'a [u8]) -> Result<Self> {
		const TAG_LEN: usize = std::mem::size_of::<ChainTag>();

		let (tag, payload) = split_header::<TAG_LEN>(bytes)?;

		let tag_num = u16::from_be_bytes(*tag);

		let tag: ChainTag = num_traits::FromPrimitive::from_u16(tag_num)
			.ok_or_else(|| anyhow!("unknown tag: {:?}", &tag_num))?;

		Ok(TagPlusMessage { tag, payload })
	}
}

/// The most recent (current) wire protocol version
const PROTOCOL_VERSION: u16 = 1;

fn add_tag_and_current_version(data: &[u8], tag: ChainTag) -> Vec<u8> {
	let with_tag = TagPlusMessage { tag, payload: data }.serialize();

	VersionedMessage { version: PROTOCOL_VERSION, payload: &with_tag }.serialize()
}

impl P2PMuxer {
	pub fn start(
		all_incoming_receiver: UnboundedReceiver<(AccountId, Vec<u8>)>,
		all_outgoing_sender: UnboundedSender<OutgoingMultisigStageMessages>,
		logger: &slog::Logger,
	) -> (
		MultisigMessageSender<EthSigning>,
		MultisigMessageReceiver<EthSigning>,
		MultisigMessageSender<PolkadotSigning>,
		MultisigMessageReceiver<PolkadotSigning>,
		impl Future<Output = ()>,
	) {
		let (eth_outgoing_sender, eth_outgoing_receiver) = tokio::sync::mpsc::unbounded_channel();
		let (eth_incoming_sender, eth_incoming_receiver) = tokio::sync::mpsc::unbounded_channel();

		let (dot_outgoing_sender, dot_outgoing_receiver) = tokio::sync::mpsc::unbounded_channel();
		let (dot_incoming_sender, dot_incoming_receiver) = tokio::sync::mpsc::unbounded_channel();

		let muxer = P2PMuxer {
			all_incoming_receiver,
			all_outgoing_sender,
			eth_outgoing_receiver,
			eth_incoming_sender,
			dot_outgoing_receiver,
			dot_incoming_sender,
			logger: logger.new(slog::o!(COMPONENT_KEY => "P2PMuxer")),
		};

		let muxer_fut = muxer.run();

		(
			MultisigMessageSender::<EthSigning>::new(eth_outgoing_sender),
			MultisigMessageReceiver::<EthSigning>::new(eth_incoming_receiver),
			MultisigMessageSender::<PolkadotSigning>::new(dot_outgoing_sender),
			MultisigMessageReceiver::<PolkadotSigning>::new(dot_incoming_receiver),
			muxer_fut,
		)
	}

	async fn process_incoming(&mut self, account_id: AccountId, data: Vec<u8>) {
		if let Ok(VersionedMessage { version, payload }) = VersionedMessage::deserialize(&data) {
			// only version 1 is expected/supported
			if version == PROTOCOL_VERSION {
				match TagPlusMessage::deserialize(payload) {
					Ok(TagPlusMessage { tag, payload }) => match tag {
						ChainTag::Ethereum => {
							self.eth_incoming_sender
								.send((account_id, payload.to_owned()))
								.expect("eth receiver dropped");
						},
						ChainTag::Polkadot => {
							self.dot_incoming_sender
								.send((account_id, payload.to_owned()))
								.expect("polkadot receiver dropped");
						},
						ChainTag::Sui => {
							slog::warn!(self.logger, "Sui chain tag is not supported yet")
						},
					},
					Err(err) => {
						slog::trace!(
							self.logger,
							"Could not deserialize tagged p2p message: {:?}",
							err
						);
					},
				}
			} else {
				slog::trace!(
					self.logger,
					"ignoring p2p message with unexpected version: {}",
					version
				);
			}
		}
	}

	async fn process_outgoing(
		&mut self,
		tag: ChainTag,
		mut messages: OutgoingMultisigStageMessages,
	) {
		match &mut messages {
			OutgoingMultisigStageMessages::Broadcast(_, data) => {
				*data = add_tag_and_current_version(data, tag);
			},
			OutgoingMultisigStageMessages::Private(messages) =>
				for (_, data) in messages {
					*data = add_tag_and_current_version(data, tag);
				},
		};

		self.all_outgoing_sender.send(messages).expect("receiver dropped")
	}

	pub async fn run(mut self) {
		loop {
			tokio::select! {
				Some((account_id, data)) = self.all_incoming_receiver.recv() => {
					self.process_incoming(account_id, data).await;
				}
				Some(data) = self.eth_outgoing_receiver.recv() => {
					self.process_outgoing(ChainTag::Ethereum, data).await;
				}
				Some(data) = self.dot_outgoing_receiver.recv() => {
					self.process_outgoing(ChainTag::Polkadot, data).await;
				}
			}
		}
	}
}

#[cfg(test)]
mod tests {

	use super::*;

	use crate::{p2p::OutgoingMultisigStageMessages, testing::expect_recv_with_timeout};

	const ACC_1: AccountId = AccountId::new([b'A'; 32]);
	const ACC_2: AccountId = AccountId::new([b'B'; 32]);

	const DATA_1: &[u8] = &[0, 1, 2];
	const DATA_2: &[u8] = &[3, 4, 5];

	const ETH_TAG_PREFIX: &[u8] = &ChainTag::Ethereum.to_bytes();
	const VERSION_PREFIX: &[u8] = &PROTOCOL_VERSION.to_be_bytes();

	#[tokio::test]
	async fn correctly_prepends_chain_tag_broadcast() {
		let logger = crate::logging::test_utils::new_test_logger();

		let (p2p_outgoing_sender, mut p2p_outgoing_receiver) =
			tokio::sync::mpsc::unbounded_channel();
		let (_, p2p_incoming_receiver) = tokio::sync::mpsc::unbounded_channel();

		let (eth_outgoing_sender, _, _, _, muxer_future) =
			P2PMuxer::start(p2p_incoming_receiver, p2p_outgoing_sender, &logger);

		let _jh = tokio::task::spawn(muxer_future);

		let message = OutgoingMultisigStageMessages::Broadcast(vec![ACC_1, ACC_2], DATA_1.to_vec());

		eth_outgoing_sender.0.send(message).unwrap();

		let received = expect_recv_with_timeout(&mut p2p_outgoing_receiver).await;

		let expected = {
			let expected_data = [VERSION_PREFIX, ETH_TAG_PREFIX, DATA_1].concat();

			OutgoingMultisigStageMessages::Broadcast(vec![ACC_1, ACC_2], expected_data)
		};

		assert_eq!(expected, received);
	}

	#[tokio::test]
	async fn correctly_prepends_chain_tag_private() {
		let logger = crate::logging::test_utils::new_test_logger();

		let (p2p_outgoing_sender, mut p2p_outgoing_receiver) =
			tokio::sync::mpsc::unbounded_channel();
		let (_, p2p_incoming_receiver) = tokio::sync::mpsc::unbounded_channel();

		let (eth_outgoing_sender, _, _, _, muxer_future) =
			P2PMuxer::start(p2p_incoming_receiver, p2p_outgoing_sender, &logger);

		let _jh = tokio::task::spawn(muxer_future);

		let message = OutgoingMultisigStageMessages::Private(vec![
			(ACC_1.clone(), DATA_1.to_vec()),
			(ACC_2.clone(), DATA_2.to_vec()),
		]);

		let expected = OutgoingMultisigStageMessages::Private(vec![
			(ACC_1, [VERSION_PREFIX, ETH_TAG_PREFIX, DATA_1].concat()),
			(ACC_2, [VERSION_PREFIX, ETH_TAG_PREFIX, DATA_2].concat()),
		]);

		eth_outgoing_sender.0.send(message).unwrap();

		let received = expect_recv_with_timeout(&mut p2p_outgoing_receiver).await;

		assert_eq!(expected, received);
	}

	/// Ensure that tag and version serialization produces the exact
	/// bytes that we expect
	#[tokio::test]
	async fn check_tag_and_version_serialization() {
		let res = add_tag_and_current_version(DATA_1, ChainTag::Ethereum);

		let version_bytes = [0x00, 0x01];
		let tag_bytes = [0x00, 0x00];

		assert_eq!(res, [&version_bytes, &tag_bytes, DATA_1].concat());
	}

	#[tokio::test]
	async fn should_parse_and_remove_headers() {
		let logger = crate::logging::test_utils::new_test_logger();

		let (p2p_outgoing_sender, _p2p_outgoing_receiver) = tokio::sync::mpsc::unbounded_channel();
		let (p2p_incoming_sender, p2p_incoming_receiver) = tokio::sync::mpsc::unbounded_channel();

		let (_eth_outgoing_sender, mut eth_incoming_receiver, _, _, muxer_future) =
			P2PMuxer::start(p2p_incoming_receiver, p2p_outgoing_sender, &logger);

		tokio::spawn(muxer_future);

		let bytes = [VERSION_PREFIX, ETH_TAG_PREFIX, DATA_1].concat();

		p2p_incoming_sender.send((ACC_1, bytes)).unwrap();

		let received = expect_recv_with_timeout(&mut eth_incoming_receiver.0).await;

		assert_eq!(received, (ACC_1, DATA_1.to_vec()));
	}
}
