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
            VaultsEvent::{
                KeygenRequestEvent, ThresholdSignatureRequestEvent, VaultRotationRequestEvent,
            },
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

                        let gen_new_key_event = MultisigInstruction::KeyGen(KeygenInfo::new(
                            KeyId(keygen_request_event.ceremony_id),
                            validators,
                        ));

                        multisig_instruction_sender
                            .send(gen_new_key_event)
                            .map_err(|_| "Receiver should exist")
                            .unwrap();
                    }
                    // TODO: Provide the pubkey of the key we want to sign with to the signing module
                    // from this event
                    // https://github.com/chainflip-io/chainflip-backend/issues/492
                    ThresholdSignatureRequestEvent(threshold_sig_requst) => {
                        let validators: Vec<_> = threshold_sig_requst
                            .threshold_signature_request
                            .validators
                            .iter()
                            .map(|v| p2p::ValidatorId(v.clone().into()))
                            .collect();

                        let sign_tx = MultisigInstruction::Sign(
                            // TODO: The hashing of the payload should be done on the SC
                            // https://github.com/chainflip-io/chainflip-backend/issues/446
                            MessageHash(
                                Keccak256::hash(
                                    &threshold_sig_requst.threshold_signature_request.payload[..],
                                )
                                .0,
                            ),
                            SigningInfo::new(KeyId(0), validators),
                        );

                        multisig_instruction_sender
                            .send(sign_tx)
                            .map_err(|_| "Receiver should exist")
                            .unwrap();

                        todo!("Receive signing result and send result back to State chain");
                    }
                    VaultRotationRequestEvent(vault_rotation_request_event) => {
                        match vault_rotation_request_event.vault_rotation_request.chain {
                            ChainParams::Ethereum(tx) => {
                                slog::debug!(logger, "Broadcasting to ETH: {:?}", tx);
                                let signer = signer.lock().unwrap();
                                // TODO: Contract address should come from the state chain
                                // https://github.com/chainflip-io/chainflip-backend/issues/459
                                let response = match eth_broadcaster
                                    .sign_and_broadcast_to(tx, settings.eth.key_manager_eth_address)
                                    .await
                                {
                                    Ok(tx_hash) => {
                                        slog::debug!(
                                            logger,
                                            "Broadcast set_agg_key_with_agg_key tx, tx_hash: {}",
                                            tx_hash
                                        );
                                        VaultRotationResponse::Success {
                                            tx_hash: tx_hash.as_bytes().to_vec(),
                                        }
                                    }
                                    Err(e) => {
                                        slog::error!(
                                            logger,
                                            "Failed to broadcast set_agg_key_with_agg_key tx: {}",
                                            e
                                        );
                                        VaultRotationResponse::Failure
                                    }
                                };
                                subxt_client
                                    .vault_rotation_response(
                                        &*signer,
                                        vault_rotation_request_event.ceremony_id,
                                        response,
                                    )
                                    .await
                                    .unwrap(); // TODO: Handle error
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

    use crate::{eth, logging, settings};
    use sp_keyring::AccountKeyring;

    use super::*;

    #[tokio::test]
    #[ignore = "runs forever, useful for testing without having to start the whole CFE"]
    async fn run_the_sc_observer() {
        let settings = settings::test_utils::new_test_settings().unwrap();
        let logger = logging::test_utils::create_test_logger();
        let alice = AccountKeyring::Alice.pair();
        let pair_signer = PairSigner::new(alice);
        let signer = Arc::new(Mutex::new(pair_signer));
        let (multisig_instruction_sender, _multisig_instruction_receiver) =
            tokio::sync::mpsc::unbounded_channel::<MultisigInstruction>();

        let web3 = eth::new_synced_web3_client(&settings, &logger)
            .await
            .unwrap();
        let eth_broadcaster = EthBroadcaster::new(&settings, web3.clone()).unwrap();

        start(
            &settings,
            ClientBuilder::<StateChainRuntime>::new()
                .set_url(&settings.state_chain.ws_endpoint)
                .build()
                .await
                .expect("Should create subxt client"),
            signer,
            eth_broadcaster,
            multisig_instruction_sender,
            &logger,
        )
        .await;
    }
}
