#[cfg(test)]
mod tests;

use anyhow::{anyhow, Context};
use cf_chains::{dot, eth::Ethereum, ChainCrypto, Polkadot};
use cf_primitives::{BlockNumber, CeremonyId, PolkadotAccountId};
use futures::{FutureExt, Stream, StreamExt};
use slog::o;
use sp_core::{Hasher, H160, H256};
use sp_runtime::{traits::Keccak256, AccountId32};
use state_chain_runtime::{AccountId, CfeSettings, EthereumInstance, PolkadotInstance};
use std::{
	collections::BTreeSet,
	sync::{
		atomic::{AtomicBool, Ordering},
		Arc,
	},
	time::Duration,
};
use tokio::sync::{mpsc::UnboundedSender, watch};

use crate::{
	dot::{rpc::DotRpcApi, DotBroadcaster},
	eth::{rpc::EthRpcApi, EthBroadcaster},
	logging::COMPONENT_KEY,
	multisig::{
		client::MultisigClientApi, eth::EthSigning, polkadot::PolkadotSigning, CryptoScheme, KeyId,
	},
	p2p::{PeerInfo, PeerUpdate},
	state_chain_observer::client::{extrinsic_api::ExtrinsicApi, storage_api::StorageApi},
	task_scope::{task_scope, Scope},
	witnesser::EpochStart,
};

pub struct EthAddressToMonitorSender {
	pub eth: UnboundedSender<H160>,
	pub flip: UnboundedSender<H160>,
	pub usdc: UnboundedSender<H160>,
}


async fn handle_keygen_request<'a, StateChainClient, MultisigClient, C, I>(
	scope: &Scope<'a, anyhow::Error>,
	multisig_client: &'a MultisigClient,
	state_chain_client: Arc<StateChainClient>,
	ceremony_id: CeremonyId,
	keygen_participants: BTreeSet<AccountId32>,
	logger: slog::Logger,
) where
	MultisigClient: MultisigClientApi<C>,
	StateChainClient: ExtrinsicApi + 'static + Send + Sync,
    state_chain_runtime::Runtime: pallet_cf_vaults::Config<I>,
	C: CryptoScheme<AggKey = <<state_chain_runtime::Runtime as pallet_cf_vaults::Config<I>>::Chain as ChainCrypto>::AggKey>,
	I: 'static + Sync + Send,
	state_chain_runtime::RuntimeCall: std::convert::From<pallet_cf_vaults::Call<state_chain_runtime::Runtime, I>>,
{
	if keygen_participants.contains(&state_chain_client.account_id()) {
		// We initiate keygen outside of the spawn to avoid requesting ceremonies out of order
		let keygen_result_future =
			multisig_client.initiate_keygen(ceremony_id, keygen_participants);
		scope.spawn(async move {
			let _result = state_chain_client
				.submit_signed_extrinsic(
					pallet_cf_vaults::Call::<state_chain_runtime::Runtime, I>::report_keygen_outcome {
						ceremony_id,
						reported_outcome: keygen_result_future
							.await
							.map_err(|(bad_account_ids, _reason)| {
								bad_account_ids
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

async fn handle_signing_request<'a, StateChainClient, MultisigClient, C, I>(
	scope: &Scope<'a, anyhow::Error>,
	multisig_client: &'a MultisigClient,
	state_chain_client: Arc<StateChainClient>,
	ceremony_id: CeremonyId,
	key_id: KeyId,
	signers: BTreeSet<AccountId>,
	payload: C::SigningPayload,
	logger: slog::Logger,
) where
	MultisigClient: MultisigClientApi<C>,
	StateChainClient: ExtrinsicApi + 'static + Send + Sync,
	C: CryptoScheme,
	I: 'static + Sync + Send,
    state_chain_runtime::Runtime: pallet_cf_threshold_signature::Config<I>,
	state_chain_runtime::RuntimeCall: std::convert::From<pallet_cf_threshold_signature::Call<state_chain_runtime::Runtime, I>>,
	<<state_chain_runtime::Runtime as pallet_cf_threshold_signature::Config<I>>::TargetChain as ChainCrypto>::ThresholdSignature: From<C::Signature>,
{
	if signers.contains(&state_chain_client.account_id()) {
		// We initiate signing outside of the spawn to avoid requesting ceremonies out of order
		let signing_result_future =
			multisig_client.initiate_signing(ceremony_id, key_id, signers, payload);
		scope.spawn(async move {
			match signing_result_future.await {
				Ok(signature) => {
					let _result =
						state_chain_client
							.submit_unsigned_extrinsic(
								pallet_cf_threshold_signature::Call::<
									state_chain_runtime::Runtime,
									I,
								>::signature_success {
									ceremony_id,
									signature: signature.into(),
								},
								&logger,
							)
							.await;
				},
				Err((bad_account_ids, _reason)) => {
					let _result =
						state_chain_client
							.submit_signed_extrinsic(
								pallet_cf_threshold_signature::Call::<
									state_chain_runtime::Runtime,
									I,
								>::report_signature_failed {
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

pub async fn start<
	StateChainClient,
	BlockStream,
	EthRpc,
	DotRpc: DotRpcApi + Send + Sync + 'static,
	EthMultisigClient,
	PolkadotMultisigClient,
>(
	state_chain_client: Arc<StateChainClient>,
	sc_block_stream: BlockStream,
	eth_broadcaster: EthBroadcaster<EthRpc>,
	dot_broadcaster: DotBroadcaster<DotRpc>,
	eth_multisig_client: EthMultisigClient,
	dot_multisig_client: PolkadotMultisigClient,
	peer_update_sender: UnboundedSender<PeerUpdate>,
	eth_epoch_start_sender: async_broadcast::Sender<EpochStart<Ethereum>>,
	eth_address_to_monitor_sender: EthAddressToMonitorSender,
	dot_epoch_start_sender: async_broadcast::Sender<EpochStart<Polkadot>>,
	dot_monitor_ingress_sender: tokio::sync::mpsc::UnboundedSender<PolkadotAccountId>,
	dot_monitor_signature_sender: tokio::sync::mpsc::UnboundedSender<[u8; 64]>,
	cfe_settings_update_sender: watch::Sender<CfeSettings>,
	initial_block_hash: H256,
	logger: slog::Logger,
) -> Result<(), anyhow::Error>
where
	BlockStream: Stream<Item = state_chain_runtime::Header> + Send + 'static,
	EthRpc: EthRpcApi + Send + Sync + 'static,
	EthMultisigClient: MultisigClientApi<EthSigning> + Send + Sync + 'static,
	PolkadotMultisigClient: MultisigClientApi<PolkadotSigning> + Send + Sync + 'static,
	StateChainClient: StorageApi + ExtrinsicApi + 'static + Send + Sync,
{
	task_scope(|scope| async {
        let logger = logger.new(o!(COMPONENT_KEY => "SCObserver"));

        let account_id = state_chain_client.account_id();

        let heartbeat_block_interval = {
            use frame_support::traits::TypedGet;
            <state_chain_runtime::Runtime as pallet_cf_reputation::Config>::HeartbeatBlockInterval::get()
        };

        let start_epoch = |block_hash: H256, index: u32, current: bool, participant: bool| {
            let eth_epoch_start_sender = &eth_epoch_start_sender;

            let dot_epoch_start_sender = &dot_epoch_start_sender;
            let state_chain_client = &state_chain_client;

            async move {
                eth_epoch_start_sender.broadcast(EpochStart::<Ethereum> {
                    epoch_index: index,
                    block_number: state_chain_client
                        .storage_map_entry::<pallet_cf_vaults::Vaults<
                            state_chain_runtime::Runtime,
                            state_chain_runtime::EthereumInstance,
                        >>(block_hash, &index)
                        .await
                        .unwrap()
                        .unwrap()
                        .active_from_block,
                    current,
                    participant,
                    data: (),
                }).await.unwrap();

                // It is possible for there not to be a Polkadot vault.
                // At genesis there is no Polkadot vault, so we want to check that the vault exists
                // before we start witnessing.
                if let Some(vault) = state_chain_client
                .storage_map_entry::<pallet_cf_vaults::Vaults<
                    state_chain_runtime::Runtime,
                    state_chain_runtime::PolkadotInstance,
                >>(block_hash, &index)
                .await
                .unwrap() {
                    dot_epoch_start_sender.broadcast(EpochStart::<Polkadot> {
                        epoch_index: index,
                        block_number: vault.active_from_block,
                        current,
                        participant,
                        data: dot::EpochStartData {
                            vault_account: state_chain_client.storage_value::<pallet_cf_environment::PolkadotVaultAccountId<state_chain_runtime::Runtime>>(block_hash).await.unwrap().unwrap()
                        }
                    }).await.unwrap();
                }
            }
        };

        {
            let historical_active_epochs = BTreeSet::from_iter(state_chain_client.storage_map_entry::<pallet_cf_validator::HistoricalActiveEpochs<state_chain_runtime::Runtime>>(
                initial_block_hash,
                &account_id
            ).await.unwrap());

            let current_epoch = state_chain_client
                .storage_value::<pallet_cf_validator::CurrentEpoch<
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

        let mut last_heartbeat_submitted_at = 0;

        // We want to submit a little more frequently than the interval, just in case we submit
        // close to the boundary, and our heartbeat ends up on the wrong side of the interval we're submitting for.
        // The assumption here is that `HEARTBEAT_SAFETY_MARGIN` >> `heartbeat_block_interval`
        const HEARTBEAT_SAFETY_MARGIN: BlockNumber = 10;
        let blocks_per_heartbeat =  heartbeat_block_interval - HEARTBEAT_SAFETY_MARGIN;

        slog::info!(
            logger,
            "Sending heartbeat every {} blocks",
            blocks_per_heartbeat
        );

        let mut sc_block_stream = Box::pin(sc_block_stream);
        loop {
            match sc_block_stream.next().await {
                Some(current_block_header) => {
                    let current_block_hash = current_block_header.hash();
                    slog::debug!(
                        logger,
                        "Processing SC block {} with block hash: {:#x}",
                        current_block_header.number,
                        current_block_hash
                    );

                    match state_chain_client.storage_value::<frame_system::Events::<state_chain_runtime::Runtime>>(current_block_hash).await {
                        Ok(events) => {
                            for event_record in events {
                                match_event! {event_record.event, logger {
                                    state_chain_runtime::RuntimeEvent::Validator(
                                        pallet_cf_validator::Event::NewEpoch(new_epoch),
                                    ) => {
                                        start_epoch(current_block_hash, new_epoch, true, state_chain_client.storage_double_map_entry::<pallet_cf_validator::AuthorityIndex<state_chain_runtime::Runtime>>(
                                            current_block_hash,
                                            &new_epoch,
                                            &account_id
                                        ).await.unwrap().is_some()).await;
                                    }
                                    state_chain_runtime::RuntimeEvent::Validator(
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
                                    state_chain_runtime::RuntimeEvent::Validator(
                                        pallet_cf_validator::Event::PeerIdUnregistered(
                                            account_id,
                                            ed25519_pubkey,
                                        ),
                                    ) => {
                                        peer_update_sender
                                            .send(PeerUpdate::Deregistered(account_id, ed25519_pubkey))
                                            .unwrap();
                                    }
                                    state_chain_runtime::RuntimeEvent::EthereumVault(
                                        pallet_cf_vaults::Event::KeygenRequest(
                                            ceremony_id,
                                            keygen_participants,
                                        ),
                                    ) => {
                                        // Ceremony id tracking is global, so update all other clients
                                        dot_multisig_client.update_latest_ceremony_id(ceremony_id);

                                        handle_keygen_request::<_, _, _, EthereumInstance>(
                                            scope,
                                            &eth_multisig_client,
                                            state_chain_client.clone(),
                                            ceremony_id,
                                            keygen_participants,
                                            logger.clone()
                                        ).await;
                                    }

                                    state_chain_runtime::RuntimeEvent::PolkadotVault(
                                        pallet_cf_vaults::Event::KeygenRequest(
                                            ceremony_id,
                                            keygen_participants,
                                        ),
                                    ) => {
                                        // Ceremony id tracking is global, so update all other clients
                                        eth_multisig_client.update_latest_ceremony_id(ceremony_id);

                                        handle_keygen_request::<_, _, _, PolkadotInstance>(
                                            scope,
                                            &dot_multisig_client,
                                            state_chain_client.clone(),
                                            ceremony_id,
                                            keygen_participants,
                                            logger.clone()
                                        ).await;
                                    }
                                    state_chain_runtime::RuntimeEvent::EthereumThresholdSigner(
                                        pallet_cf_threshold_signature::Event::ThresholdSignatureRequest{
                                            request_id: _,
                                            ceremony_id,
                                            key_id,
                                            signatories,
                                            payload,
                                        },
                                    ) => {
                                        // Ceremony id tracking is global, so update all other clients
                                        dot_multisig_client.update_latest_ceremony_id(ceremony_id);

                                        handle_signing_request::<_, _, _, EthereumInstance>(
                                                scope,
                                                &eth_multisig_client,
                                            state_chain_client.clone(),
                                            ceremony_id,
                                            KeyId(key_id),
                                            signatories,
                                            crate::multisig::eth::SigningPayload(payload.0),
                                            logger.clone(),
                                        ).await;
                                    }

                                    state_chain_runtime::RuntimeEvent::PolkadotThresholdSigner(
                                        pallet_cf_threshold_signature::Event::ThresholdSignatureRequest{
                                            request_id: _,
                                            ceremony_id,
                                            key_id,
                                            signatories,
                                            payload,
                                        },
                                    ) => {
                                        // Ceremony id tracking is global, so update all other clients
                                        eth_multisig_client.update_latest_ceremony_id(ceremony_id);

                                        handle_signing_request::<_, _, PolkadotSigning, PolkadotInstance>(
                                                scope,
                                                &dot_multisig_client,
                                            state_chain_client.clone(),
                                            ceremony_id,
                                            KeyId(key_id),
                                            signatories,
                                            crate::multisig::polkadot::SigningPayload::new(payload.0)
                                                .expect("Payload should be correct size"),
                                            logger.clone(),
                                        ).await;
                                    }
                                    state_chain_runtime::RuntimeEvent::EthereumBroadcaster(
                                        pallet_cf_broadcast::Event::TransactionBroadcastRequest {
                                            broadcast_attempt_id,
                                            nominee,
                                            unsigned_tx,
                                        },
                                    ) if nominee == account_id => {
                                        slog::debug!(
                                            logger,
                                            "Received signing request with broadcast_attempt_id {} for transaction: {:?}",
                                            broadcast_attempt_id,
                                            unsigned_tx,
                                        );
                                        match eth_broadcaster.encode_and_sign_tx(unsigned_tx).await {
                                            Ok(raw_signed_tx) => {
                                                // We want to transmit here to decrease the delay between getting a gas price estimate
                                                // and transmitting it to the Ethereum network
                                                let expected_broadcast_tx_hash = Keccak256::hash(&raw_signed_tx.0[..]);
                                                match eth_broadcaster.send(raw_signed_tx.0).await {
                                                    Ok(tx_hash) => {
                                                        slog::debug!(
                                                            logger,
                                                            "Successful TransmissionRequest broadcast_attempt_id {}, tx_hash: {:#x}",
                                                            broadcast_attempt_id,
                                                            tx_hash
                                                        );
                                                        assert_eq!(
                                                            tx_hash.0, expected_broadcast_tx_hash.0,
                                                            "tx_hash returned from `send` does not match expected hash"
                                                        );
                                                    },
                                                    Err(e) => {
                                                        slog::info!(
                                                            logger,
                                                            "TransmissionRequest broadcast_attempt_id {} failed: {:?}",
                                                            broadcast_attempt_id,
                                                            e
                                                        );
                                                    },
                                                }
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
                                                    state_chain_runtime::RuntimeCall::EthereumBroadcaster(
                                                        pallet_cf_broadcast::Call::transaction_signing_failure {
                                                            broadcast_attempt_id,
                                                        },
                                                    ),
                                                    &logger,
                                                ).await;
                                            }
                                        }
                                    }

                                    state_chain_runtime::RuntimeEvent::PolkadotBroadcaster(
                                        pallet_cf_broadcast::Event::TransactionBroadcastRequest {
                                            broadcast_attempt_id,
                                            nominee,
                                            unsigned_tx,
                                        },
                                    ) => {
                                        // we want to monitor for this new broadcast
                                        let (_api_call, signature) = state_chain_client
                                            .storage_map_entry::<pallet_cf_broadcast::ThresholdSignatureData<state_chain_runtime::Runtime, PolkadotInstance>>(current_block_hash, &broadcast_attempt_id.broadcast_id)
                                            .await
                                            .context(format!("Failed to fetch signature for broadcast_id: {}", broadcast_attempt_id.broadcast_id))?
                                            .expect("If we are broadcasting this tx, the signature must exist");

                                        // get the threhsold signature, and we want the raw bytes inside the signature
                                        dot_monitor_signature_sender.send(signature.0).unwrap();
                                        if nominee == account_id {
                                            let _result = dot_broadcaster.send(unsigned_tx.encoded_extrinsic).await
                                            .map(|_| slog::info!(logger, "Polkadot transmission successful: {broadcast_attempt_id}"))
                                            .map_err(|error| {
                                                slog::error!(logger, "Error: {:?}", error);
                                            });
                                        }
                                    }
                                    state_chain_runtime::RuntimeEvent::Environment(
                                        pallet_cf_environment::Event::CfeSettingsUpdated {
                                            new_cfe_settings
                                        }) => {
                                            cfe_settings_update_sender.send(new_cfe_settings).unwrap();
                                    }

                                    state_chain_runtime::RuntimeEvent::EthereumIngressEgress(
                                        pallet_cf_ingress_egress::Event::StartWitnessing {
                                            ingress_address,
                                            ingress_asset
                                        }
                                    ) => {
                                        use cf_primitives::chains::assets::eth;
                                        match ingress_asset {
                                            eth::Asset::Eth => {
                                                eth_address_to_monitor_sender.eth.send(ingress_address).unwrap();
                                            }
                                            eth::Asset::Flip => {
                                                eth_address_to_monitor_sender.flip.send(ingress_address).unwrap();
                                            }
                                            eth::Asset::Usdc => {
                                                eth_address_to_monitor_sender.usdc.send(ingress_address).unwrap();
                                            }
                                        }
                                    }

                                    state_chain_runtime::RuntimeEvent::PolkadotIngressEgress(
                                        pallet_cf_ingress_egress::Event::StartWitnessing {
                                            ingress_address,
                                            ingress_asset
                                        }
                                    ) => {
                                        assert_eq!(ingress_asset, cf_primitives::chains::assets::dot::Asset::Dot);
                                        dot_monitor_ingress_sender.send(ingress_address).unwrap();
                                    }
                                }}}}
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
                    // We send it every `blocks_per_heartbeat` from the block they started up at.
                    if ((current_block_header.number - last_heartbeat_submitted_at) >= blocks_per_heartbeat
                        // Submitting earlier than one minute in may falsely indicate liveness.
                        ) && has_submitted_init_heartbeat.load(Ordering::Relaxed)
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

                        last_heartbeat_submitted_at = current_block_header.number;
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
