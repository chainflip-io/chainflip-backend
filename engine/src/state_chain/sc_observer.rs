use crate::common::Mutex;
use pallet_cf_vaults::{
    rotation::{ChainParams, VaultRotationResponse},
    KeygenResponse, ThresholdSignatureResponse,
};
use slog::o;
use sp_runtime::AccountId32;
use std::{convert::TryInto, sync::Arc};
use substrate_subxt::{Client, EventSubscription, PairSigner};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::{
    eth::EthBroadcaster,
    logging::COMPONENT_KEY,
    p2p, settings,
    signing::{
        KeyId, KeygenInfo, KeygenOutcome, MessageHash, MultisigEvent, MultisigInstruction,
        SigningInfo, SigningOutcome,
    },
    state_chain::{
        pallets::vaults::VaultsEvent::{
            KeygenRequestEvent, ThresholdSignatureRequestEvent, VaultRotationRequestEvent,
        },
        pallets::witness_api::{
            WitnessKeygenResponseCallExt, WitnessThresholdSignatureResponseCallExt,
            WitnessVaultRotationResponseCallExt,
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
    mut multisig_event_receiver: UnboundedReceiver<MultisigEvent>,
    logger: &slog::Logger,
) {
    let logger = logger.new(o!(COMPONENT_KEY => "SCObserver"));

    let mut sub = EventSubscription::new(
        subxt_client
            .subscribe_finalized_events()
            .await
            .expect("Could not subscribe to state chain events"),
        subxt_client.events_decoder(),
    );
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

        match raw_event_to_sc_event(&raw_event)
            .expect("Could not convert substrate event to SCEvent")
        {
            Some(sc_event) => {
                match sc_event {
                    VaultsEvent(event) => match event {
                        KeygenRequestEvent(keygen_request_event) => {
                            let signers: Vec<_> = keygen_request_event
                                .keygen_request
                                .validator_candidates
                                .iter()
                                .map(|v| p2p::AccountId(v.clone().into()))
                                .collect();

                            let gen_new_key_event = MultisigInstruction::KeyGen(KeygenInfo::new(
                                keygen_request_event.ceremony_id,
                                signers,
                            ));

                            multisig_instruction_sender
                                .send(gen_new_key_event)
                                .map_err(|_| "Receiver should exist")
                                .unwrap();

                            let response = match multisig_event_receiver.recv().await {
                                Some(event) => match event {
                                    MultisigEvent::KeygenResult(KeygenOutcome {
                                        id: _,
                                        result,
                                    }) => match result {
                                        Ok(pubkey) => {
                                            KeygenResponse::<AccountId32, Vec<u8>>::Success(
                                                pubkey.serialize().into(),
                                            )
                                        }
                                        Err((err, bad_account_ids)) => {
                                            slog::error!(
                                                logger,
                                                "Keygen failed with error: {:?}",
                                                err
                                            );
                                            let bad_account_ids: Vec<_> = bad_account_ids
                                                .iter()
                                                .map(|v| AccountId32::from(v.0))
                                                .collect();
                                            KeygenResponse::Error(bad_account_ids)
                                        }
                                    },
                                    MultisigEvent::MessageSigningResult(message_signing_result) => {
                                        panic!(
                                            "Expecting KeygenResult, got: {:?}",
                                            message_signing_result
                                        );
                                    }
                                },
                                None => todo!(),
                            };
                            slog::trace!(
                                logger,
                                "Sending new key back to the State Chain {:?}",
                                response
                            );
                            let mut signer = signer.lock().await;
                            match subxt_client
                                .witness_keygen_response(
                                    &*signer,
                                    keygen_request_event.ceremony_id,
                                    response,
                                )
                                .await
                            {
                                Ok(_) => signer.increment_nonce(),
                                Err(e) => {
                                    slog::error!(
                                        logger,
                                        "Could not submit witness_keygen_response for ceremony_id {}: {}", keygen_request_event.ceremony_id, e
                                    )
                                }
                            }
                        }
                        ThresholdSignatureRequestEvent(event) => {
                            let req = event.threshold_signature_request;

                            let signers: Vec<_> = req
                                .validators
                                .iter()
                                .map(|v| p2p::AccountId(v.clone().into()))
                                .collect();

                            let message_hash: [u8; 32] =
                                req.payload.try_into().expect("Should be a 32 byte hash");
                            let sign_tx = MultisigInstruction::Sign(SigningInfo::new(
                                event.ceremony_id,
                                KeyId(req.public_key),
                                MessageHash(message_hash),
                                signers,
                            ));

                            // The below will be replaced with one shot channels
                            multisig_instruction_sender
                                .send(sign_tx)
                                .map_err(|_| "Receiver should exist")
                                .unwrap();

                            let response = match multisig_event_receiver.recv().await {
                                Some(event) => match event {
                                    MultisigEvent::MessageSigningResult(SigningOutcome {
                                        id: _,
                                        result,
                                    }) => match result {
                                        Ok(sig) => ThresholdSignatureResponse::<
                                            AccountId32,
                                            pallet_cf_vaults::SchnorrSigTruncPubkey,
                                        >::Success {
                                            message_hash,
                                            signature: sig.into(),
                                        },
                                        Err((err, bad_account_ids)) => {
                                            slog::error!(
                                                logger,
                                                "Signing failed with error: {:?}",
                                                err
                                            );
                                            let bad_account_ids: Vec<_> = bad_account_ids
                                                .iter()
                                                .map(|v| AccountId32::from(v.0))
                                                .collect();
                                            ThresholdSignatureResponse::Error(bad_account_ids)
                                        }
                                    },
                                    MultisigEvent::KeygenResult(keygen_result) => {
                                        panic!(
                                            "Expecting MessageSigningResult, got: {:?}",
                                            keygen_result
                                        );
                                    }
                                },
                                _ => panic!("Channel closed"),
                            };
                            let mut signer = signer.lock().await;
                            match subxt_client
                                .witness_threshold_signature_response(
                                    &*signer,
                                    event.ceremony_id,
                                    response,
                                )
                                .await
                            {
                                Ok(_) => signer.increment_nonce(),
                                Err(e) => {
                                    slog::error!(
                                        logger,
                                        "Could not submit witness_threshold_signature_response for ceremony_id {}: {}", event.ceremony_id, e
                                    )
                                }
                            }
                        }
                        VaultRotationRequestEvent(vault_rotation_request_event) => {
                            match vault_rotation_request_event.vault_rotation_request.chain {
                                ChainParams::Ethereum(tx) => {
                                    slog::debug!(
                                        logger,
                                        "Sending ETH vault rotation tx for ceremony {}: {:?}",
                                        vault_rotation_request_event.ceremony_id,
                                        tx
                                    );
                                    // TODO: Contract address should come from the state chain
                                    // https://github.com/chainflip-io/chainflip-backend/issues/459
                                    let response = match eth_broadcaster
                                        .send(tx, settings.eth.key_manager_eth_address)
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
                                            VaultRotationResponse::Error
                                        }
                                    };
                                    let mut signer = signer.lock().await;
                                    match subxt_client
                                        .witness_vault_rotation_response(
                                            &*signer,
                                            vault_rotation_request_event.ceremony_id,
                                            response,
                                        )
                                        .await
                                    {
                                        Ok(_) => signer.increment_nonce(),
                                        Err(e) => {
                                            slog::error!(
                                                logger,
                                                "Could not submit witness_vault_rotation_response for ceremony_id {}: {}", vault_rotation_request_event.ceremony_id, e
                                            )
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
                        slog::trace!(logger, "Ignoring event: {:?}", raw_event);
                    }
                }
            }
            None => {
                slog::trace!(logger, "No action for raw event: {:?}", raw_event);
                continue;
            }
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
    #[ignore = "Start the state chain and then run this to see what types are missing from the register_type_sizes in runtime.rs"]
    async fn test_types() {
        let settings = settings::test_utils::new_test_settings().unwrap();
        let subxt_client = ClientBuilder::<StateChainRuntime>::new()
            .set_url(&settings.state_chain.ws_endpoint)
            .build()
            .await
            .expect("Should create subxt client");
        EventSubscription::new(
            subxt_client
                .subscribe_finalized_events()
                .await
                .expect("Could not subscribe to state chain events"),
            subxt_client.events_decoder(),
        );
    }

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
        let (_multisig_event_sender, multisig_event_receiver) =
            tokio::sync::mpsc::unbounded_channel::<MultisigEvent>();

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
            multisig_event_receiver,
            &logger,
        )
        .await;
    }
}
