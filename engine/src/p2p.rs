mod core;
mod muxer;
mod peer_info_submitter;

use std::{
	marker::PhantomData,
	net::{IpAddr, Ipv4Addr},
	sync::Arc,
};

use crate::{
	common::read_clean_and_decode_hex_str_file,
	multisig::{bitcoin::BtcSigning, eth::EthSigning, polkadot::PolkadotSigning, CryptoScheme},
	settings::P2P as P2PSettings,
	state_chain_observer::client::{extrinsic_api::ExtrinsicApi, storage_api::StorageApi},
};

pub use self::{
	core::{PeerInfo, PeerUpdate},
	muxer::{ProtocolVersion, VersionedCeremonyMessage, CURRENT_PROTOCOL_VERSION},
};
use anyhow::Context;
use cf_primitives::AccountId;
use futures::{Future, FutureExt};
use muxer::P2PMuxer;
use sp_core::H256;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tracing::{info_span, Instrument};
use zeroize::Zeroizing;

use crate::task_scope::task_scope;

// TODO: Consider if this should be removed, particularly once we no longer use Substrate for
// peering
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
	pub UnboundedReceiver<(AccountId, VersionedCeremonyMessage)>,
	PhantomData<C>,
);

impl<C: CryptoScheme> MultisigMessageReceiver<C> {
	pub fn new(receiver: UnboundedReceiver<(AccountId, VersionedCeremonyMessage)>) -> Self {
		MultisigMessageReceiver(receiver, PhantomData)
	}
}

pub async fn start<StateChainClient>(
	state_chain_client: Arc<StateChainClient>,
	settings: P2PSettings,
	latest_block_hash: H256,
) -> anyhow::Result<(
	MultisigMessageSender<EthSigning>,
	MultisigMessageReceiver<EthSigning>,
	MultisigMessageSender<PolkadotSigning>,
	MultisigMessageReceiver<PolkadotSigning>,
	MultisigMessageSender<BtcSigning>,
	MultisigMessageReceiver<BtcSigning>,
	UnboundedSender<PeerUpdate>,
	impl Future<Output = anyhow::Result<()>>,
)>
where
	StateChainClient: StorageApi + ExtrinsicApi + 'static + Send + Sync,
{
	if settings.ip_address == IpAddr::V4(Ipv4Addr::UNSPECIFIED) {
		anyhow::bail!("Should provide a valid IP address");
	}

	if !settings.allow_local_ip && !IpAddr::is_global(&settings.ip_address) {
		anyhow::bail!("Provided IP address is not globally routable");
	}

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
	let our_account_id = state_chain_client.account_id();

	let own_peer_info = current_peers.iter().find(|pi| pi.account_id == our_account_id).cloned();

	let (
		outgoing_message_sender,
		peer_update_sender,
		incoming_message_receiver,
		own_peer_info_receiver,
		p2p_fut,
	) = core::start(&node_key, settings.port, current_peers, our_account_id);

	let (
		eth_outgoing_sender,
		eth_incoming_receiver,
		dot_outgoing_sender,
		dot_incoming_receiver,
		btc_outgoing_sender,
		btc_incoming_receiver,
		muxer_future,
	) = P2PMuxer::start(incoming_message_receiver, outgoing_message_sender);

	let fut = task_scope(move |scope| {
		async move {
			scope.spawn(async {
				p2p_fut.await;
				Ok(())
			});

			scope.spawn(
				peer_info_submitter::start(
					node_key,
					state_chain_client,
					settings.ip_address,
					settings.port,
					own_peer_info,
					own_peer_info_receiver,
				)
				.instrument(info_span!("P2PClient")),
			);

			scope.spawn(async move {
				muxer_future.await;
				Ok(())
			});

			Ok(())
		}
		.boxed()
	});

	Ok((
		eth_outgoing_sender,
		eth_incoming_receiver,
		dot_outgoing_sender,
		dot_incoming_receiver,
		btc_outgoing_sender,
		btc_incoming_receiver,
		peer_update_sender,
		fut,
	))
}
