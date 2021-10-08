use futures::{Stream, StreamExt};
use pallet_cf_vaults::{
    rotation::{ChainParams, VaultRotationResponse},
    KeygenResponse, ThresholdSignatureResponse,
};
use slog::o;
use sp_runtime::AccountId32;
use std::{convert::TryInto, sync::Arc};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::{
    eth::EthBroadcaster,
    logging::COMPONENT_KEY,
    p2p, settings,
    signing::{
        KeyId, KeygenInfo, KeygenOutcome, MessageHash, MultisigEvent, MultisigInstruction,
        SigningInfo, SigningOutcome,
    },
};

pub async fn start<BlockStream>(
    settings: &settings::Settings,
    state_chain_client: Arc<super::client::StateChainClient>,
    sc_block_stream: BlockStream,
    eth_broadcaster: EthBroadcaster,
    multisig_instruction_sender: UnboundedSender<MultisigInstruction>,
    mut multisig_event_receiver: UnboundedReceiver<MultisigEvent>,
    logger: &slog::Logger,
) where
    BlockStream: Stream<Item = anyhow::Result<state_chain_runtime::Header>>,
{
    let logger = logger.new(o!(COMPONENT_KEY => "SCObserver"));

    let heartbeat_block_interval = state_chain_client
        .metadata
        .module("Reputation")
        .expect("No module 'Reputation' in chain metadata")
        .constant("HeartbeatBlockInterval")
        .expect("No constant 'HeartbeatBlockInterval' in chain metadata for module 'Reputation'")
        .value::<u32>()
        .expect("Could not decode HeartbeatBlockInterval to u32");
    slog::info!(
        logger,
        "Sending heartbeat every {} blocks",
        heartbeat_block_interval,
    );

    state_chain_client
        .submit_extrinsic(&logger, pallet_cf_reputation::Call::heartbeat())
        .await;

    let mut sc_block_stream = Box::pin(sc_block_stream);
    while let Some(result_block_header) = sc_block_stream.next().await {
        if let Ok(block_header) = result_block_header {
            // Target the middle of the heartbeat block interval so block drift is *very* unlikely to cause failure
            if (block_header.number + (heartbeat_block_interval / 2)) % heartbeat_block_interval
                == 0
            {
                slog::info!(
                    logger,
                    "Sending heartbeat at block: {}",
                    block_header.number
                );
                state_chain_client
                    .submit_extrinsic(&logger, pallet_cf_reputation::Call::heartbeat())
                    .await;
            }

            // Process this block's events
            match state_chain_client.events(&block_header).await {
                Ok(events) => {
                    for (_phase, event, _topics) in events {
                        match event {
                            state_chain_runtime::Event::pallet_cf_vaults(
                                pallet_cf_vaults::Event::KeygenRequest(ceremony_id, keygen_request),
                            ) => {
                                let signers: Vec<_> = keygen_request
                                    .validator_candidates
                                    .iter()
                                    .map(|v| p2p::AccountId(v.clone().into()))
                                    .collect();

                                let gen_new_key_event = MultisigInstruction::KeyGen(
                                    KeygenInfo::new(ceremony_id, signers),
                                );

                                multisig_instruction_sender
                                    .send(gen_new_key_event)
                                    .map_err(|_| "Receiver should exist")
                                    .unwrap();

                                let response = match multisig_event_receiver
                                    .recv()
                                    .await
                                    .expect("Channel closed!")
                                {
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
                                };
                                state_chain_client
                                    .submit_extrinsic(
                                        &logger,
                                        pallet_cf_witnesser_api::Call::witness_keygen_response(
                                            ceremony_id,
                                            response,
                                        ),
                                    )
                                    .await;
                            }
                            state_chain_runtime::Event::pallet_cf_vaults(
                                pallet_cf_vaults::Event::ThresholdSignatureRequest(
                                    ceremony_id,
                                    threshold_signature_request,
                                ),
                            ) => {
                                let signers: Vec<_> = threshold_signature_request
                                    .validators
                                    .iter()
                                    .map(|v| p2p::AccountId(v.clone().into()))
                                    .collect();

                                let message_hash: [u8; 32] = threshold_signature_request
                                    .payload
                                    .try_into()
                                    .expect("Should be a 32 byte hash");
                                let sign_tx = MultisigInstruction::Sign(SigningInfo::new(
                                    ceremony_id,
                                    KeyId(threshold_signature_request.public_key),
                                    MessageHash(message_hash),
                                    signers,
                                ));

                                // The below will be replaced with one shot channels
                                multisig_instruction_sender
                                    .send(sign_tx)
                                    .map_err(|_| "Receiver should exist")
                                    .unwrap();

                                let response = match multisig_event_receiver
                                    .recv()
                                    .await
                                    .expect("Channel closed!")
                                {
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
                                };
                                state_chain_client
                                    .submit_extrinsic(
                                        &logger,
                                        pallet_cf_witnesser_api::Call::witness_threshold_signature_response(
                                            ceremony_id,
                                            response,
                                        ),
                                    )
                                    .await;
                            }
                            state_chain_runtime::Event::pallet_cf_vaults(
                                pallet_cf_vaults::Event::VaultRotationRequest(
                                    ceremony_id,
                                    vault_rotation_request,
                                ),
                            ) => {
                                match vault_rotation_request.chain {
                                    ChainParams::Ethereum(tx) => {
                                        slog::debug!(
                                            logger,
                                            "Sending ETH vault rotation tx for ceremony {}: {:?}",
                                            ceremony_id,
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
                                        state_chain_client.submit_extrinsic(
                                            &logger,
                                            pallet_cf_witnesser_api::Call::witness_vault_rotation_response(
                                                ceremony_id,
                                                response,
                                            ),
                                        ).await;
                                    }
                                }
                            }
                            ignored_event => {
                                // ignore events we don't care about
                                slog::trace!(logger, "Ignoring event: {:?}", ignored_event);
                            }
                        }
                    }
                }
                Err(error) => {
                    slog::error!(
                        logger,
                        "Failed to decode events at block {}. {}",
                        block_header.number,
                        error,
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{eth, logging, settings};

    use super::*;

    #[tokio::test]
    #[ignore = "runs forever, useful for testing without having to start the whole CFE"]
    async fn run_the_sc_observer() {
        let settings = settings::test_utils::new_test_settings().unwrap();
        let logger = logging::test_utils::create_test_logger();

        let (state_chain_client, block_stream) =
            crate::state_chain::client::connect_to_state_chain(&settings)
                .await
                .unwrap();

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
            state_chain_client,
            block_stream,
            eth_broadcaster,
            multisig_instruction_sender,
            multisig_event_receiver,
            &logger,
        )
        .await;
    }
}
