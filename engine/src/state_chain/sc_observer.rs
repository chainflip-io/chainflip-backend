use futures::{stream, Stream, StreamExt};
use pallet_cf_validator::CeremonyId;
use slog::o;
use sp_core::{Hasher, H256};
use sp_runtime::{traits::Keccak256, AccountId32};
use state_chain_runtime::AccountId;
use std::{collections::BTreeSet, iter::FromIterator, sync::Arc};
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    eth::{rpc::EthRpcApi, EthBroadcaster, ObserveInstruction},
    logging::COMPONENT_KEY,
    multisig::{client::MultisigClientApi, KeyId, MessageHash},
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
    use pallet_cf_vaults::KeygenOutcome;

    tokio::spawn(async move {
        let keygen_outcome = multisig_client
            .keygen(ceremony_id, validator_candidates.clone())
            .await;

        let keygen_outcome = match keygen_outcome {
            Ok(public_key) => {
                // Keygen verification: before the new key is returned to the SC,
                // we first ensure that all parties can use it for signing

                let public_key_bytes = public_key.serialize();

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
                    Ok(_signature) => KeygenOutcome::Success(
                        cf_chains::eth::AggKey::from_pubkey_compressed(public_key_bytes),
                    ),
                    // Report keygen failure if we failed to sign
                    Err((bad_account_ids, error)) => {
                        slog::debug!(
                            logger,
                            "Keygen ceremony {} verification failed: {:?}",
                            ceremony_id,
                            error
                        );
                        KeygenOutcome::Failure(BTreeSet::from_iter(bad_account_ids))
                    }
                }
            }
            Err((bad_account_ids, error)) => {
                slog::debug!(
                    logger,
                    "Keygen ceremony {} failed: {:?}",
                    ceremony_id,
                    error
                );
                KeygenOutcome::Failure(BTreeSet::from_iter(bad_account_ids))
            }
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

    // TODO: we should be able to factor this out into a single ETH window sender
    sm_instruction_sender: UnboundedSender<ObserveInstruction>,
    km_instruction_sender: UnboundedSender<ObserveInstruction>,
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
        .submit_signed_extrinsic(pallet_cf_online::Call::heartbeat {}, &logger)
        .await
        .expect("Should be able to submit first heartbeat");

    let send_instruction = |observe_instruction: ObserveInstruction| {
        km_instruction_sender
            .send(observe_instruction.clone())
            .unwrap();
        sm_instruction_sender.send(observe_instruction).unwrap();
    };

    macro_rules! start_epoch_observation {
        ($state_chain_client:ident, $block_hash:expr, $epoch:expr) => {
            let block_hash = $block_hash;
            let epoch = $epoch;

            send_instruction(ObserveInstruction::Start(
                $state_chain_client
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
        };
    }

    macro_rules! try_end_previous_epoch_observation {
        ($state_chain_client:ident, $block_hash:expr, $epoch:expr) => {{
            let block_hash = $block_hash;
            let epoch = $epoch;

            if let Some(vault) = $state_chain_client
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
        }};
    }

    let historical_active_epochs = state_chain_client.get_storage_map::<pallet_cf_validator::HistoricalActiveEpochs<state_chain_runtime::Runtime>>(
        initial_block_hash,
        &state_chain_client.our_account_id
    ).await.unwrap();

    assert!(historical_active_epochs.iter().is_sorted());

    let mut active_in_current_epoch = stream::iter(historical_active_epochs.into_iter())
        .fold(false, |acc, epoch| {
            let state_chain_client = state_chain_client.clone();
            async move {
                start_epoch_observation!(state_chain_client, initial_block_hash, epoch);
                if try_end_previous_epoch_observation!(
                    state_chain_client,
                    initial_block_hash,
                    epoch + 1
                ) {
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
        tokio::select! {
            option_result_block_header = sc_block_stream.next() => {
                match option_result_block_header {
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

                                // Process this block's events
                                match state_chain_client.get_events(current_block_hash).await {
                                    Ok(events) => {
                                        for (_phase, event, _topics) in events {

                                            slog::debug!(logger, "Received event at block {}: {:?}", current_block_header.number, &event);

                                            match event {
                                                state_chain_runtime::Event::Validator(
                                                    pallet_cf_validator::Event::NewEpoch(new_epoch),
                                                ) => {
                                                    let current_block_hash = current_block_header.hash();

                                                    if active_in_current_epoch {
                                                        assert!(try_end_previous_epoch_observation!(state_chain_client, current_block_hash, new_epoch));
                                                    }

                                                    active_in_current_epoch = state_chain_client.get_storage_double_map::<pallet_cf_validator::AuthorityIndex<state_chain_runtime::Runtime>>(
                                                        current_block_hash,
                                                        &new_epoch,
                                                        &state_chain_client.our_account_id
                                                    ).await.unwrap().is_some();

                                                    if active_in_current_epoch {
                                                        start_epoch_observation!(state_chain_client, current_block_hash, new_epoch);
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
                                                ) if validator_candidates.contains(&state_chain_client.our_account_id) => {
                                                    handle_keygen_request(multisig_client.clone(), state_chain_client.clone(), ceremony_id, validator_candidates, logger.clone()).await;
                                                }
                                                state_chain_runtime::Event::EthereumThresholdSigner(
                                                    pallet_cf_threshold_signature::Event::ThresholdSignatureRequest(
                                                        ceremony_id,
                                                        key_id,
                                                        validators,
                                                        payload,
                                                    ),
                                                ) if validators.contains(&state_chain_client.our_account_id) => {
                                                    let multisig_client = multisig_client.clone();
                                                    let state_chain_client = state_chain_client.clone();
                                                    let logger = logger.clone();
                                                    tokio::spawn(async move {
                                                        match multisig_client.sign(ceremony_id, KeyId(key_id), validators, MessageHash(payload.to_fixed_bytes())).await {
                                                            Ok(signature) => {
                                                                let _result = state_chain_client
                                                                    .submit_unsigned_extrinsic(
                                                                        pallet_cf_threshold_signature::Call::signature_success {
                                                                            ceremony_id,
                                                                            signature: signature.into()
                                                                        },
                                                                        &logger,
                                                                    )
                                                                    .await;
                                                            }
                                                            Err((bad_account_ids, error)) => {
                                                                slog::debug!(logger, "Threshold signing ceremony {} failed: {:?}", ceremony_id, error);
                                                                let _result = state_chain_client
                                                                    .submit_signed_extrinsic(
                                                                        pallet_cf_threshold_signature::Call::report_signature_failed_unbounded {
                                                                            id: ceremony_id,
                                                                            offenders: BTreeSet::from_iter(bad_account_ids),
                                                                        },
                                                                        &logger,
                                                                    )
                                                                    .await;
                                                            }
                                                        }
                                                    });
                                                }
                                                state_chain_runtime::Event::EthereumBroadcaster(
                                                    pallet_cf_broadcast::Event::TransactionSigningRequest(
                                                        attempt_id,
                                                        validator_id,
                                                        unsigned_tx,
                                                    ),
                                                ) if validator_id == state_chain_client.our_account_id => {
                                                    slog::debug!(
                                                        logger,
                                                        "Received signing request with attempt_id {} for transaction: {:?}",
                                                        attempt_id,
                                                        unsigned_tx,
                                                    );
                                                    match eth_broadcaster.encode_and_sign_tx(unsigned_tx).await {
                                                        Ok(raw_signed_tx) => {
                                                            let _result = state_chain_client.submit_signed_extrinsic(
                                                                state_chain_runtime::Call::EthereumBroadcaster(
                                                                    pallet_cf_broadcast::Call::transaction_ready_for_transmission {
                                                                        broadcast_attempt_id: attempt_id,
                                                                        signed_tx: raw_signed_tx.0,
                                                                        signer_id: eth_broadcaster.address,
                                                                    },
                                                                ),
                                                                &logger,
                                                            ).await;
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
                                                                attempt_id,
                                                                e
                                                            );
                                                        }
                                                    }
                                                }
                                                state_chain_runtime::Event::EthereumBroadcaster(
                                                    pallet_cf_broadcast::Event::TransmissionRequest(
                                                        attempt_id,
                                                        signed_tx,
                                                    ),
                                                ) => {
                                                    let expected_broadcast_tx_hash = Keccak256::hash(&signed_tx[..]);
                                                    match eth_broadcaster
                                                        .send(signed_tx)
                                                        .await
                                                    {
                                                        Ok(tx_hash) => {
                                                            slog::debug!(
                                                                logger,
                                                                "Successful TransmissionRequest attempt_id {}, tx_hash: {:#x}",
                                                                attempt_id,
                                                                tx_hash
                                                            );
                                                            assert_eq!(tx_hash, expected_broadcast_tx_hash, "tx_hash returned from `send` does not match expected hash");
                                                        }
                                                        Err(e) => {
                                                            slog::info!(
                                                                logger,
                                                                "TransmissionRequest attempt_id {} failed: {:?}",
                                                                attempt_id,
                                                                e
                                                            );
                                                        }
                                                    };
                                                }
                                                ignored_event => {
                                                    // ignore events we don't care about
                                                    slog::trace!(
                                                        logger,
                                                        "Ignoring event at block {}: {:?}",
                                                        current_block_header.number,
                                                        ignored_event
                                                    );
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
                                if (current_block_header.number + (state_chain_client.heartbeat_block_interval / 2))
                                    % blocks_per_heartbeat
                                    == 0
                                {
                                    slog::info!(
                                        logger,
                                        "Sending heartbeat at block: {}",
                                        current_block_header.number
                                    );
                                    let _result = state_chain_client
                                        .submit_signed_extrinsic(pallet_cf_online::Call::heartbeat {}, &logger)
                                        .await;
                                }
                            }
                            Err(error) => {
                                slog::error!(logger, "Failed to decode block header: {}", error,);
                            }
                        }
                    },
                    None => {
                        slog::error!(logger, "Exiting as State Chain block stream ended");
                        break
                    }
                }
            },
        }
    }
}
