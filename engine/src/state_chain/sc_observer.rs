use std::sync::{Arc, Mutex};

use pallet_cf_vaults::rotation::{ChainParams, VaultRotationResponse};
use slog::o;
use sp_core::Hasher;
use sp_runtime::traits::Keccak256;
use substrate_subxt::{Client, EventSubscription, PairSigner};
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    eth::EthBroadcaster,
    logging::COMPONENT_KEY,
    p2p, settings,
    signing::{KeyId, KeygenInfo, MessageHash, MultisigInstruction, SigningInfo},
    state_chain::{
        pallets::vaults::{
            VaultRotationResponseCallExt,
            VaultsEvent::{EthSignTxRequestEvent, KeygenRequestEvent, VaultRotationRequestEvent},
        },
        sc_event::SCEvent::VaultsEvent,
    },
};

use super::{runtime::StateChainRuntime, sc_event::raw_event_to_sc_event};

pub async fn start(
    settings: &settings::Settings,
    subxt_client: Client<StateChainRuntime>,
    signer: Arc<Mutex<PairSigner<StateChainRuntime, sp_core::sr25519::Pair>>>,
    eth_broadcaster: EthBroadcaster,
    multisig_instruction_sender: UnboundedSender<MultisigInstruction>,
    logger: &slog::Logger,
) {
    let logger = logger.new(o!(COMPONENT_KEY => "SCObserver"));

    let sub = subxt_client
        .subscribe_finalized_events()
        .await
        .expect("Could not subscribe to state chain events");
    let decoder = subxt_client.events_decoder();
    let mut sub = EventSubscription::new(sub, decoder);
    while let Some(res_event) = sub.next().await {
        let raw_event = match res_event {
            Ok(raw_event) => raw_event,
            Err(e) => {
                slog::error!(
                    logger,
                    "Next event could not be read from subxt subscription: {}",
                    e
                );
                continue;
            }
        };

        if let Some(sc_event) =
            raw_event_to_sc_event(&raw_event).expect("Could not convert substrate event to SCEvent")
        {
            match sc_event {
                VaultsEvent(event) => match event {
                    KeygenRequestEvent(keygen_request_event) => {
                        let validators: Vec<_> = keygen_request_event
                            .keygen_request
                            .validator_candidates
                            .iter()
                            .map(|v| p2p::ValidatorId(v.clone().into()))
                            .collect();
                        // TODO: Should this be request index? @andy
                        let key_gen_info =
                            KeygenInfo::new(KeyId(keygen_request_event.request_index), validators);
                        let gen_new_key_event = MultisigInstruction::KeyGen(key_gen_info);
                        // We need the Sender for Multisig instructions channel here
                        multisig_instruction_sender
                            .send(gen_new_key_event)
                            .map_err(|_| "Receiver should exist")
                            .unwrap();
                    }
                    EthSignTxRequestEvent(eth_sign_tx_request) => {
                        let validators: Vec<_> = eth_sign_tx_request
                            .eth_signing_tx_request
                            .validators
                            .iter()
                            .map(|v| p2p::ValidatorId(v.clone().into()))
                            .collect();

                        let sign_tx = MultisigInstruction::Sign(
                            // TODO: Should this hash be on the state chain or the signing module?
                            // https://github.com/chainflip-io/chainflip-backend/issues/446
                            MessageHash(
                                Keccak256::hash(
                                    &eth_sign_tx_request.eth_signing_tx_request.payload[..],
                                )
                                .0,
                            ),
                            // TODO: we want to use some notion of "KeyId"
                            // https://github.com/chainflip-io/chainflip-backend/issues/442
                            SigningInfo::new(KeyId(eth_sign_tx_request.request_index), validators),
                        );

                        multisig_instruction_sender
                            .send(sign_tx)
                            .map_err(|_| "Receiver should exist")
                            .unwrap();
                    }
                    VaultRotationRequestEvent(vault_rotation_request_event) => {
                        match vault_rotation_request_event.vault_rotation_request.chain {
                            ChainParams::Ethereum(tx) => {
                                slog::debug!(logger, "Broadcasting to ETH: {:?}", tx);
                                let signer = signer.lock().unwrap();
                                // TODO: Contract address should come from the state chain

                                match eth_broadcaster
                                    .sign_and_broadcast_to(
                                        tx.clone(),
                                        settings.eth.key_manager_eth_address,
                                    )
                                    .await
                                {
                                    Ok(tx_hash) => {
                                        slog::debug!(
                                            logger,
                                            "Broadcast set_agg_key_with_agg_key tx, tx_hash: {}",
                                            tx_hash
                                        );
                                        subxt_client
                                            .vault_rotation_response(
                                                &*signer,
                                                vault_rotation_request_event.request_index,
                                                VaultRotationResponse::Success {
                                                    // TODO: Add the actual keys here
                                                    // why are these being added here? The SC should know these already? right?
                                                    old_key: Vec::default(),
                                                    new_key: Vec::default(),
                                                    tx,
                                                },
                                            )
                                            .await
                                            .unwrap(); // TODO: Handle error
                                    }
                                    Err(e) => {
                                        slog::error!(
                                            logger,
                                            "Failed to broadcast set_agg_key_with_agg_key tx: {}",
                                            e
                                        );
                                        subxt_client
                                            .vault_rotation_response(
                                                &*signer,
                                                vault_rotation_request_event.request_index,
                                                VaultRotationResponse::Failure,
                                            )
                                            .await
                                            .unwrap(); // TODO: Handle error
                                    }
                                }
                            }
                            // Leave this to be explicit about future chains being added
                            ChainParams::Other(_) => panic!("Chain::Other does not exist"),
                        }
                    }
                },
                _ => {
                    // ignore events we don't care about
                }
            }
        } else {
            slog::trace!(logger, "No action for raw event: {:?}", raw_event);
            continue;
        }
    }
}

#[cfg(test)]
mod tests {
    use substrate_subxt::ClientBuilder;

    use crate::{logging, settings};
    use sp_keyring::AccountKeyring;

    use super::*;

    #[tokio::test]
    #[ignore = "runs forever, useful for testing without having to start the whole CFE"]
    async fn run_the_sc_observer() {
        let settings = settings::test_utils::new_test_settings().unwrap();
        let alice = AccountKeyring::Alice.pair();
        let pair_signer = PairSigner::new(alice);
        let signer = Arc::new(Mutex::new(pair_signer));

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        start(
            ClientBuilder::<StateChainRuntime>::new()
                .set_url(&settings.state_chain.ws_endpoint)
                .build()
                .await
                .expect("Should create subxt client"),
            &signer,
            tx,
            &logging::test_utils::create_test_logger(),
        )
        .await;
    }
}
