mod core;
mod muxer;
mod peer_info_submitter;

use std::{
	marker::PhantomData,
	net::{IpAddr, Ipv4Addr},
	sync::Arc,
};

use crate::{
	p2p::core::ed25519_secret_key_to_x25519_secret_key,
	settings::P2P as P2PSettings,
	state_chain_observer::client::{
		chain_api::ChainApi,
		extrinsic_api::signed::SignedExtrinsicApi,
		storage_api::StorageApi,
		stream_api::{StreamApi, FINALIZED},
	},
};

use self::core::X25519KeyPair;

pub use self::{
	core::{PeerInfo, PeerUpdate},
	muxer::{ProtocolVersion, VersionedCeremonyMessage, CURRENT_PROTOCOL_VERSION},
};
use anyhow::Context;
use cf_chains::{btc::BitcoinCrypto, dot::PolkadotCrypto, evm::EvmCrypto, ChainCrypto};
use cf_primitives::AccountId;
use futures::{Future, FutureExt, StreamExt};
use multisig::p2p::OutgoingMultisigStageMessages;
use muxer::P2PMuxer;
use sp_core::{ed25519, H256};
use tokio::sync::{
	mpsc::{UnboundedReceiver, UnboundedSender},
	oneshot,
};
use tracing::{error, info_span, Instrument};
use zeroize::Zeroizing;

use utilities::{read_clean_and_decode_hex_str_file, task_scope::task_scope};

type EdPublicKey = ed25519::Public;
type XPublicKey = x25519_dalek::PublicKey;

pub struct MultisigMessageSender<C: ChainCrypto>(
	pub UnboundedSender<OutgoingMultisigStageMessages>,
	PhantomData<C>,
);

impl<C: ChainCrypto> MultisigMessageSender<C> {
	pub fn new(sender: UnboundedSender<OutgoingMultisigStageMessages>) -> Self {
		MultisigMessageSender(sender, PhantomData)
	}
}
pub struct MultisigMessageReceiver<C: ChainCrypto>(
	pub UnboundedReceiver<(AccountId, VersionedCeremonyMessage)>,
	PhantomData<C>,
);

impl<C: ChainCrypto> MultisigMessageReceiver<C> {
	pub fn new(receiver: UnboundedReceiver<(AccountId, VersionedCeremonyMessage)>) -> Self {
		MultisigMessageReceiver(receiver, PhantomData)
	}
}

struct P2PKey {
	signing_key: ed25519_dalek::SigningKey,
	encryption_key: X25519KeyPair,
}

impl P2PKey {
	fn new(ed25519_secret_key: &ed25519_dalek::SecretKey) -> Self {
		let x_secret_key = ed25519_secret_key_to_x25519_secret_key(ed25519_secret_key);
		P2PKey {
			signing_key: ed25519_dalek::SigningKey::from_bytes(ed25519_secret_key),
			encryption_key: X25519KeyPair {
				public_key: (&x_secret_key).into(),
				secret_key: x_secret_key,
			},
		}
	}
}

fn pk_to_string(pk: &XPublicKey) -> String {
	hex::encode(pk.as_bytes())
}

pub async fn start<StateChainClient, BlockStream: StreamApi<FINALIZED>>(
	state_chain_client: Arc<StateChainClient>,
	sc_block_stream: BlockStream,
	settings: P2PSettings,
	initial_block_hash: H256,
) -> anyhow::Result<(
	MultisigMessageSender<EvmCrypto>,
	MultisigMessageReceiver<EvmCrypto>,
	MultisigMessageSender<PolkadotCrypto>,
	MultisigMessageReceiver<PolkadotCrypto>,
	MultisigMessageSender<BitcoinCrypto>,
	MultisigMessageReceiver<BitcoinCrypto>,
	oneshot::Receiver<()>,
	impl Future<Output = anyhow::Result<()>>,
)>
where
	StateChainClient: StorageApi + SignedExtrinsicApi + ChainApi + 'static + Send + Sync,
{
	if settings.ip_address == IpAddr::V4(Ipv4Addr::UNSPECIFIED) {
		anyhow::bail!("Should provide a valid IP address");
	}

	if !settings.allow_local_ip && !IpAddr::is_global(&settings.ip_address) {
		anyhow::bail!("Provided IP address is not globally routable");
	}

	let node_key = {
		let mut ed_secret_key = Zeroizing::new(ed25519_dalek::SecretKey::default());
		read_clean_and_decode_hex_str_file(&settings.node_key_file, "Node Key", |str| {
			hex::decode_to_slice(str, &mut ed_secret_key[..]).map_err(anyhow::Error::msg)
		})?;

		P2PKey::new(&ed_secret_key)
	};

	let current_peers =
		peer_info_submitter::get_current_peer_infos(&state_chain_client, initial_block_hash)
			.await
			.context("Failed to get initial peer info")?;
	let our_account_id = state_chain_client.account_id();

	let own_peer_info = current_peers.iter().find(|pi| pi.account_id == our_account_id).cloned();

	let (incoming_message_sender, incoming_message_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	let (outgoing_message_sender, outgoing_message_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	let (peer_update_sender, peer_update_receiver) = tokio::sync::mpsc::unbounded_channel();

	let (p2p_ready_sender, p2p_ready_receiver) = oneshot::channel();

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
			scope.spawn({
				let state_chain_client = state_chain_client.clone();
				async move {
					peer_info_submitter::ensure_peer_info_registered(
						&node_key,
						&state_chain_client,
						settings.ip_address,
						settings.port,
						own_peer_info,
					)
					.instrument(info_span!("P2PClient"))
					.await?;

					p2p_ready_sender.send(()).unwrap();

					core::start(
						node_key,
						settings.port,
						current_peers,
						our_account_id,
						incoming_message_sender,
						outgoing_message_receiver,
						peer_update_receiver,
					)
					.await;

					Ok(())
				}
			});

			scope.spawn(async move {
				muxer_future.await;
				Ok(())
			});

			scope.spawn(async move {
				monitor_p2p_registration_events(
					state_chain_client,
					sc_block_stream,
					peer_update_sender,
				)
				.await;
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
		p2p_ready_receiver,
		fut,
	))
}

/// Monitors the State Chain for peer registration events and sends them to the P2P client.
/// This is done separate to the SC Observer because we do not want to process events in the initial
/// block.
async fn monitor_p2p_registration_events<StateChainClient, BlockStream: StreamApi<FINALIZED>>(
	state_chain_client: Arc<StateChainClient>,
	sc_block_stream: BlockStream,
	peer_update_sender: UnboundedSender<PeerUpdate>,
) where
	StateChainClient: StorageApi + 'static + Send + Sync,
{
	use state_chain_runtime::Runtime;
	type CfeEvent = pallet_cf_cfe_interface::CfeEvent<Runtime>;

	let mut sc_block_stream = Box::pin(sc_block_stream);
	loop {
		match sc_block_stream.next().await {
			Some(current_block) => {
				if let Ok(events) = state_chain_client
					.storage_value::<pallet_cf_cfe_interface::CfeEvents<Runtime>>(
						current_block.hash,
					)
					.await
				{
					for event in events {
						match event {
							CfeEvent::PeerIdRegistered { account_id, pubkey, port, ip } => {
								peer_update_sender
									.send(PeerUpdate::Registered(PeerInfo::new(
										account_id,
										pubkey,
										ip.into(),
										port,
									)))
									.unwrap();
							},
							CfeEvent::PeerIdDeregistered { account_id, pubkey } => {
								peer_update_sender
									.send(PeerUpdate::Deregistered(account_id, pubkey))
									.unwrap();
							},
							_ => {
								// We only care about peer registration events
							},
						}
					}
				}
			},
			None => {
				error!("Exiting as State Chain block stream ended");
				break
			},
		}
	}
}
