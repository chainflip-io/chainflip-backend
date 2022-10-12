#[cfg(test)]
mod tests;

use anyhow::{anyhow, Context};
use cf_primitives::CeremonyId;
use futures::{FutureExt, Stream, StreamExt};
use pallet_cf_vaults::KeygenError;
use slog::o;
use sp_core::H256;
use sp_runtime::AccountId32;
use state_chain_runtime::{AccountId, CfeSettings};
use std::{
	collections::BTreeSet,
	sync::{
		atomic::{AtomicBool, Ordering},
		Arc,
	},
	time::Duration,
};
use tokio::sync::{broadcast, mpsc::UnboundedSender, watch};

use crate::{
	eth::{rpc::EthRpcApi, EpochStart, EthBroadcaster},
	logging::COMPONENT_KEY,
	multisig::{
		client::{CeremonyFailureReason, KeygenFailureReason, KeygenStageName, MultisigClientApi},
		KeyId, MessageHash,
	},
	p2p::{PeerInfo, PeerUpdate},
	state_chain_observer::client::{StateChainClient, StateChainRpcApi},
	task_scope::{with_task_scope, Scope},
};

#[cfg(feature = "ibiza")]
use sp_core::H160;

async fn handle_keygen_request<'a, MultisigClient, RpcClient>(
	scope: &Scope<'a, anyhow::Result<()>, true>,
	multisig_client: Arc<MultisigClient>,
	state_chain_client: Arc<StateChainClient<RpcClient>>,
	ceremony_id: CeremonyId,
	keygen_participants: BTreeSet<AccountId32>,
	logger: slog::Logger,
) where
	MultisigClient: MultisigClientApi<crate::multisig::eth::EthSigning> + Send + Sync + 'static,
	RpcClient: StateChainRpcApi + Send + Sync + 'static,
{
	if keygen_participants.contains(&state_chain_client.our_account_id) {
		scope.spawn(async move {
			let _result = state_chain_client
				.submit_signed_extrinsic(
					pallet_cf_vaults::Call::report_keygen_outcome {
						ceremony_id,
						reported_outcome: multisig_client
							.keygen(ceremony_id, keygen_participants.clone())
							.await
							.map(|point| {
								cf_chains::eth::AggKey::from_pubkey_compressed(
									point.get_element().serialize(),
								)
							})
							.map_err(|(bad_account_ids, reason)| {
								if let CeremonyFailureReason::<
										KeygenFailureReason,
										KeygenStageName,
									>::Other(KeygenFailureReason::KeyNotCompatible) = reason
									{
										KeygenError::Incompatible
									} else {
										KeygenError::Failure(bad_account_ids)
									}
							}),
					},
					&logger,
				)
				.await;
			Ok(())
		});
	} else {
		// If we are not participating, just send an empty ceremony request (needed for ceremony id
		// tracking)
		multisig_client.update_latest_ceremony_id(ceremony_id);
	}
}

async fn handle_signing_request<'a, MultisigClient, RpcClient>(
	scope: &Scope<'a, anyhow::Result<()>, true>,
	multisig_client: Arc<MultisigClient>,
	state_chain_client: Arc<StateChainClient<RpcClient>>,
	ceremony_id: CeremonyId,
	key_id: KeyId,
	signers: BTreeSet<AccountId>,
	data: MessageHash,
	logger: slog::Logger,
) where
	MultisigClient: MultisigClientApi<crate::multisig::eth::EthSigning> + Send + Sync + 'static,
	RpcClient: StateChainRpcApi + Send + Sync + 'static,
{
	if signers.contains(&state_chain_client.our_account_id) {
		// Send a signing request and wait to submit the result to the SC
		scope.spawn(async move {
			match multisig_client.sign(ceremony_id, key_id, signers, data).await {
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
				},
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
				},
			}
			Ok(())
		});
	} else {
		// If we are not participating, just send an empty ceremony request (needed for ceremony id
		// tracking)
		multisig_client.update_latest_ceremony_id(ceremony_id);
	}
}

// Wrap the match so we add a log message before executing the processing of the event
// if we are processing. Else, ignore it.
macro_rules! match_event {
    ($event:expr, $logger:ident { $($(#[$cfg_param:meta])? $bind:pat $(if $condition:expr)? => $block:expr)+ }) => {{
        let event = $event;
        let formatted_event = format!("{:?}", event);
        match event {
            $(
                $(#[$cfg_param])?
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
	peer_update_sender: UnboundedSender<PeerUpdate>,
	epoch_start_sender: broadcast::Sender<EpochStart>,
	#[cfg(feature = "ibiza")] eth_monitor_ingress_sender: tokio::sync::mpsc::UnboundedSender<H160>,
	#[cfg(feature = "ibiza")] eth_monitor_flip_ingress_sender: tokio::sync::mpsc::UnboundedSender<
		H160,
	>,
	#[cfg(feature = "ibiza")] eth_monitor_usdc_ingress_sender: tokio::sync::mpsc::UnboundedSender<
		H160,
	>,
	cfe_settings_update_sender: watch::Sender<CfeSettings>,
	initial_block_hash: H256,
	logger: slog::Logger,
) -> Result<(), anyhow::Error>
where
	BlockStream: Stream<Item = anyhow::Result<state_chain_runtime::Header>> + Send + 'static,
	RpcClient: StateChainRpcApi + Send + Sync + 'static,
	EthRpc: EthRpcApi + Send + Sync + 'static,
	MultisigClient: MultisigClientApi<crate::multisig::eth::EthSigning> + Send + Sync + 'static,
{
	with_task_scope(|scope| async {
        let logger = logger.new(o!(COMPONENT_KEY => "SCObserver"));

        let heartbeat_block_interval = {
            use frame_support::traits::TypedGet;
            <state_chain_runtime::Runtime as pallet_cf_reputation::Config>::HeartbeatBlockInterval::get()
        };

        let blocks_per_heartbeat =
            std::cmp::max(1, heartbeat_block_interval / 2);

        slog::info!(
            logger,
            "Sending heartbeat every {} blocks",
            blocks_per_heartbeat
        );

        let start_epoch = |block_hash: H256, index: u32, current: bool, participant: bool| {
            let epoch_start_sender = &epoch_start_sender;
            let state_chain_client = &state_chain_client;

            async move {
                epoch_start_sender.send(EpochStart {
                    index,
                    eth_block: state_chain_client
                        .get_storage_map::<pallet_cf_vaults::Vaults<
                            state_chain_runtime::Runtime,
                            state_chain_runtime::EthereumInstance,
                        >>(block_hash, &index)
                        .await
                        .unwrap()
                        .unwrap()
                        .active_from_block,
                    current,
                    participant,
                }).unwrap();
            }
        };

        {
            let historical_active_epochs = BTreeSet::from_iter(state_chain_client.get_storage_map::<pallet_cf_validator::HistoricalActiveEpochs<state_chain_runtime::Runtime>>(
                initial_block_hash,
                &state_chain_client.our_account_id
            ).await.unwrap());

            let current_epoch = state_chain_client
                .get_storage_value::<pallet_cf_validator::CurrentEpoch<
                    state_chain_runtime::Runtime,
                >>(initial_block_hash)
                .await
                .unwrap();

            if let Some(earliest_historical_active_epoch) = historical_active_epochs.iter().next() {
                for epoch in *earliest_historical_active_epoch..current_epoch {
                    start_epoch(initial_block_hash, epoch, false, historical_active_epochs.contains(&epoch)).await;
                }
            }

            start_epoch(initial_block_hash, current_epoch, true, historical_active_epochs.contains(&current_epoch)).await;
        }

        // Ensure we don't submit initial heartbeat too early. Early heartbeats could falsely indicate
        // liveness
        let has_submitted_init_heartbeat = Arc::new(AtomicBool::new(false));
        scope.spawn({
            let state_chain_client = state_chain_client.clone();
            let has_submitted_init_heartbeat = has_submitted_init_heartbeat.clone();
            let logger = logger.clone();
            async move {
                tokio::time::sleep(Duration::from_secs(60)).await;
                    state_chain_client
                    .submit_signed_extrinsic(
                        pallet_cf_reputation::Call::heartbeat {},
                        &logger,
                    )
                    .await
                    .context("Failed to submit initial heartbeat")?;
                has_submitted_init_heartbeat.store(true, Ordering::Relaxed);
            Ok(())
        }.boxed()});

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

                            match state_chain_client.get_storage_value::<frame_system::Events::<state_chain_runtime::Runtime>>(current_block_hash).await {
                                Ok(events) => {
                                    for event_record in events {
                                        match_event! {event_record.event, logger {
                                            state_chain_runtime::Event::Validator(
                                                pallet_cf_validator::Event::NewEpoch(new_epoch),
                                            ) => {
                                                start_epoch(current_block_hash, new_epoch, true, state_chain_client.get_storage_double_map::<pallet_cf_validator::AuthorityIndex<state_chain_runtime::Runtime>>(
                                                    current_block_hash,
                                                    &new_epoch,
                                                    &state_chain_client.our_account_id
                                                ).await.unwrap().is_some()).await;
                                            }
                                            state_chain_runtime::Event::Validator(
                                                pallet_cf_validator::Event::PeerIdRegistered(
                                                    account_id,
                                                    ed25519_pubkey,
                                                    port,
                                                    ip_address,
                                                ),
                                            ) => {
                                                peer_update_sender
                                                    .send(PeerUpdate::Registered(
                                                            PeerInfo::new(account_id, ed25519_pubkey, ip_address.into(), port)
                                                        )
                                                    )
                                                    .unwrap();
                                            }
                                            state_chain_runtime::Event::Validator(
                                                pallet_cf_validator::Event::PeerIdUnregistered(
                                                    account_id,
                                                    ed25519_pubkey,
                                                ),
                                            ) => {
                                                peer_update_sender
                                                    .send(PeerUpdate::Deregistered(account_id, ed25519_pubkey))
                                                    .unwrap();
                                            }
                                            state_chain_runtime::Event::EthereumVault(
                                                pallet_cf_vaults::Event::KeygenRequest(
                                                    ceremony_id,
                                                    keygen_participants,
                                                ),
                                            ) => {
                                                handle_keygen_request(
                                                    scope,
                                                    multisig_client.clone(),
                                                    state_chain_client.clone(),
                                                    ceremony_id,
                                                    keygen_participants,
                                                    logger.clone()
                                                ).await;
                                            }
                                            state_chain_runtime::Event::EthereumThresholdSigner(
                                                pallet_cf_threshold_signature::Event::ThresholdSignatureRequest(
                                                    ceremony_id,
                                                    key_id,
                                                    signers,
                                                    payload,
                                                ),
                                            ) => {
                                                handle_signing_request(
                                                        scope,
                                                    multisig_client.clone(),
                                                    state_chain_client.clone(),
                                                    ceremony_id,
                                                    KeyId(key_id),
                                                    signers,
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
                                            #[cfg(feature = "ibiza")]
                                            state_chain_runtime::Event::Ingress(
                                                pallet_cf_ingress::Event::StartWitnessing {
                                                    ingress_address,
                                                    ingress_asset
                                                }
                                            ) => {
                                                use cf_primitives::{Asset, ForeignChain, ForeignChainAddress};
                                                if let ForeignChainAddress::Eth(address) = ingress_address {
                                                    assert_eq!(ingress_asset.chain, ForeignChain::Ethereum);
                                                    match ingress_asset.asset {
                                                        Asset::Eth => {
                                                            eth_monitor_ingress_sender.send(H160::from(address)).unwrap();
                                                        }
                                                        Asset::Flip => {
                                                            eth_monitor_flip_ingress_sender.send(H160::from(address)).unwrap();
                                                        }
                                                        Asset::Usdc => {
                                                            eth_monitor_usdc_ingress_sender.send(H160::from(address)).unwrap();
                                                        }
                                                        _ => {
                                                            slog::warn!(logger, "Not a supported asset: {:?}", ingress_asset);
                                                        }
                                                    }
                                                } else {
                                                    slog::warn!(logger, "Unsupported addresss: {:?}", ingress_address);
                                                }
                                            }
                                        }}
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
                            if ((current_block_header.number
                                + (heartbeat_block_interval / 2))
                                % blocks_per_heartbeat
                                // Submitting earlier than one minute in may falsely indicate liveness.
                                == 0) && has_submitted_init_heartbeat.load(Ordering::Relaxed)
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
        Err(anyhow!("State Chain block stream ended"))
    }.boxed()).await
}
