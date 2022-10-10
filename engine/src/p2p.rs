mod p2p_core;
mod p2p_muxer;
mod peer_info_submitter;

use std::{marker::PhantomData, sync::Arc};

use crate::{
    common::read_clean_and_decode_hex_str_file,
    multisig::{eth::EthSigning, CryptoScheme},
    settings::P2P as P2PSettings,
};

use anyhow::Context;
use cf_primitives::AccountId;
use futures::{Future, FutureExt};
pub use p2p_core::{PeerInfo, PeerUpdate};
use p2p_muxer::P2PMuxer;
use sp_core::H256;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use zeroize::Zeroizing;

use crate::{
    state_chain_observer::client::{ChainflipClient, StateChainClient, StateChainRpcClient},
    task_scope::with_task_scope,
};

// TODO: Consider if this should be removed, particularly once we no longer use Substrate for peering
#[derive(Debug, PartialEq, Eq)]
pub enum OutgoingMultisigStageMessages {
    Broadcast(Vec<AccountId>, Vec<u8>),
    Private(Vec<(AccountId, Vec<u8>)>),
}

pub struct MultisigMessageSender<C: CryptoScheme>(
    pub UnboundedSender<OutgoingMultisigStageMessages>,
    PhantomData<C>,
);

impl<C: CryptoScheme> MultisigMessageSender<C> {
    pub fn new(sender: UnboundedSender<OutgoingMultisigStageMessages>) -> Self {
        MultisigMessageSender(sender, PhantomData)
    }
}
pub struct MultisigMessageReceiver<C: CryptoScheme>(
    pub UnboundedReceiver<(AccountId, Vec<u8>)>,
    PhantomData<C>,
);

impl<C: CryptoScheme> MultisigMessageReceiver<C> {
    pub fn new(receiver: UnboundedReceiver<(AccountId, Vec<u8>)>) -> Self {
        MultisigMessageReceiver(receiver, PhantomData)
    }
}

pub async fn start(
    state_chain_client: Arc<
        StateChainClient<StateChainRpcClient<impl ChainflipClient + Send + Sync + 'static>>,
    >,
    settings: P2PSettings,
    latest_block_hash: H256,
    logger: &slog::Logger,
) -> anyhow::Result<(
    MultisigMessageSender<EthSigning>,
    MultisigMessageReceiver<EthSigning>,
    UnboundedSender<PeerUpdate>,
    impl Future<Output = anyhow::Result<()>>,
)> {
    let node_key = {
        let secret =
            read_clean_and_decode_hex_str_file(&settings.node_key_file, "Node Key", |str| {
                ed25519_dalek::SecretKey::from_bytes(
                    &Zeroizing::new(hex::decode(str).map_err(anyhow::Error::new)?)[..],
                )
                .map_err(anyhow::Error::new)
            })?;

        let public = (&secret).into();
        ed25519_dalek::Keypair { secret, public }
    };

    let current_peers =
        peer_info_submitter::get_current_peer_infos(&state_chain_client, latest_block_hash)
            .await
            .context("Failed to get initial peer info")?;
    let our_account_id = state_chain_client.our_account_id.clone();

    let own_peer_info = current_peers
        .iter()
        .find(|pi| pi.account_id == our_account_id)
        .cloned();

    let (
        outgoing_message_sender,
        peer_update_sender,
        incoming_message_receiver,
        own_peer_info_receiver,
        p2p_fut,
    ) = p2p_core::start(
        &node_key,
        settings.port,
        current_peers,
        our_account_id,
        logger,
    );

    let (eth_outgoing_sender, eth_incoming_receiver, muxer_future) =
        P2PMuxer::start(incoming_message_receiver, outgoing_message_sender, &logger);

    let logger = logger.clone();

    let fut = with_task_scope(move |scope| {
        let fut = async move {
            scope.spawn(async {
                p2p_fut.await;
                Ok(())
            });

            scope.spawn(peer_info_submitter::start(
                node_key,
                state_chain_client,
                settings.ip_address,
                settings.port,
                own_peer_info,
                own_peer_info_receiver,
                logger,
            ));

            scope.spawn(async move {
                muxer_future.await;
                Ok(())
            });

            Ok(())
        }
        .boxed();
        fut
    });

    Ok((
        eth_outgoing_sender,
        eth_incoming_receiver,
        peer_update_sender,
        fut,
    ))
}
