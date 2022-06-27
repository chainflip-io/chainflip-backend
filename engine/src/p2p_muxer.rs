use futures::Future;
use serde::{Deserialize, Serialize};
use state_chain_runtime::AccountId;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::{
    logging::COMPONENT_KEY, multisig::ChainTag, multisig_p2p::OutgoingMultisigStageMessages,
};

pub struct P2PMuxer {
    all_incoming_receiver: UnboundedReceiver<(AccountId, Vec<u8>)>,
    all_outgoing_sender: UnboundedSender<OutgoingMultisigStageMessages>,
    eth_incoming_sender: UnboundedSender<(AccountId, Vec<u8>)>,
    eth_outgoing_receiver: UnboundedReceiver<OutgoingMultisigStageMessages>,
    logger: slog::Logger,
}

/// Top-level protocol message, encapsulates all others
#[derive(Serialize, Deserialize)]
struct VersionedMessage<'a> {
    version: u16,
    payload: &'a [u8],
}

/// Messages in protocol version 1 have this payload
#[derive(Serialize, Deserialize)]
struct TagPlusMessage {
    tag: ChainTag,
    payload: Vec<u8>,
}

/// The most recent (current) wire protocol version
const PROTOCOL_VERSION: u16 = 1;

fn add_tag_and_current_version(data: &mut Vec<u8>, tag: ChainTag) {
    let with_tag = bincode::serialize(&TagPlusMessage {
        tag,
        payload: std::mem::take(data),
    })
    .expect("serialization should not fail");
    let with_version = bincode::serialize(&VersionedMessage {
        version: PROTOCOL_VERSION,
        payload: &with_tag,
    })
    .expect("serialization should not fail");
    *data = with_version;
}

impl P2PMuxer {
    pub fn init(
        all_incoming_receiver: UnboundedReceiver<(AccountId, Vec<u8>)>,
        all_outgoing_sender: UnboundedSender<OutgoingMultisigStageMessages>,
        logger: &slog::Logger,
    ) -> (
        UnboundedSender<OutgoingMultisigStageMessages>,
        UnboundedReceiver<(AccountId, Vec<u8>)>,
        impl Future<Output = ()>,
    ) {
        let (eth_outgoing_sender, eth_outgoing_receiver) = tokio::sync::mpsc::unbounded_channel();
        let (eth_incoming_sender, eth_incoming_receiver) = tokio::sync::mpsc::unbounded_channel();

        let muxer = P2PMuxer {
            all_incoming_receiver,
            all_outgoing_sender,
            eth_outgoing_receiver,
            eth_incoming_sender,
            logger: logger.new(slog::o!(COMPONENT_KEY => "P2PMuxer")),
        };

        let muxer_fut = muxer.run();

        (eth_outgoing_sender, eth_incoming_receiver, muxer_fut)
    }

    async fn process_incoming(&mut self, account_id: AccountId, data: Vec<u8>) {
        if let Ok(VersionedMessage { version, payload }) = bincode::deserialize(&data) {
            // only version 1 is expected/supported
            if version == PROTOCOL_VERSION {
                if let Ok(TagPlusMessage { tag, payload }) = bincode::deserialize(payload) {
                    match tag {
                        ChainTag::Ethereum => {
                            self.eth_incoming_sender
                                .send((account_id, payload))
                                .expect("eth receiver dropped");
                        }
                        ChainTag::Polkadot => {
                            slog::trace!(
                                self.logger,
                                "ignoring p2p message: polkadot scheme not yet supported",
                            )
                        }
                    }
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
                add_tag_and_current_version(data, tag);
            }
            OutgoingMultisigStageMessages::Private(messages) => {
                for (_, data) in messages {
                    add_tag_and_current_version(data, tag);
                }
            }
        };

        self.all_outgoing_sender
            .send(messages)
            .expect("receiver dropped")
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
            }
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    use crate::engine_utils::test_utils::expect_recv_with_timeout;
    use crate::multisig_p2p::OutgoingMultisigStageMessages;

    #[tokio::test]
    async fn correctly_prepends_chain_tag() {
        let logger = crate::logging::test_utils::new_test_logger();

        let (p2p_outgoing_sender, mut p2p_outgoing_receiver) =
            tokio::sync::mpsc::unbounded_channel();
        let (_, p2p_incoming_receiver) = tokio::sync::mpsc::unbounded_channel();

        let (eth_outgoing_sender, _, muxer_future) =
            P2PMuxer::init(p2p_incoming_receiver, p2p_outgoing_sender, &logger);

        let _jh = tokio::task::spawn(muxer_future);

        let message = OutgoingMultisigStageMessages::Broadcast(
            vec![AccountId::new([b'A'; 32]), AccountId::new([b'B'; 32])],
            vec![0, 1, 2],
        );

        eth_outgoing_sender.send(message).unwrap();

        let message = expect_recv_with_timeout(&mut p2p_outgoing_receiver).await;

        let expected = {
            let expected_data = [&ChainTag::Ethereum.to_bytes()[..], &[0u8, 1, 2][..]].concat();

            OutgoingMultisigStageMessages::Broadcast(
                vec![AccountId::new([b'A'; 32]), AccountId::new([b'B'; 32])],
                expected_data,
            )
        };

        assert_eq!(expected, message);
    }
}
