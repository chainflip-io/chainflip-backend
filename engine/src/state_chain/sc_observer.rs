use cf_chains::ChainId;
use cf_traits::{ChainflipAccountData, ChainflipAccountState};
use futures::{Stream, StreamExt};
use pallet_cf_broadcast::TransmissionFailure;
use pallet_cf_vaults::BlockHeightWindow;
use slog::o;
use sp_core::H256;
use sp_runtime::AccountId32;
use std::{collections::BTreeSet, iter::FromIterator, sync::Arc};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::{
    eth::EthBroadcaster,
    logging::COMPONENT_KEY,
    multisig::{
        KeyId, KeygenInfo, KeygenOutcome, MessageHash, MultisigInstruction, MultisigOutcome,
        SigningInfo, SigningOutcome,
    },
    p2p::{self, AccountId, AccountPeerMappingChange},
    state_chain::client::{StateChainClient, StateChainRpcApi},
};

pub async fn start<BlockStream, RpcClient>(
    state_chain_client: Arc<StateChainClient<RpcClient>>,
    sc_block_stream: BlockStream,
    eth_broadcaster: EthBroadcaster,
    multisig_instruction_sender: UnboundedSender<MultisigInstruction>,
    account_peer_mapping_change_sender: UnboundedSender<(
        AccountId,
        sp_core::ed25519::Public,
        AccountPeerMappingChange,
    )>,
    mut multisig_outcome_receiver: UnboundedReceiver<MultisigOutcome>,

    // TODO: we should be able to factor this out into a single ETH window sender
    sm_window_sender: UnboundedSender<BlockHeightWindow>,
    km_window_sender: UnboundedSender<BlockHeightWindow>,
    latest_block_hash: H256,
    logger: &slog::Logger,
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

    state_chain_client
        .submit_signed_extrinsic(&logger, pallet_cf_online::Call::heartbeat())
        .await
        .expect("Should be able to submit first heartbeat");

    async fn get_current_account_state<RpcClient: StateChainRpcApi>(
        state_chain_client: Arc<StateChainClient<RpcClient>>,
        block_hash: H256,
    ) -> (ChainflipAccountData, bool) {
        let new_account_data = state_chain_client
            .get_account_data(block_hash)
            .await
            .expect("Could not get account data");

        let current_epoch = state_chain_client
            .epoch_at_block(block_hash)
            .await
            .expect("Could not get current epoch");

        let is_outgoing = if let Some(last_active_epoch) = new_account_data.last_active_epoch {
            last_active_epoch + 1 == current_epoch
        } else {
            false
        };

        (new_account_data, is_outgoing)
    }

    async fn send_windows_to_witness_processes<RpcClient: StateChainRpcApi>(
        state_chain_client: Arc<StateChainClient<RpcClient>>,
        block_hash: H256,
        account_data: ChainflipAccountData,
        sm_window_sender: &UnboundedSender<BlockHeightWindow>,
        km_window_sender: &UnboundedSender<BlockHeightWindow>,
    ) -> anyhow::Result<()> {
        let eth_vault = state_chain_client
            .get_vault(
                block_hash,
                account_data
                    .last_active_epoch
                    .expect("we are active or outgoing"),
                ChainId::Ethereum,
            )
            .await?;
        sm_window_sender
            .send(eth_vault.active_window.clone())
            .unwrap();
        km_window_sender.send(eth_vault.active_window).unwrap();
        Ok(())
    }

    let (mut account_data, mut is_outgoing) =
        get_current_account_state(state_chain_client.clone(), latest_block_hash).await;

    // Initialise the account state
    if account_data.state == ChainflipAccountState::Validator || is_outgoing {
        send_windows_to_witness_processes(
            state_chain_client.clone(),
            latest_block_hash,
            account_data,
            &sm_window_sender,
            &km_window_sender,
        )
        .await
        .expect("Failed to send windows to the witness processes");
    }

    let mut sc_block_stream = Box::pin(sc_block_stream);
    while let Some(result_block_header) = sc_block_stream.next().await {
        match result_block_header {
            Ok(block_header) => {
                let block_hash = block_header.hash();

                let mut received_new_epoch = false;

                // Process this block's events
                match state_chain_client.get_events(block_hash).await {
                    Ok(events) => {
                        for (_phase, event, _topics) in events {
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
                                    ),
                                ) => {
                                    account_peer_mapping_change_sender
                                        .send((
                                            AccountId(*account_id.as_ref()),
                                            peer_id,
                                            AccountPeerMappingChange::Registered,
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
                                            AccountId(*account_id.as_ref()),
                                            peer_id,
                                            AccountPeerMappingChange::Unregistered,
                                        ))
                                        .unwrap();
                                }
                                state_chain_runtime::Event::Vaults(
                                    pallet_cf_vaults::Event::KeygenRequest(
                                        ceremony_id,
                                        chain_id,
                                        validator_candidates,
                                    ),
                                ) => {
                                    let signers: Vec<_> = validator_candidates
                                        .iter()
                                        .map(|v| p2p::AccountId(v.clone().into()))
                                        .collect();

                                    let gen_new_key_event = MultisigInstruction::Keygen(
                                        KeygenInfo::new(ceremony_id, signers),
                                    );

                                    multisig_instruction_sender
                                        .send(gen_new_key_event)
                                        .map_err(|_| "Receiver should exist")
                                        .unwrap();

                                    match multisig_outcome_receiver
                                        .recv()
                                        .await
                                        .expect("Channel closed!")
                                    {
                                        MultisigOutcome::Keygen(KeygenOutcome {
                                            id: _,
                                            result,
                                        }) => match result {
                                            Ok(pubkey) => {
                                                let _ = state_chain_client
                                                    .submit_signed_extrinsic(&logger, pallet_cf_vaults::Call::report_keygen_outcome(
                                                        ceremony_id,
                                                        chain_id,
                                                        pallet_cf_vaults::KeygenOutcome::Success(
                                                            pubkey.serialize().to_vec(),
                                                        ),
                                                    ))
                                                    .await;
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

                                                let _ = state_chain_client
                                                    .submit_signed_extrinsic(&logger, pallet_cf_vaults::Call::report_keygen_outcome(
                                                        ceremony_id,
                                                        chain_id,
                                                        pallet_cf_vaults::KeygenOutcome::Failure(
                                                            BTreeSet::from_iter(bad_account_ids),
                                                        ),
                                                    ))
                                                    .await;
                                            }
                                        },
                                        MultisigOutcome::Ignore => {
                                            // ignore
                                        }
                                        MultisigOutcome::Signing(message_signing_result) => {
                                            panic!(
                                                "Expecting KeygenResult, got: {:?}",
                                                message_signing_result
                                            );
                                        }
                                    };
                                }
                                state_chain_runtime::Event::EthereumThresholdSigner(
                                    pallet_cf_threshold_signature::Event::ThresholdSignatureRequest(
                                        ceremony_id,
                                        key_id,
                                        validators,
                                        payload,
                                    ),
                                ) if validators.contains(&state_chain_client.our_account_id) => {
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

                                    match multisig_outcome_receiver
                                        .recv()
                                        .await
                                        .expect("Channel closed!")
                                    {
                                        MultisigOutcome::Signing(SigningOutcome {
                                            id: _,
                                            result,
                                        }) => match result {
                                            Ok(sig) => {
                                                let _ = state_chain_client
                                                    .submit_unsigned_extrinsic(
                                                        &logger,
                                                        pallet_cf_threshold_signature::Call::signature_success(
                                                            ceremony_id,
                                                            sig.into()
                                                        )
                                                    )
                                                    .await;
                                            }
                                            Err((_, bad_account_ids)) => {
                                                let bad_account_ids: BTreeSet<_> = bad_account_ids
                                                    .iter()
                                                    .map(|v| AccountId32::from(v.0))
                                                    .collect();
                                                let _ = state_chain_client
                                                    .submit_signed_extrinsic(
                                                        &logger,
                                                        pallet_cf_threshold_signature::Call::report_signature_failed_unbounded(
                                                            ceremony_id,
                                                            bad_account_ids
                                                        )
                                                    )
                                                    .await;
                                            }
                                        },
                                        MultisigOutcome::Ignore => {
                                            // ignore
                                        }
                                        MultisigOutcome::Keygen(keygen_result) => {
                                            panic!(
                                                "Expecting MessageSigningResult, got: {:?}",
                                                keygen_result
                                            );
                                        }
                                    };
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
                                        "Received signing request {} for transaction: {:?}",
                                        attempt_id,
                                        unsigned_tx,
                                    );
                                    match eth_broadcaster.encode_and_sign_tx(unsigned_tx).await {
                                        Ok(raw_signed_tx) => {
                                            let _ = state_chain_client.submit_signed_extrinsic(
                                                &logger,
                                                state_chain_runtime::Call::EthereumBroadcaster(
                                                    pallet_cf_broadcast::Call::transaction_ready_for_transmission(
                                                        attempt_id,
                                                        raw_signed_tx.0,
                                                        eth_broadcaster.address,
                                                    ),
                                                )
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
                                                "Transaction signing attempt {} failed: {:?}",
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
                                    slog::debug!(
                                        logger,
                                        "Sending signed tx for broadcast attempt {}: {:?}",
                                        attempt_id,
                                        hex::encode(&signed_tx),
                                    );
                                    let response_extrinsic = match eth_broadcaster
                                        .send(signed_tx)
                                        .await
                                    {
                                        Ok(tx_hash) => {
                                            slog::debug!(
                                                logger,
                                                "Successful broadcast attempt {}, tx_hash: {:#x}",
                                                attempt_id,
                                                tx_hash
                                            );
                                            pallet_cf_witnesser_api::Call::witness_eth_transmission_success(
                                                attempt_id, tx_hash.into()
                                            )
                                        }
                                        Err(e) => {
                                            slog::error!(
                                                logger,
                                                "Broadcast attempt {} failed: {:?}",
                                                attempt_id,
                                                e
                                            );
                                            // TODO: Fill in the transaction hash with the real one
                                            pallet_cf_witnesser_api::Call::witness_eth_transmission_failure(
                                                attempt_id, TransmissionFailure::TransactionRejected, Default::default()
                                            )
                                        }
                                    };
                                    let _ = state_chain_client
                                        .submit_signed_extrinsic(&logger, response_extrinsic)
                                        .await;
                                }
                                ignored_event => {
                                    // ignore events we don't care about
                                    slog::trace!(
                                        logger,
                                        "Ignoring event at block {}: {:?}",
                                        block_header.number,
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
                            block_header.number,
                            error,
                        );
                    }
                }

                // if we receive a new epoch, there are a few scenarios:
                // 1. Validators in the last epoch couuld now be outgoing, so we should send the windows to them (as they now contain the end)
                // 2. New validators need to receive their start point windows, so send windows to them
                // 3. Validators from the previous epoch that continue to be validators, we can send to them (has no impact, they'll just keep going)
                // 4. Note: Nodes that were outgoing in the last epoch (active 2 epochs ago) have already received their end window, so we don't
                // need to send anything to them
                if received_new_epoch {
                    let (new_account_data, new_is_outgoing) =
                        get_current_account_state(state_chain_client.clone(), block_hash).await;
                    account_data = new_account_data;
                    is_outgoing = new_is_outgoing;

                    if matches!(account_data.state, ChainflipAccountState::Validator) || is_outgoing
                    {
                        send_windows_to_witness_processes(
                            state_chain_client.clone(),
                            latest_block_hash,
                            account_data,
                            &sm_window_sender,
                            &km_window_sender,
                        )
                        .await
                        .expect("Failed to send windows to the witness processes");
                    }
                } else if matches!(
                    account_data.state,
                    ChainflipAccountState::Backup | ChainflipAccountState::Passive
                ) {
                    // If we are Backup or Passive, we must update our state on every block, since it's possible
                    // we move between Backup and Passive on every block
                    let (new_account_data, new_is_outgoing) =
                        get_current_account_state(state_chain_client.clone(), block_hash).await;
                    account_data = new_account_data;
                    is_outgoing = new_is_outgoing;
                }

                // If we are Backup, Validator or outoing, we need to send a heartbeat
                // we send it in the middle of the online interval (so any node sync issues don't
                // cause issues (if we tried to send on one of the interval boundaries)
                if (matches!(account_data.state, ChainflipAccountState::Backup)
                    || matches!(account_data.state, ChainflipAccountState::Validator)
                    || is_outgoing)
                    && ((block_header.number + (heartbeat_block_interval / 2))
                        % heartbeat_block_interval
                        == 0)
                {
                    slog::info!(
                        logger,
                        "Sending heartbeat at block: {}",
                        block_header.number
                    );
                    let _ = state_chain_client
                        .submit_signed_extrinsic(&logger, pallet_cf_online::Call::heartbeat())
                        .await;
                }
            }
            Err(error) => {
                slog::error!(logger, "Failed to decode block header: {}", error,);
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
        let logger = logging::test_utils::new_test_logger();

        let (latest_block_hash, block_stream, state_chain_client) =
            crate::state_chain::client::connect_to_state_chain(&settings.state_chain, &logger)
                .await
                .unwrap();

        let (multisig_instruction_sender, _multisig_instruction_receiver) =
            tokio::sync::mpsc::unbounded_channel::<MultisigInstruction>();
        let (account_peer_mapping_change_sender, _account_peer_mapping_change_receiver) =
            tokio::sync::mpsc::unbounded_channel();
        let (_multisig_event_sender, multisig_outcome_receiver) =
            tokio::sync::mpsc::unbounded_channel::<MultisigOutcome>();

        let web3 = eth::new_synced_web3_client(&settings.eth, &logger)
            .await
            .unwrap();
        let eth_broadcaster = EthBroadcaster::new(&settings.eth, web3.clone()).unwrap();

        let (sm_window_sender, _sm_window_receiver) =
            tokio::sync::mpsc::unbounded_channel::<BlockHeightWindow>();
        let (km_window_sender, _km_window_receiver) =
            tokio::sync::mpsc::unbounded_channel::<BlockHeightWindow>();

        start(
            state_chain_client,
            block_stream,
            eth_broadcaster,
            multisig_instruction_sender,
            account_peer_mapping_change_sender,
            multisig_outcome_receiver,
            sm_window_sender,
            km_window_sender,
            latest_block_hash,
            &logger,
        )
        .await;
    }
}
