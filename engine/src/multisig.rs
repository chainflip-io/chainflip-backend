use anyhow::Result;
use cf_primitives::CeremonyId;

use multisig::{ChainSigning, MultisigClient};
use tracing::{info, info_span, Instrument};

use crate::{
	db::KeyStore,
	p2p::{MultisigMessageReceiver, MultisigMessageSender},
};
use state_chain_runtime::AccountId;

/// Start the multisig client, which listens for p2p messages and requests from the SC
pub fn start_client<C: ChainSigning>(
	my_account_id: AccountId,
	key_store: KeyStore<C>,
	incoming_p2p_message_receiver: MultisigMessageReceiver<<C as ChainSigning>::ChainCrypto>,
	outgoing_p2p_message_sender: MultisigMessageSender<<C as ChainSigning>::ChainCrypto>,
	latest_ceremony_id: CeremonyId,
) -> (MultisigClient<C, KeyStore<C>>, impl futures::Future<Output = Result<()>> + Send) {
	info!("Starting {} MultisigClient", C::NAME);

	let (ceremony_request_sender, ceremony_request_receiver) =
		tokio::sync::mpsc::unbounded_channel();

	let multisig_client =
		MultisigClient::new(my_account_id.clone(), key_store, ceremony_request_sender);

	let multisig_client_backend_future = {
		use multisig::client::ceremony_manager::CeremonyManager;

		let ceremony_manager = CeremonyManager::<C>::new(
			my_account_id,
			outgoing_p2p_message_sender.0,
			latest_ceremony_id,
		);

		ceremony_manager
			.run(ceremony_request_receiver, incoming_p2p_message_receiver.0)
			.instrument(info_span!("MultisigClient", chain = C::NAME))
	};

	(multisig_client, multisig_client_backend_future)
}
