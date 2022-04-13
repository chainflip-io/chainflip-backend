use cf_chains::Ethereum;
use cf_traits::{ChainflipAccountData, ChainflipAccountState};
use futures::{Stream, StreamExt};
use pallet_cf_broadcast::TransmissionFailure;
use pallet_cf_validator::CeremonyId;
use pallet_cf_vaults::BlockHeightWindow;
use slog::o;
use sp_core::H256;
use sp_runtime::AccountId32;
use state_chain_runtime::AccountId;
use std::{collections::BTreeSet, iter::FromIterator, sync::Arc};
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    eth::{EthBroadcaster, EthRpcApi},
    logging::{COMPONENT_KEY, LOG_ACCOUNT_STATE},
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
    MultisigClient: MultisigClientApi + Send + Sync + 'static,
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
                    Err((bad_account_ids, _error)) => {
                        KeygenOutcome::Failure(BTreeSet::from_iter(bad_account_ids))
                    }
                }
            }
            Err((bad_account_ids, _error)) => {
                KeygenOutcome::Failure(BTreeSet::from_iter(bad_account_ids))
            }
        };

        let _result = state_chain_client
            .submit_signed_extrinsic(
                pallet_cf_vaults::Call::report_keygen_outcome(ceremony_id, keygen_outcome),
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
    sm_window_sender: UnboundedSender<BlockHeightWindow>,
    km_window_sender: UnboundedSender<BlockHeightWindow>,
    initial_block_hash: H256,
    logger: &slog::Logger,
) where
    BlockStream: Stream<Item = anyhow::Result<state_chain_runtime::Header>>,
    RpcClient: StateChainRpcApi + Send + Sync + 'static,
    EthRpc: EthRpcApi,
    MultisigClient: MultisigClientApi + Send + Sync + 'static,
{
    let logger = logger.new(o!(COMPONENT_KEY => "SCObserver"));

    let blocks_per_heartbeat = std::cmp::max(1, state_chain_client.heartbeat_block_interval / 2);

    slog::info!(
        logger,
        "Sending heartbeat every {} blocks",
        blocks_per_heartbeat
    );

    state_chain_client
        .submit_signed_extrinsic(pallet_cf_online::Call::heartbeat(), &logger)
        .await
        .expect("Should be able to submit first heartbeat");

    async fn get_current_account_data<RpcClient: StateChainRpcApi>(
        state_chain_client: Arc<StateChainClient<RpcClient>>,
        block_hash: H256,
        logger: &slog::Logger,
    ) -> ChainflipAccountData {
        let new_account_data = state_chain_client
            .get_account_data(block_hash)
            .await
            .expect("Could not get account data");

        slog::debug!(logger, #LOG_ACCOUNT_STATE, "Account state: {:?}",  new_account_data.state);

        new_account_data
    }

    async fn send_windows_to_witness_processes<RpcClient: StateChainRpcApi>(
        state_chain_client: Arc<StateChainClient<RpcClient>>,
        block_hash: H256,
        sm_window_sender: &UnboundedSender<BlockHeightWindow>,
        km_window_sender: &UnboundedSender<BlockHeightWindow>,
    ) -> anyhow::Result<()> {
        // TODO: Use all the historical epochs: https://github.com/chainflip-io/chainflip-backend/issues/1218
        let last_active_epoch = *state_chain_client
            .get_historical_active_epochs(block_hash)
            .await?
            .last()
            .expect("Must exist if we're sending windows to witness processes");

        let eth_vault = state_chain_client
            .get_vault::<Ethereum>(block_hash, last_active_epoch)
            .await?;

        sm_window_sender
            .send(eth_vault.active_window.clone())
            .unwrap();
        km_window_sender.send(eth_vault.active_window).unwrap();
        Ok(())
    }

    // Initialise the account state
    let mut account_data =
        get_current_account_data(state_chain_client.clone(), initial_block_hash, &logger).await;

    if account_data.state == ChainflipAccountState::CurrentAuthority
        || matches!(
            account_data.state,
            ChainflipAccountState::HistoricalAuthority(_)
        )
    {
        send_windows_to_witness_processes(
            state_chain_client.clone(),
            initial_block_hash,
            &sm_window_sender,
            &km_window_sender,
        )
        .await
        .expect("Failed to send windows to the witness processes");
    }

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

                                let mut received_new_epoch = false;

                                // Process this block's events
                                match state_chain_client.get_events(current_block_hash).await {
                                    Ok(events) => {
                                        for (_phase, event, _topics) in events {

                                            slog::debug!(logger, "Received event at block {}: {:?}", current_block_header.number, &event);

                                            match event {
                                                state_chain_runtime::Event::Validator(
                                                    pallet_cf_validator::Event::NewEpoch(_),
                                                ) => {
                                                    received_new_epoch = true;
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
                                                                        pallet_cf_threshold_signature::Call::signature_success(ceremony_id, signature.into()),
                                                                        &logger,
                                                                    )
                                                                    .await;
                                                            }
                                                            Err((bad_account_ids, _error)) => {
                                                                let _result = state_chain_client
                                                                    .submit_signed_extrinsic(
                                                                        pallet_cf_threshold_signature::Call::report_signature_failed_unbounded(
                                                                            ceremony_id,
                                                                            BTreeSet::from_iter(bad_account_ids),
                                                                        ),
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
                                                                    pallet_cf_broadcast::Call::transaction_ready_for_transmission(
                                                                        attempt_id,
                                                                        raw_signed_tx.0,
                                                                        eth_broadcaster.address,
                                                                    ),
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
                                                    let response_extrinsic = match eth_broadcaster
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
                                                            pallet_cf_witnesser_api::Call::witness_eth_transmission_success(
                                                                attempt_id, tx_hash
                                                            )
                                                        }
                                                        Err(e) => {
                                                            slog::error!(
                                                                logger,
                                                                "TransmissionRequest attempt_id {} failed: {:?}",
                                                                attempt_id,
                                                                e
                                                            );
                                                            // TODO: Fill in the transaction hash with the real one
                                                            pallet_cf_witnesser_api::Call::witness_eth_transmission_failure(
                                                                attempt_id, TransmissionFailure::TransactionRejected, Default::default()
                                                            )
                                                        }
                                                    };
                                                    let _result = state_chain_client
                                                        .submit_signed_extrinsic(response_extrinsic, &logger)
                                                        .await;
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

                                // Backup and passive nodes (could be historical backup or passive) need to update their state on every block (since it's possible to move
                                // between Backup and Passive on every block), while CurrentAuthority nodes only need to update every new epoch.
                                if received_new_epoch || matches!(
                                    account_data.state,
                                    ChainflipAccountState::BackupOrPassive(_)) || matches!(account_data.state, ChainflipAccountState::HistoricalAuthority(_))
                                 {
                                    account_data = get_current_account_data(state_chain_client.clone(), current_block_hash, &logger).await;
                                }

                                // New windows should be sent to HistoricalAuthority validators (so they know when to finish) or to
                                // new/existing CurrentAuthoritys (so they know when to start)
                                // Note: nodes that were outgoing in the last epoch (active 2 epochs ago) have already
                                // received their end window, so we don't need to send anything to them
                                if received_new_epoch && (matches!(account_data.state, ChainflipAccountState::CurrentAuthority) || matches!(account_data.state, ChainflipAccountState::HistoricalAuthority(_))) {
                                    send_windows_to_witness_processes(
                                        state_chain_client.clone(),
                                        current_block_hash,
                                        &sm_window_sender,
                                        &km_window_sender,
                                    )
                                    .await
                                    .expect("Failed to send windows to the witness processes");
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
                                        .submit_signed_extrinsic(pallet_cf_online::Call::heartbeat(), &logger)
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
