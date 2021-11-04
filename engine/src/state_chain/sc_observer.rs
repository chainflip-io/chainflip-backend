use cf_chains::ChainId;
use futures::{Stream, StreamExt};
use pallet_cf_broadcast::TransmissionFailure;
use slog::o;
use sp_runtime::AccountId32;
use std::sync::Arc;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::sync::RwLock;

use crate::{
    duty_manager::DutyManager,
    eth::EthBroadcaster,
    logging::COMPONENT_KEY,
    multisig::{
        KeyId, KeygenInfo, KeygenOutcome, MessageHash, MultisigEvent, MultisigInstruction,
        SigningInfo, SigningOutcome,
    },
    p2p,
    state_chain::client::StateChainRpcApi,
};

pub async fn start<BlockStream, RpcClient>(
    state_chain_client: Arc<super::client::StateChainClient<RpcClient>>,
    sc_block_stream: BlockStream,
    eth_broadcaster: EthBroadcaster,
    multisig_instruction_sender: UnboundedSender<MultisigInstruction>,
    mut multisig_event_receiver: UnboundedReceiver<MultisigEvent>,
    logger: &slog::Logger,
    duty_manager: Arc<RwLock<DutyManager>>,
) where
    BlockStream: Stream<Item = anyhow::Result<state_chain_runtime::Header>>,
    RpcClient: StateChainRpcApi,
{
    let logger = logger.new(o!(COMPONENT_KEY => "SCObserver"));

    let heartbeat_block_interval = state_chain_client.get_heartbeat_block_interval();

    slog::info!(
        logger,
        "Sending heartbeat every {} blocks",
        heartbeat_block_interval,
    );

    let mut sc_block_stream = Box::pin(sc_block_stream);
    while let Some(result_block_header) = sc_block_stream.next().await {
        match result_block_header {
            Ok(block_header) => {
                // Target the middle of the heartbeat block interval so block drift is *very* unlikely to cause failure
                if (block_header.number + (heartbeat_block_interval / 2)) % heartbeat_block_interval
                    == 0
                {
                    if duty_manager.read().await.is_heartbeat_enabled() {
                        slog::info!(
                            logger,
                            "Sending heartbeat at block: {}",
                            block_header.number
                        );
                        let _ = state_chain_client
                            .submit_extrinsic(&logger, pallet_cf_online::Call::heartbeat())
                            .await;
                    }
                }

                // Process this block's events
                match state_chain_client.get_events(&block_header).await {
                    Ok(events) => {
                        for (_phase, event, _topics) in events {
                            match event {
                                state_chain_runtime::Event::pallet_cf_vaults(
                                    pallet_cf_vaults::Event::KeygenRequest(
                                        ceremony_id,
                                        chain_id,
                                        validator_candidates
                                    ),
                                ) => {
                                    let signers: Vec<_> = validator_candidates
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

                                    let response_extrinsic = match multisig_event_receiver
                                        .recv()
                                        .await
                                        .expect("Channel closed!")
                                    {
                                        MultisigEvent::KeygenResult(KeygenOutcome {
                                            id: _,
                                            result,
                                        }) => match result {
                                            Ok(pubkey) => {
                                                pallet_cf_witnesser_api::Call::witness_keygen_success(
                                                    ceremony_id,
                                                    chain_id,
                                                    pubkey.serialize().to_vec(),
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
                                                pallet_cf_witnesser_api::Call::witness_keygen_failure(
                                                    ceremony_id,
                                                    chain_id,
                                                    bad_account_ids,
                                                )
                                            }
                                        },
                                        MultisigEvent::MessageSigningResult(
                                            message_signing_result,
                                        ) => {
                                            panic!(
                                                "Expecting KeygenResult, got: {:?}",
                                                message_signing_result
                                            );
                                        }
                                    };
                                    let _ = state_chain_client
                                        .submit_extrinsic(
                                            &logger,
                                            response_extrinsic,
                                        )
                                        .await;
                                }
                                state_chain_runtime::Event::pallet_cf_threshold_signature_Instance0(
                                    pallet_cf_threshold_signature::Event::ThresholdSignatureRequest(
                                        ceremony_id,
                                        key_id,
                                        validators,
                                        payload,
                                    ),
                                ) => {
                                    let signers: Vec<_> = validators
                                        .iter()
                                        .map(|v| p2p::AccountId(v.clone().into()))
                                        .collect();

                                    let sign_tx = MultisigInstruction::Sign(SigningInfo::new(
                                        ceremony_id,
                                        KeyId(key_id),
                                        MessageHash(payload.to_fixed_bytes()),
                                        signers,
                                    ));

                                    // The below will be replaced with one shot channels
                                    multisig_instruction_sender
                                        .send(sign_tx)
                                        .map_err(|_| "Receiver should exist")
                                        .unwrap();

                                    let response_extrinsic = match multisig_event_receiver
                                        .recv()
                                        .await
                                        .expect("Channel closed!")
                                    {
                                        MultisigEvent::MessageSigningResult(SigningOutcome {
                                            id: _,
                                            result,
                                        }) => match result {
                                            Ok(sig) => pallet_cf_witnesser_api::Call::witness_eth_signature_success(
                                                ceremony_id, sig.into()
                                            ),
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
                                                pallet_cf_witnesser_api::Call::witness_eth_signature_failed(
                                                    ceremony_id, bad_account_ids
                                                )
                                            }
                                        },
                                        MultisigEvent::KeygenResult(keygen_result) => {
                                            panic!(
                                                "Expecting MessageSigningResult, got: {:?}",
                                                keygen_result
                                            );
                                        }
                                    };
                                    let _ = state_chain_client
                                        .submit_extrinsic(
                                            &logger,
                                            response_extrinsic,
                                        )
                                        .await;
                                }
                                state_chain_runtime::Event::pallet_cf_broadcast_Instance0(
                                    pallet_cf_broadcast::Event::TransactionSigningRequest(
                                        attempt_id,
                                        validator_id,
                                        unsigned_tx
                                    ),
                                ) if validator_id == state_chain_client.our_account_id => {
                                    slog::debug!(
                                        logger,
                                        "Received signing request {} for transaction: {:?}",
                                        attempt_id,
                                        unsigned_tx,
                                    );
                                    match eth_broadcaster.encode_and_sign_tx(unsigned_tx).await {
                                        Ok(raw_signed_tx) => {
                                            let _ = state_chain_client.submit_extrinsic(
                                                &logger,
                                                state_chain_runtime::Call::EthereumBroadcaster(
                                                    pallet_cf_broadcast::Call::transaction_ready_for_transmission(
                                                        attempt_id,
                                                        raw_signed_tx.0,
                                                    ),
                                                )
                                            ).await;
                                        },
                                        Err(e) => {
                                            // Note: this error case should only occur if there is a problem with the
                                            // local ethereum node, which would mean the web3 lib is unable to fill in 
                                            // the tranaction params, mainly the gas limit.
                                            // In the long run all transaction parameters will be provided by the state
                                            // chain and the above eth_broadcaster.sign_tx method can be made 
                                            // infallible.
                                            slog::error!(
                                                logger,
                                                "Transaction signing attempt {} failed: {:?}",
                                                attempt_id,
                                                e
                                            );
                                        },
                                    }
                                }
                                state_chain_runtime::Event::pallet_cf_broadcast_Instance0(
                                    pallet_cf_broadcast::Event::TransmissionRequest(
                                        attempt_id,
                                        signed_tx,
                                    ),
                                ) => {
                                    slog::debug!(
                                        logger,
                                        "Sending signed tx for broadcast attempt {}: {:?}",
                                        attempt_id,
                                        hex::encode(&signed_tx),
                                    );
                                    let response_extrinsics = match eth_broadcaster.send(signed_tx).await
                                    {
                                        Ok(tx_hash) => {
                                            slog::debug!(
                                                logger,
                                                "Successful broadcast attempt {}, tx_hash: {}",
                                                attempt_id,
                                                tx_hash
                                            );
                                            [
                                                pallet_cf_witnesser_api::Call::witness_eth_transmission_success(
                                                    attempt_id, tx_hash.into()
                                                ),
                                                // TODO: This should be triggered from the eth event.
                                                // See https://github.com/chainflip-io/chainflip-backend/issues/586
                                                pallet_cf_witnesser_api::Call::witness_vault_key_rotated(
                                                    ChainId::Ethereum, vec![], 0, tx_hash.as_bytes().to_vec()
                                                ),
                                            ].to_vec()
                                        }
                                        Err(e) => {
                                            slog::error!(
                                                logger,
                                                "Broadcast attempt {} failed: {:?}",
                                                attempt_id,
                                                e
                                            );
                                            [
                                                // TODO: Fill in the transaction hash with the real one
                                                // See https://github.com/chainflip-io/chainflip-backend/issues/586
                                                pallet_cf_witnesser_api::Call::witness_eth_transmission_failure(
                                                    attempt_id, TransmissionFailure::TransactionFailed, [0u8; 32]
                                                ),
                                            ].to_vec()
                                        }
                                    };
                                    for ext in response_extrinsics {
                                        let _ = state_chain_client.submit_extrinsic(
                                            &logger,
                                            ext,
                                        ).await;
                                    }
                                }
                                state_chain_runtime::Event::pallet_cf_validator(
                                    pallet_cf_validator::Event::NewEpoch(epoch_index)) => {
                                        let mut duty_manager = duty_manager.write().await;
                                        duty_manager.set_current_epoch(epoch_index);

                                        // Are we still an active validator here?
                                        // do we need to check this


                                    }
                                ignored_event => {
                                    // ignore events we don't care about
                                    slog::trace!(logger, "Ignoring event at block {}: {:?}", block_header.number, ignored_event);
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
            Err(error) => {
                slog::error!(logger, "Failed to decode block header: {}", error,);
            }
        }
    }
}

// #[cfg(test)]
// mod tests {
//     use crate::{eth, logging, settings};

//     use super::*;

//     #[tokio::test]
//     #[ignore = "runs forever, useful for testing without having to start the whole CFE"]
//     async fn run_the_sc_observer() {
//         let settings = settings::test_utils::new_test_settings().unwrap();
//         let logger = logging::test_utils::new_test_logger();

// let (state_chain_client, block_stream) =
//     crate::state_chain::client::connect_to_state_chain(&settings.state_chain)
//         .await
//         .unwrap();

//         let (multisig_instruction_sender, _multisig_instruction_receiver) =
//             tokio::sync::mpsc::unbounded_channel::<MultisigInstruction>();
//         let (_multisig_event_sender, multisig_event_receiver) =
//             tokio::sync::mpsc::unbounded_channel::<MultisigEvent>();

//         let web3 = eth::new_synced_web3_client(&settings, &logger)
//             .await
//             .unwrap();
//         let eth_broadcaster = EthBroadcaster::new(&settings, web3.clone()).unwrap();

//         start(
//             state_chain_client,
//             block_stream,
//             eth_broadcaster,
//             multisig_instruction_sender,
//             multisig_event_receiver,
//             &logger,
//             Arc::new(RwLock::new(DutyManager::new_test())),
//         )
//         .await;
//     }
// }
