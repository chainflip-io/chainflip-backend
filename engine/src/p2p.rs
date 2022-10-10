mod p2p_core;

use std::sync::Arc;

use crate::settings::P2P as P2PSettings;
use anyhow::Context;
use cf_primitives::AccountId;
use futures::{Future, FutureExt};
pub use p2p_core::{PeerInfo, PeerUpdate};
use sp_core::H256;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::{
    multisig_p2p::{self, OutgoingMultisigStageMessages},
    state_chain_observer::client::{ChainflipClient, StateChainClient, StateChainRpcClient},
    task_scope::with_task_scope,
};

pub async fn start(
    node_key: ed25519_dalek::Keypair,
    state_chain_client: Arc<
        StateChainClient<StateChainRpcClient<impl ChainflipClient + Send + Sync + 'static>>,
    >,
    settings: P2PSettings,
    latest_block_hash: H256,
    logger: &slog::Logger,
) -> anyhow::Result<(
    UnboundedSender<OutgoingMultisigStageMessages>,
    UnboundedSender<PeerUpdate>,
    UnboundedReceiver<(AccountId, Vec<u8>)>,
    impl Future<Output = anyhow::Result<()>>,
)> {
    let current_peers =
        multisig_p2p::get_current_peer_infos(&state_chain_client, latest_block_hash)
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

    let logger = logger.clone();

    let fut = with_task_scope(move |scope| {
        let fut = async move {
            scope.spawn(async {
                p2p_fut.await;
                Ok(())
            });

            scope.spawn(multisig_p2p::start(
                node_key,
                state_chain_client,
                settings.ip_address,
                settings.port,
                own_peer_info,
                own_peer_info_receiver,
                logger,
            ));

            Ok(())
        }
        .boxed();
        fut
    });

    Ok((
        outgoing_message_sender,
        peer_update_sender,
        incoming_message_receiver,
        fut,
    ))
}
