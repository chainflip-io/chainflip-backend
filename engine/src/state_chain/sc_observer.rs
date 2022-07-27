use cf_traits::EpochIndex;
use futures::{stream, Stream, StreamExt};
use pallet_cf_validator::CeremonyId;
use pallet_cf_vaults::KeygenError;
use slog::o;
use sp_core::H256;
use sp_runtime::AccountId32;
use state_chain_runtime::{AccountId, CfeSettings};
use std::{collections::BTreeSet, sync::Arc};
use tokio::sync::{broadcast, mpsc::UnboundedSender, watch};

use crate::{
    eth::{rpc::EthRpcApi, EthBroadcaster, ObserveInstruction},
    logging::{CEREMONY_ID_KEY, COMPONENT_KEY},
    multisig::{
        client::{CeremonyFailureReason, KeygenFailureReason, MultisigClientApi},
        KeyId, MessageHash,
    },
    multisig_p2p::AccountPeerMappingChange,
    state_chain::client::{StateChainClient, StateChainRpcApi},
};

async fn handle_keygen_request<MultisigClient, RpcClient>(
    multisig_client: Arc<MultisigClient>,
    state_chain_client: Arc<StateChainClient<RpcClient>>,
    ceremony_id: CeremonyId,
    validator_candidates: Vec<AccountId32>,
    logger: slog::Logger,
) where
    MultisigClient: MultisigClientApi<crate::multisig::eth::EthSigning> + Send + Sync + 'static,
    RpcClient: StateChainRpcApi + Send + Sync + 'static,
{
    if validator_candidates.contains(&state_chain_client.our_account_id) {
        // Send a keygen request and wait to submit the result to the SC
        tokio::spawn(async move {
            let keygen_outcome = multisig_client
                .keygen(ceremony_id, validator_candidates.clone())
                .await;

            let keygen_outcome = match keygen_outcome {
                Ok(public_key) => {
                    // Keygen verification: before the new key is returned to the SC,
                    // we first ensure that all parties can use it for signing

                    let public_key_bytes = public_key.get_element().serialize();

                    // We arbitrarily choose the data to sign over to be the hash of the generated pubkey
                    let data_to_sign = sp_core::hashing::blake2_256(&public_key_bytes);
                    match multisig_client
                        .sign(
                            ceremony_id,
                            KeyId(public_key_bytes.to_vec()),
                            validator_candidates,
                            MessageHash(data_to_sign),
                        )
                        .await
                    {
                        // Report keygen success if we are able to sign
                        Ok(signature) => Ok((
                            cf_chains::eth::AggKey::from_pubkey_compressed(public_key_bytes),
                            data_to_sign.into(),
                            signature.into(),
                        )),
                        // Report keygen failure if we failed to sign
                        Err((bad_account_ids, _reason)) => {
                            slog::debug!(logger, "Keygen ceremony verification failed"; CEREMONY_ID_KEY => ceremony_id);
                            Err(KeygenError::Failure(BTreeSet::from_iter(bad_account_ids)))
                        }
                    }
                }
                Err((bad_account_ids, reason)) => Err({
                    if let CeremonyFailureReason::<KeygenFailureReason>::Other(
                        KeygenFailureReason::KeyNotCompatible,
                    ) = reason
                    {
                        KeygenError::Incompatible
                    } else {
                        KeygenError::Failure(BTreeSet::from_iter(bad_account_ids))
                    }
                }),
            };

            let _result = state_chain_client
                .submit_signed_extrinsic(
                    pallet_cf_vaults::Call::report_keygen_outcome {
                        ceremony_id,
                        reported_outcome: keygen_outcome,
                    },
                    &logger,
                )
                .await;
        });
    } else {
        // If we are not participating, just send an empty ceremony request (needed for ceremony id tracking)
        multisig_client.not_participating_ceremony(ceremony_id);
    }
}

async fn handle_signing_request<MultisigClient, RpcClient>(
    multisig_client: Arc<MultisigClient>,
    state_chain_client: Arc<StateChainClient<RpcClient>>,
    ceremony_id: CeremonyId,
    key_id: KeyId,
    signers: Vec<AccountId>,
    data: MessageHash,
    logger: slog::Logger,
) where
    MultisigClient: MultisigClientApi<crate::multisig::eth::EthSigning> + Send + Sync + 'static,
    RpcClient: StateChainRpcApi + Send + Sync + 'static,
{
    if signers.contains(&state_chain_client.our_account_id) {
        // Send a signing request and wait to submit the result to the SC
        tokio::spawn(async move {
            match multisig_client
                .sign(ceremony_id, key_id, signers, data)
                .await
            {
                Ok(signature) => {
                    let _result = state_chain_client
                        .submit_unsigned_extrinsic(
                            pallet_cf_threshold_signature::Call::signature_success {
                                ceremony_id,
                                signature: signature.into(),
                            },
                            &logger,
                        )
                        .await;
                }
                Err((bad_account_ids, _reason)) => {
                    let _result = state_chain_client
                        .submit_signed_extrinsic(
                            pallet_cf_threshold_signature::Call::report_signature_failed {
                                id: ceremony_id,
                                offenders: BTreeSet::from_iter(bad_account_ids),
                            },
                            &logger,
                        )
                        .await;
                }
            }
        });
    } else {
        // If we are not participating, just send an empty ceremony request (needed for ceremony id tracking)
        multisig_client.not_participating_ceremony(ceremony_id);
    }
}

#[cfg(test)]
pub async fn test_handle_keygen_request<MultisigClient, RpcClient>(
    multisig_client: Arc<MultisigClient>,
    state_chain_client: Arc<StateChainClient<RpcClient>>,
    ceremony_id: CeremonyId,
    validator_candidates: Vec<AccountId32>,
    logger: slog::Logger,
) where
    MultisigClient: MultisigClientApi<crate::multisig::eth::EthSigning> + Send + Sync + 'static,
    RpcClient: StateChainRpcApi + Send + Sync + 'static,
{
    handle_keygen_request(
        multisig_client,
        state_chain_client,
        ceremony_id,
        validator_candidates,
        logger,
    )
    .await;
}

#[cfg(test)]
pub async fn test_handle_signing_request<MultisigClient, RpcClient>(
    multisig_client: Arc<MultisigClient>,
    state_chain_client: Arc<StateChainClient<RpcClient>>,
    ceremony_id: CeremonyId,
    key_id: KeyId,
    signers: Vec<AccountId>,
    data: MessageHash,
    logger: slog::Logger,
) where
    MultisigClient: MultisigClientApi<crate::multisig::eth::EthSigning> + Send + Sync + 'static,
    RpcClient: StateChainRpcApi + Send + Sync + 'static,
{
    handle_signing_request(
        multisig_client,
        state_chain_client,
        ceremony_id,
        key_id,
        signers,
        data,
        logger,
    )
    .await;
}

async fn start_epoch_observation<RpcClient>(
    send_instruction: impl FnOnce(ObserveInstruction),
    state_chain_client: &Arc<StateChainClient<RpcClient>>,
    block_hash: H256,
    epoch: EpochIndex,
) where
    RpcClient: StateChainRpcApi + Send + Sync + 'static,
{
    send_instruction(ObserveInstruction::Start(
        state_chain_client
            .get_storage_map::<pallet_cf_vaults::Vaults<
                state_chain_runtime::Runtime,
                state_chain_runtime::EthereumInstance,
            >>(block_hash, &epoch)
            .await
            .unwrap()
            .unwrap()
            .active_from_block,
        epoch,
    ));
}

async fn try_end_previous_epoch_observation<RpcClient>(
    send_instruction: impl FnOnce(ObserveInstruction),
    state_chain_client: &Arc<StateChainClient<RpcClient>>,
    block_hash: H256,
    epoch: EpochIndex,
) -> bool
where
    RpcClient: StateChainRpcApi + Send + Sync + 'static,
{
    if let Some(vault) = state_chain_client
        .get_storage_map::<pallet_cf_vaults::Vaults<
            state_chain_runtime::Runtime,
            state_chain_runtime::EthereumInstance,
        >>(block_hash, &epoch)
        .await
        .unwrap()
    {
        send_instruction(ObserveInstruction::End(vault.active_from_block));
        true
    } else {
        false
    }
}

// Wrap the match so we add a log message before executing the processing of the event
// if we are processing. Else, ignore it.
macro_rules! match_event {
    ($logger:ident, $event:ident { $($bind:pat $(if $condition:expr)? => $block:expr)+ }) => {{
        let formatted_event = format!("{:?}", $event);
        match $event {
            $(
                $bind => {
                    $(if !$condition {
                        slog::trace!(
                            $logger,
                            "Ignoring event {}",
                            formatted_event
                        );
                    } else )? {
                        slog::debug!(
                            $logger,
                            "Handling event {}",
                            formatted_event
                        );
                        $block
                    }
                }
            )+
            _ => () // Don't log events the CFE does not ever process
        }
    }}
}

pub async fn start<BlockStream, RpcClient, EthRpc, MultisigClient>(
    state_chain_client: Arc<StateChainClient<RpcClient>>,
    sc_block_stream: BlockStream,
    eth_broadcaster: EthBroadcaster<EthRpc>,
    multisig_client: Arc<MultisigClient>,
    account_peer_mapping_change_sender: UnboundedSender<(
        AccountId,
        sp_core::ed25519::Public,
        AccountPeerMappingChange,
    )>,

    witnessing_instruction_sender: broadcast::Sender<ObserveInstruction>,
    cfe_settings_update_sender: watch::Sender<CfeSettings>,
    initial_block_hash: H256,
    logger: &slog::Logger,
) where
    BlockStream: Stream<Item = anyhow::Result<state_chain_runtime::Header>>,
    RpcClient: StateChainRpcApi + Send + Sync + 'static,
    EthRpc: EthRpcApi,
    MultisigClient: MultisigClientApi<crate::multisig::eth::EthSigning> + Send + Sync + 'static,
{
    let logger = logger.new(o!(COMPONENT_KEY => "SCObserver"));

    let blocks_per_heartbeat = std::cmp::max(1, state_chain_client.heartbeat_block_interval / 2);

    slog::info!(
        logger,
        "Sending heartbeat every {} blocks",
        blocks_per_heartbeat
    );

    state_chain_client
        .submit_signed_extrinsic(pallet_cf_reputation::Call::heartbeat {}, &logger)
        .await
        .expect("Should be able to submit first heartbeat");

    let send_instruction = |observe_instruction: ObserveInstruction| {
        witnessing_instruction_sender
            .send(observe_instruction)
            .unwrap();
    };

    let historical_active_epochs = state_chain_client.get_storage_map::<pallet_cf_validator::HistoricalActiveEpochs<state_chain_runtime::Runtime>>(
        initial_block_hash,
        &state_chain_client.our_account_id
    ).await.unwrap();

    assert!(historical_active_epochs.iter().is_sorted());

    let mut active_in_current_epoch = stream::iter(historical_active_epochs.into_iter())
        .fold(false, |acc, epoch| {
            let state_chain_client = state_chain_client.clone();
            async move {
                start_epoch_observation(
                    send_instruction,
                    &state_chain_client,
                    initial_block_hash,
                    epoch,
                )
                .await;
                if try_end_previous_epoch_observation(
                    send_instruction,
                    &state_chain_client,
                    initial_block_hash,
                    epoch + 1,
                )
                .await
                {
                    acc
                } else {
                    assert!(!acc);
                    true
                }
            }
        })
        .await;

    let mut sc_block_stream = Box::pin(sc_block_stream);
    loop {
        match sc_block_stream.next().await {
            Some(result_block_header) => {
                match result_block_header {
                    Ok(current_block_header) => {
                        let current_block_hash = current_block_header.hash();
                        slog::debug!(
                            logger,
                            "Processing SC block {} with block hash: {:#x}",
                            current_block_header.number,
                            current_block_hash
                        );

                        match state_chain_client.get_events(current_block_hash).await {
                            Ok(events) => {
                                for (_phase, event, _topics) in events {
                                    match_event! { logger, event {
                                            state_chain_runtime::Event::Validator(
                                                pallet_cf_validator::Event::NewEpoch(new_epoch),
                                            ) => {
                                                if active_in_current_epoch {
                                                    assert!(try_end_previous_epoch_observation(send_instruction, &state_chain_client, current_block_hash, new_epoch).await);
                                                }

                                                active_in_current_epoch = state_chain_client.get_storage_double_map::<pallet_cf_validator::AuthorityIndex<state_chain_runtime::Runtime>>(
                                                    current_block_hash,
                                                    &new_epoch,
                                                    &state_chain_client.our_account_id
                                                ).await.unwrap().is_some();

                                                if active_in_current_epoch {
                                                    start_epoch_observation(send_instruction, &state_chain_client, current_block_hash, new_epoch).await;
                                                }
                                            }
                                            state_chain_runtime::Event::Validator(
                                                pallet_cf_validator::Event::PeerIdRegistered(
                                                    account_id,
                                                    peer_id,
                                                    port,
                                                    ip_address,
                                                ),
                                            ) => {
                                                account_peer_mapping_change_sender
                                                    .send((
                                                        account_id,
                                                        peer_id,
                                                        AccountPeerMappingChange::Registered(
                                                            port,
                                                            ip_address.into(),
                                                        ),
                                                    ))
                                                    .unwrap();
                                            }
                                            state_chain_runtime::Event::Validator(
                                                pallet_cf_validator::Event::PeerIdUnregistered(
                                                    account_id,
                                                    peer_id,
                                                ),
                                            ) => {
                                                account_peer_mapping_change_sender
                                                    .send((
                                                        account_id,
                                                        peer_id,
                                                        AccountPeerMappingChange::Unregistered,
                                                    ))
                                                    .unwrap();
                                            }
                                            state_chain_runtime::Event::EthereumVault(
                                                pallet_cf_vaults::Event::KeygenRequest(
                                                    ceremony_id,
                                                    validator_candidates,
                                                ),
                                            ) => {
                                                handle_keygen_request(
                                                    multisig_client.clone(),
                                                    state_chain_client.clone(),
                                                    ceremony_id,
                                                    validator_candidates,
                                                    logger.clone()
                                                ).await;
                                            }
                                            state_chain_runtime::Event::EthereumThresholdSigner(
                                                pallet_cf_threshold_signature::Event::ThresholdSignatureRequest(
                                                    ceremony_id,
                                                    key_id,
                                                    validators,
                                                    payload,
                                                ),
                                            ) => {
                                                handle_signing_request(
                                                    multisig_client.clone(),
                                                    state_chain_client.clone(),
                                                    ceremony_id,
                                                    KeyId(key_id),
                                                    validators,
                                                    MessageHash(payload.to_fixed_bytes()),
                                                    logger.clone(),
                                                ).await;
                                            }
                                            state_chain_runtime::Event::EthereumBroadcaster(
                                                pallet_cf_broadcast::Event::TransactionSigningRequest(
                                                    broadcast_attempt_id,
                                                    validator_id,
                                                    unsigned_tx,
                                                ),
                                            ) if validator_id == state_chain_client.our_account_id => {
                                                slog::debug!(
                                                    logger,
                                                    "Received signing request with broadcast_attempt_id {} for transaction: {:?}",
                                                    broadcast_attempt_id,
                                                    unsigned_tx,
                                                );
                                                match eth_broadcaster.encode_and_sign_tx(unsigned_tx).await {
                                                    Ok(raw_signed_tx) => {
                                                        let _result = state_chain_client.submit_signed_extrinsic(
                                                            state_chain_runtime::Call::EthereumBroadcaster(
                                                                pallet_cf_broadcast::Call::transaction_ready_for_transmission {
                                                                    broadcast_attempt_id,
                                                                    signed_tx: raw_signed_tx.0.clone(),
                                                                    signer_id: eth_broadcaster.address,
                                                                },
                                                            ),
                                                            &logger,
                                                        ).await;

                                                        // We want to transmit here to decrease the delay between getting a gas price estimate
                                                        // and transmitting it to the Ethereum network
                                                        eth_broadcaster.send_for_broadcast_attempt(raw_signed_tx.0, broadcast_attempt_id).await
                                                    }
                                                    Err(e) => {
                                                        // Note: this error case should only occur if there is a problem with the
                                                        // local ethereum node, which would mean the web3 lib is unable to fill in
                                                        // the tranaction params, mainly the gas limit.
                                                        // In the long run all transaction parameters will be provided by the state
                                                        // chain and the above eth_broadcaster.sign_tx method can be made
                                                        // infallible.

                                                        slog::error!(
                                                            logger,
                                                            "TransactionSigningRequest attempt_id {} failed: {:?}",
                                                            broadcast_attempt_id,
                                                            e
                                                        );

                                                        let _result = state_chain_client.submit_signed_extrinsic(
                                                            state_chain_runtime::Call::EthereumBroadcaster(
                                                                pallet_cf_broadcast::Call::transaction_signing_failure {
                                                                    broadcast_attempt_id,
                                                                },
                                                            ),
                                                            &logger,
                                                        ).await;
                                                    }
                                                }
                                            }
                                            state_chain_runtime::Event::EthereumBroadcaster(
                                                pallet_cf_broadcast::Event::TransmissionRequest(
                                                    broadcast_attempt_id,
                                                    signed_tx,
                                                ),
                                            ) => {
                                                eth_broadcaster
                                                    .send_for_broadcast_attempt(signed_tx, broadcast_attempt_id).await
                                            }
                                            state_chain_runtime::Event::Environment(
                                                pallet_cf_environment::Event::CfeSettingsUpdated {
                                                    new_cfe_settings
                                                }
                                            ) => {
                                                cfe_settings_update_sender.send(new_cfe_settings).unwrap();
                                            }
                                        }
                                    }
                                }
                            }
                            Err(error) => {
                                slog::error!(
                                    logger,
                                    "Failed to decode events at block {}. {}",
                                    current_block_header.number,
                                    error,
                                );
                            }
                        }

                        // All nodes must send a heartbeat regardless of their validator status (at least for now).
                        // We send it in the middle of the online interval (so any node sync issues don't
                        // cause issues (if we tried to send on one of the interval boundaries)
                        if (current_block_header.number
                            + (state_chain_client.heartbeat_block_interval / 2))
                            % blocks_per_heartbeat
                            == 0
                        {
                            slog::info!(
                                logger,
                                "Sending heartbeat at block: {}",
                                current_block_header.number
                            );
                            let _result = state_chain_client
                                .submit_signed_extrinsic(
                                    pallet_cf_reputation::Call::heartbeat {},
                                    &logger,
                                )
                                .await;
                        }
                    }
                    Err(error) => {
                        slog::error!(logger, "Failed to decode block header: {}", error,);
                    }
                }
            }
            None => {
                slog::error!(logger, "Exiting as State Chain block stream ended");
                break;
            }
        }
    }
}
