mod crypto_compat;
#[cfg(test)]
mod tests;

use anyhow::{anyhow, Context};
use cf_chains::{
	btc::{self, PreviousOrCurrent, ScriptPubkey},
	dot::{self, PolkadotAccountId, PolkadotSignature},
	eth::Ethereum,
	Bitcoin, Polkadot,
};
use cf_primitives::{BlockNumber, CeremonyId, EpochIndex};
use crypto_compat::CryptoCompat;
use futures::{FutureExt, StreamExt};
use sp_core::{H160, H256};
use sp_runtime::AccountId32;
use state_chain_runtime::{AccountId, BitcoinInstance, EthereumInstance, PolkadotInstance};
use std::{
	collections::BTreeSet,
	sync::{
		atomic::{AtomicBool, Ordering},
		Arc,
	},
	time::Duration,
};
use tokio::sync::mpsc::UnboundedSender;
use tracing::{debug, error, info, info_span, trace, Instrument};

use crate::{
	btc::{rpc::BtcRpcApi, BtcBroadcaster},
	dot::{rpc::DotRpcApi, DotBroadcaster},
	eth::{broadcaster::EthBroadcaster, ethers_rpc::EthersRpcApi},
	p2p::{PeerInfo, PeerUpdate},
	state_chain_observer::client::{
		extrinsic_api::{
			signed::{SignedExtrinsicApi, UntilFinalized},
			unsigned::UnsignedExtrinsicApi,
		},
		storage_api::StorageApi,
		StateChainStreamApi,
	},
	witnesser::{EpochStart, MonitorCommand},
};
use multisig::{
	bitcoin::BtcSigning, client::MultisigClientApi, eth::EthSigning, polkadot::PolkadotSigning,
	CryptoScheme, KeyId, SignatureToThresholdSignature,
};
use utilities::task_scope::{task_scope, Scope};

pub type EthMonitorCommandSender = UnboundedSender<MonitorCommand<H160>>;

pub struct EthAddressToMonitorSender {
	pub eth: EthMonitorCommandSender,
	pub flip: EthMonitorCommandSender,
	pub usdc: EthMonitorCommandSender,
}

async fn handle_keygen_request<'a, StateChainClient, MultisigClient, C, I>(
	scope: &Scope<'a, anyhow::Error>,
	multisig_client: &'a MultisigClient,
	state_chain_client: Arc<StateChainClient>,
	ceremony_id: CeremonyId,
	epoch_index: EpochIndex,
	keygen_participants: BTreeSet<AccountId32>,
) where
	MultisigClient: MultisigClientApi<C>,
	StateChainClient: SignedExtrinsicApi + 'static + Send + Sync,
	state_chain_runtime::Runtime: pallet_cf_vaults::Config<I>,
	C: CryptoScheme<Chain = <state_chain_runtime::Runtime as pallet_cf_vaults::Config<I>>::Chain>
		+ 'static,
	I: CryptoCompat<C, C::Chain> + 'static + Sync + Send,
	state_chain_runtime::RuntimeCall:
		std::convert::From<pallet_cf_vaults::Call<state_chain_runtime::Runtime, I>>,
{
	if keygen_participants.contains(&state_chain_client.account_id()) {
		// We initiate keygen outside of the spawn to avoid requesting ceremonies out of order
		let keygen_result_future =
			multisig_client.initiate_keygen(ceremony_id, epoch_index, keygen_participants);
		scope.spawn(async move {
			state_chain_client
                .submit_signed_extrinsic(pallet_cf_vaults::Call::<
                    state_chain_runtime::Runtime,
                    I,
                >::report_keygen_outcome {
                    ceremony_id,
                    reported_outcome: keygen_result_future
                        .await
                        .map(I::pubkey_to_aggkey)
                        .map_err(|(bad_account_ids, _reason)| bad_account_ids),
                })
                .await;
			Ok(())
		});
	} else {
		// If we are not participating, just send an empty ceremony request (needed for ceremony id
		// tracking)
		multisig_client.update_latest_ceremony_id(ceremony_id);
	}
}

async fn handle_key_handover_request<'a, StateChainClient, MultisigClient>(
	scope: &Scope<'a, anyhow::Error>,
	multisig_client: &'a MultisigClient,
	state_chain_client: Arc<StateChainClient>,
	ceremony_id: CeremonyId,
	from_epoch: EpochIndex,
	to_epoch: EpochIndex,
	sharing_participants: BTreeSet<AccountId32>,
	receiving_participants: BTreeSet<AccountId32>,
	key_to_share: btc::AggKey,
	mut new_key: btc::AggKey,
) where
	MultisigClient: MultisigClientApi<BtcSigning>,
	StateChainClient: SignedExtrinsicApi + 'static + Send + Sync,
	state_chain_runtime::Runtime: pallet_cf_vaults::Config<BitcoinInstance>,
	state_chain_runtime::RuntimeCall:
		std::convert::From<pallet_cf_vaults::Call<state_chain_runtime::Runtime, BitcoinInstance>>,
{
	let account_id = &state_chain_client.account_id();
	if sharing_participants.contains(account_id) || receiving_participants.contains(account_id) {
		let key_handover_result_future = multisig_client.initiate_key_handover(
			ceremony_id,
			KeyId::new(from_epoch, key_to_share.current),
			to_epoch,
			sharing_participants,
			receiving_participants,
		);
		scope.spawn(async move {
			let _result = state_chain_client
				.submit_signed_extrinsic(pallet_cf_vaults::Call::<
					state_chain_runtime::Runtime,
					BitcoinInstance,
				>::report_key_handover_outcome {
					ceremony_id,
					reported_outcome: key_handover_result_future
						.await
						.map(move |handover_key| {
							assert!(new_key.previous.replace(handover_key.serialize()).is_none());
							new_key
						})
						.map_err(|(bad_account_ids, _reason)| bad_account_ids),
				})
				.await;
			Ok(())
		});
	} else {
		multisig_client.update_latest_ceremony_id(ceremony_id);
	}
}

async fn handle_signing_request<'a, StateChainClient, MultisigClient, C, I>(
	scope: &Scope<'a, anyhow::Error>,
	multisig_client: &'a MultisigClient,
	state_chain_client: Arc<StateChainClient>,
	ceremony_id: CeremonyId,
	signers: BTreeSet<AccountId>,
	signing_info: Vec<(KeyId, C::SigningPayload)>,
) where
	MultisigClient: MultisigClientApi<C>,
	StateChainClient: SignedExtrinsicApi + UnsignedExtrinsicApi + 'static + Send + Sync,
	C: CryptoScheme,
	I: 'static + Sync + Send,
	state_chain_runtime::Runtime: pallet_cf_threshold_signature::Config<I>,
	state_chain_runtime::RuntimeCall:
		std::convert::From<pallet_cf_threshold_signature::Call<state_chain_runtime::Runtime, I>>,
	Vec<C::Signature>: SignatureToThresholdSignature<
		<state_chain_runtime::Runtime as pallet_cf_threshold_signature::Config<I>>::TargetChain,
	>,
{
	if signers.contains(&state_chain_client.account_id()) {
		// We initiate signing outside of the spawn to avoid requesting ceremonies out of order
		let signing_result_future =
			multisig_client.initiate_signing(ceremony_id, signers, signing_info);

		scope.spawn(async move {
			match signing_result_future.await {
				Ok(signatures) => {
					state_chain_client
						.submit_unsigned_extrinsic(pallet_cf_threshold_signature::Call::<
							state_chain_runtime::Runtime,
							I,
						>::signature_success {
							ceremony_id,
							signature: signatures.to_threshold_signature(),
						})
						.await;
				},
				Err((bad_account_ids, _reason)) => {
					state_chain_client
						.submit_signed_extrinsic(pallet_cf_threshold_signature::Call::<
							state_chain_runtime::Runtime,
							I,
						>::report_signature_failed {
							ceremony_id,
							offenders: BTreeSet::from_iter(bad_account_ids),
						})
						.await;
				},
			}
			Ok(())
		});
	} else {
		multisig_client.update_latest_ceremony_id(ceremony_id);
	}
}

// Wrap the match so we add a log message before executing the processing of the event
// if we are processing. Else, ignore it.
macro_rules! match_event {
    ($event:expr, { $($(#[$cfg_param:meta])? $bind:pat $(if $condition:expr)? => $block:expr)+ }) => {{
        let event = $event;
        let formatted_event = format!("{:?}", event);
        match event {
            $(
                $(#[$cfg_param])?
                $bind => {
                    $(if !$condition {
                        trace!("Ignoring event {formatted_event}");
                    } else )? {
                        debug!("Handling event {formatted_event}");
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
	DotRpc,
	BtcRpc,
	EthMultisigClient,
	PolkadotMultisigClient,
	BitcoinMultisigClient,
>(
	state_chain_client: Arc<StateChainClient>,
	sc_block_stream: BlockStream,
	eth_broadcaster: EthBroadcaster<EthRpc>,
	dot_broadcaster: DotBroadcaster<DotRpc>,
	btc_broadcaster: BtcBroadcaster<BtcRpc>,
	eth_multisig_client: EthMultisigClient,
	dot_multisig_client: PolkadotMultisigClient,
	btc_multisig_client: BitcoinMultisigClient,
	peer_update_sender: UnboundedSender<PeerUpdate>,
	eth_epoch_start_sender: async_broadcast::Sender<EpochStart<Ethereum>>,
	eth_address_to_monitor_sender: EthAddressToMonitorSender,
	dot_epoch_start_sender: async_broadcast::Sender<EpochStart<Polkadot>>,
	dot_monitor_command_sender: tokio::sync::mpsc::UnboundedSender<
		MonitorCommand<PolkadotAccountId>,
	>,
	dot_monitor_signature_sender: tokio::sync::mpsc::UnboundedSender<
		MonitorCommand<PolkadotSignature>,
	>,
	btc_epoch_start_sender: async_broadcast::Sender<EpochStart<Bitcoin>>,
	btc_monitor_command_sender: tokio::sync::mpsc::UnboundedSender<MonitorCommand<ScriptPubkey>>,
	btc_tx_hash_sender: tokio::sync::mpsc::UnboundedSender<MonitorCommand<[u8; 32]>>,
) -> Result<(), anyhow::Error>
where
	BlockStream: StateChainStreamApi,
	EthRpc: EthersRpcApi + Send + Sync + 'static,
	DotRpc: DotRpcApi + Send + Sync + 'static,
	BtcRpc: BtcRpcApi + Send + Sync + 'static,
	EthMultisigClient: MultisigClientApi<EthSigning> + Send + Sync + 'static,
	PolkadotMultisigClient: MultisigClientApi<PolkadotSigning> + Send + Sync + 'static,
	BitcoinMultisigClient: MultisigClientApi<BtcSigning> + Send + Sync + 'static,
	StateChainClient:
		StorageApi + UnsignedExtrinsicApi + SignedExtrinsicApi + 'static + Send + Sync,
{
	task_scope(|scope| async {
        let account_id = state_chain_client.account_id();

        let heartbeat_block_interval = {
            use frame_support::traits::TypedGet;
            <state_chain_runtime::Runtime as pallet_cf_reputation::Config>::HeartbeatBlockInterval::get()
        };

        let start_epoch = |block_hash: H256, index: u32, current: bool, participant: bool| {
            let eth_epoch_start_sender = &eth_epoch_start_sender;
            let dot_epoch_start_sender = &dot_epoch_start_sender;
            let btc_epoch_start_sender = &btc_epoch_start_sender;
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

                // It is possible for there not to be a Bitcoin vault.
                // At genesis there is no Bitcoin vault, so we want to check that the vault exists
                // before we start witnessing.
                if let Some(vault) = state_chain_client
                .storage_map_entry::<pallet_cf_vaults::Vaults<
                    state_chain_runtime::Runtime,
                    BitcoinInstance,
                >>(block_hash, &index)
                .await
                .unwrap() {

                    let change_pubkey = vault.public_key;

                    btc_epoch_start_sender.broadcast(EpochStart::<Bitcoin> {
                        epoch_index: index,
                        block_number: vault.active_from_block,
                        current,
                        participant,
                        data: btc::EpochStartData {
                            change_pubkey
                        },
                    }).await.unwrap();
                }
            }
        };

        {
            let historical_active_epochs = BTreeSet::from_iter(state_chain_client.storage_map_entry::<pallet_cf_validator::HistoricalActiveEpochs<state_chain_runtime::Runtime>>(
                sc_block_stream.cache().block_hash,
                &account_id
            ).await.unwrap());

            let current_epoch = state_chain_client
                .storage_value::<pallet_cf_validator::CurrentEpoch<
                    state_chain_runtime::Runtime,
                >>(sc_block_stream.cache().block_hash)
                .await
                .unwrap();

            if let Some(earliest_historical_active_epoch) = historical_active_epochs.iter().next() {
                for epoch in *earliest_historical_active_epoch..current_epoch {
                    start_epoch(sc_block_stream.cache().block_hash, epoch, false, historical_active_epochs.contains(&epoch)).await;
                }
            }

            start_epoch(sc_block_stream.cache().block_hash, current_epoch, true, historical_active_epochs.contains(&current_epoch)).await;
        }

        // Ensure we don't submit initial heartbeat too early. Early heartbeats could falsely indicate
        // liveness
        let has_submitted_init_heartbeat = Arc::new(AtomicBool::new(false));
        scope.spawn({
            let state_chain_client = state_chain_client.clone();
            let has_submitted_init_heartbeat = has_submitted_init_heartbeat.clone();
            async move {
                tokio::time::sleep(Duration::from_secs(60)).await;
                state_chain_client
                    .submit_signed_extrinsic(
                        pallet_cf_reputation::Call::heartbeat {},
                    )
                    .await
                    .until_finalized()
                    .await
                    .context("Failed to submit initial heartbeat")?;
                has_submitted_init_heartbeat.store(true, Ordering::Relaxed);
            Ok(())
            }.boxed()
        });

        let mut last_heartbeat_submitted_at = 0;

        // We want to submit a little more frequently than the interval, just in case we submit
        // close to the boundary, and our heartbeat ends up on the wrong side of the interval we're submitting for.
        // The assumption here is that `HEARTBEAT_SAFETY_MARGIN` >> `heartbeat_block_interval`
        const HEARTBEAT_SAFETY_MARGIN: BlockNumber = 10;
        let blocks_per_heartbeat =  heartbeat_block_interval - HEARTBEAT_SAFETY_MARGIN;

        info!("Sending heartbeat every {blocks_per_heartbeat} blocks");

        let mut sc_block_stream = Box::pin(sc_block_stream);
        loop {
            match sc_block_stream.next().await {
                Some((current_block_hash, current_block_header)) => {
                    debug!("Processing SC block {} with block hash: {current_block_hash:#x}", current_block_header.number);

                    match state_chain_client.storage_value::<frame_system::Events::<state_chain_runtime::Runtime>>(current_block_hash).await {
                        Ok(events) => {
                            for event_record in events {
                                match_event! {event_record.event, {
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
                                        pallet_cf_vaults::Event::KeygenRequest {
                                            ceremony_id,
                                            participants,
                                            epoch_index
                                        }
                                    ) => {
                                        handle_keygen_request::<_, _, _, EthereumInstance>(
                                            scope,
                                            &eth_multisig_client,
                                            state_chain_client.clone(),
                                            ceremony_id,
                                            epoch_index,
                                            participants,
                                        ).await;
                                    }
                                    state_chain_runtime::RuntimeEvent::PolkadotVault(
                                        pallet_cf_vaults::Event::KeygenRequest {
                                            ceremony_id,
                                            participants,
                                            epoch_index
                                        }
                                    ) => {
                                        handle_keygen_request::<_, _, _, PolkadotInstance>(
                                            scope,
                                            &dot_multisig_client,
                                            state_chain_client.clone(),
                                            ceremony_id,
                                            epoch_index,
                                            participants,
                                        ).await;
                                    }
                                    state_chain_runtime::RuntimeEvent::BitcoinVault(
                                        pallet_cf_vaults::Event::KeygenRequest {
                                            ceremony_id,
                                            participants,
                                            epoch_index
                                        }
                                    ) => {
                                        handle_keygen_request::<_, _, _, BitcoinInstance>(
                                            scope,
                                            &btc_multisig_client,
                                            state_chain_client.clone(),
                                            ceremony_id,
                                            epoch_index,
                                            participants,
                                        ).await;
                                    }
                                    state_chain_runtime::RuntimeEvent::EthereumThresholdSigner(
                                        pallet_cf_threshold_signature::Event::ThresholdSignatureRequest{
                                            request_id: _,
                                            ceremony_id,
                                            epoch,
                                            key,
                                            signatories,
                                            payload,
                                        },
                                    ) => {
                                        handle_signing_request::<_, _, _, EthereumInstance>(
                                                scope,
                                                &eth_multisig_client,
                                            state_chain_client.clone(),
                                            ceremony_id,
                                            signatories,
                                            vec![(
                                                KeyId::new(epoch, key),
                                                multisig::eth::SigningPayload(payload.0)
                                            )],
                                        ).await;
                                    }

                                    state_chain_runtime::RuntimeEvent::PolkadotThresholdSigner(
                                        pallet_cf_threshold_signature::Event::ThresholdSignatureRequest{
                                            request_id: _,
                                            ceremony_id,
                                            epoch,
                                            key,
                                            signatories,
                                            payload,
                                        },
                                    ) => {
                                        handle_signing_request::<_, _, PolkadotSigning, PolkadotInstance>(
                                                scope,
                                                &dot_multisig_client,
                                            state_chain_client.clone(),
                                            ceremony_id,
                                            signatories,
                                            vec![(
                                                KeyId::new(epoch, key),
                                                multisig::polkadot::SigningPayload::new(payload.0)
                                                    .expect("Payload should be correct size")
                                            )],
                                        ).await;
                                    }
                                    state_chain_runtime::RuntimeEvent::BitcoinThresholdSigner(
                                        pallet_cf_threshold_signature::Event::ThresholdSignatureRequest{
                                            request_id: _,
                                            ceremony_id,
                                            epoch,
                                            key,
                                            signatories,
                                            payload: payloads,
                                        },
                                    ) => {
                                        let signing_info = payloads.into_iter().map(|(previous_or_current, payload)| {
                                                (
                                                    KeyId::new(
                                                        epoch,
                                                        match previous_or_current {
                                                            PreviousOrCurrent::Current => key.current,
                                                            PreviousOrCurrent::Previous => key.previous
                                                                .expect("Cannot be asked to sign with previous key if none exists."),
                                                        },
                                                    ),
                                                    multisig::bitcoin::SigningPayload(payload),
                                                )
                                            })
                                            .collect::<Vec<_>>();

                                        handle_signing_request::<_, _, _, BitcoinInstance>(
                                                scope,
                                                &btc_multisig_client,
                                            state_chain_client.clone(),
                                            ceremony_id,
                                            signatories,
                                            signing_info,
                                        ).await;
                                    }
                                    // ======= KEY HANDOVER =======
                                    state_chain_runtime::RuntimeEvent::BitcoinVault(
                                        pallet_cf_vaults::Event::KeyHandoverRequest {
                                           ceremony_id,
                                           key_to_share,
                                           from_epoch,
                                           sharing_participants,
                                           receiving_participants,
                                           new_key,
                                           to_epoch,
                                        },
                                    ) => {
                                        handle_key_handover_request::<_, _>(
                                            scope,
                                            &btc_multisig_client,
                                            state_chain_client.clone(),
                                            ceremony_id,
                                            from_epoch,
                                            to_epoch,
                                            sharing_participants,
                                            receiving_participants,
                                            key_to_share,
                                            new_key,
                                        ).await;
                                    }
                                    state_chain_runtime::RuntimeEvent::EthereumVault(
                                        pallet_cf_vaults::Event::KeyHandoverRequest {
                                           ..
                                        },
                                    ) => {
                                        panic!("There should be no key handover requests made for Ethereum")
                                    }
                                    state_chain_runtime::RuntimeEvent::PolkadotVault(
                                        pallet_cf_vaults::Event::KeyHandoverRequest {
                                           ..
                                        },
                                    ) => {
                                        panic!("There should be no key handover requests made for Polkadot")
                                    }

                                    state_chain_runtime::RuntimeEvent::EthereumBroadcaster(
                                        pallet_cf_broadcast::Event::TransactionBroadcastRequest {
                                            broadcast_attempt_id,
                                            nominee,
                                            transaction_payload,
                                            // We're already witnessing this since we witness the KeyManager for SignatureAccepted events.
                                            transaction_out_id: _,
                                        },
                                    ) if nominee == account_id => {
                                        match eth_broadcaster.send(transaction_payload).await {
                                            Ok(tx_hash) => info!("Ethereum TransactionBroadcastRequest {broadcast_attempt_id:?} success: tx_hash: {tx_hash:#x}"),
                                            Err(error) => {
                                                // Note: this error can indicate that we failed to estimate gas, or that there is
                                                // a problem with the ethereum rpc node, or with the configured account. For example
                                                // if the account balance is too low to pay for required gas.
                                                error!("Error on Ethereum TransactionBroadcastRequest {broadcast_attempt_id:?}: {error:?}");
                                                state_chain_client.submit_signed_extrinsic(
                                                    state_chain_runtime::RuntimeCall::EthereumBroadcaster(
                                                        pallet_cf_broadcast::Call::transaction_signing_failure {
                                                            broadcast_attempt_id,
                                                        },
                                                    ),
                                                )
                                                .await;
                                            }
                                        }
                                    }
                                    state_chain_runtime::RuntimeEvent::PolkadotBroadcaster(
                                        pallet_cf_broadcast::Event::TransactionBroadcastRequest {
                                            broadcast_attempt_id,
                                            nominee,
                                            transaction_payload,
                                            transaction_out_id,
                                        },
                                    ) => {
                                        dot_monitor_signature_sender.send(MonitorCommand::Add(transaction_out_id)).unwrap();
                                        if nominee == account_id {
                                            match dot_broadcaster.send(transaction_payload.encoded_extrinsic).await {
                                                Ok(tx_hash) => info!("Polkadot TransactionBroadcastRequest {broadcast_attempt_id:?} success: tx_hash: {tx_hash:#x}"),
                                                Err(error) => {
                                                    error!("Error on Polkadot TransactionBroadcastRequest {broadcast_attempt_id:?}: {error:?}");
                                                    state_chain_client.submit_signed_extrinsic(
                                                        state_chain_runtime::RuntimeCall::PolkadotBroadcaster(
                                                            pallet_cf_broadcast::Call::transaction_signing_failure {
                                                                broadcast_attempt_id,
                                                            },
                                                        ),
                                                    )
                                                    .await;
                                                }
                                            }
                                        }
                                    }
                                    state_chain_runtime::RuntimeEvent::BitcoinBroadcaster(
                                        pallet_cf_broadcast::Event::TransactionBroadcastRequest {
                                            broadcast_attempt_id,
                                            nominee,
                                            transaction_payload,
                                            transaction_out_id,
                                        },
                                    ) => {
                                        btc_tx_hash_sender.send(MonitorCommand::Add(transaction_out_id)).unwrap();

                                        if nominee == account_id {
                                            match btc_broadcaster.send(transaction_payload.encoded_transaction).await {
                                                Ok(tx_hash) => info!("Bitcoin TransactionBroadcastRequest {broadcast_attempt_id:?} success: tx_hash: {tx_hash:#x}"),
                                                Err(error) => {
                                                    error!("Error on Bitcoin TransactionBroadcastRequest {broadcast_attempt_id:?}: {error:?}");
                                                    state_chain_client.submit_signed_extrinsic(
                                                        state_chain_runtime::RuntimeCall::BitcoinBroadcaster(
                                                            pallet_cf_broadcast::Call::transaction_signing_failure {
                                                                broadcast_attempt_id,
                                                            },
                                                        ),
                                                    )
                                                    .await;
                                                }
                                            }
                                        }
                                    }
                                    state_chain_runtime::RuntimeEvent::BitcoinBroadcaster(
                                        pallet_cf_broadcast::Event::BroadcastSuccess { broadcast_id: _, transaction_out_id }
                                    ) => {
                                        btc_tx_hash_sender.send(MonitorCommand::Remove(transaction_out_id)).unwrap();
                                    }
                                    state_chain_runtime::RuntimeEvent::EthereumIngressEgress(
                                        pallet_cf_ingress_egress::Event::StartWitnessing {
                                            deposit_address,
                                            source_asset,
                                            ..
                                        }
                                    ) => {
                                        use cf_primitives::chains::assets::eth;
                                        match source_asset {
                                            eth::Asset::Eth => {
                                                &eth_address_to_monitor_sender.eth
                                            }
                                            eth::Asset::Flip => {
                                                &eth_address_to_monitor_sender.flip
                                            }
                                            eth::Asset::Usdc => {
                                                &eth_address_to_monitor_sender.usdc
                                            }
                                        }.send(MonitorCommand::Add(deposit_address)).unwrap();
                                    }
                                    state_chain_runtime::RuntimeEvent::EthereumIngressEgress(
                                        pallet_cf_ingress_egress::Event::StopWitnessing {
                                            deposit_address,
                                            source_asset
                                        }
                                    ) => {
                                        use cf_primitives::chains::assets::eth;
                                        match source_asset {
                                            eth::Asset::Eth => {
                                                &eth_address_to_monitor_sender.eth
                                            }
                                            eth::Asset::Flip => {
                                                &eth_address_to_monitor_sender.flip
                                            }
                                            eth::Asset::Usdc => {
                                                &eth_address_to_monitor_sender.usdc
                                            }
                                        }.send(MonitorCommand::Remove(deposit_address)).unwrap();
                                    }
                                    state_chain_runtime::RuntimeEvent::PolkadotIngressEgress(
                                        pallet_cf_ingress_egress::Event::StartWitnessing {
                                            deposit_address,
                                            source_asset,
                                            ..
                                        }
                                    ) => {
                                        assert_eq!(source_asset, cf_primitives::chains::assets::dot::Asset::Dot);
                                        dot_monitor_command_sender.send(MonitorCommand::Add(deposit_address)).unwrap();
                                    }
                                    state_chain_runtime::RuntimeEvent::PolkadotIngressEgress(
                                        pallet_cf_ingress_egress::Event::StopWitnessing {
                                            deposit_address,
                                            source_asset
                                        }
                                    ) => {
                                        assert_eq!(source_asset, cf_primitives::chains::assets::dot::Asset::Dot);
                                        dot_monitor_command_sender.send(MonitorCommand::Remove(deposit_address)).unwrap();
                                    }
                                    state_chain_runtime::RuntimeEvent::BitcoinIngressEgress(
                                        pallet_cf_ingress_egress::Event::StartWitnessing {
                                            deposit_address,
                                            source_asset,
                                            ..
                                        }
                                    ) => {
                                        assert_eq!(source_asset, cf_primitives::chains::assets::btc::Asset::Btc);
                                        btc_monitor_command_sender.send(MonitorCommand::Add(deposit_address)).unwrap();
                                    }
                                    state_chain_runtime::RuntimeEvent::BitcoinIngressEgress(
                                        pallet_cf_ingress_egress::Event::StopWitnessing {
                                            deposit_address,
                                            source_asset
                                        }
                                    ) => {
                                        assert_eq!(source_asset, cf_primitives::chains::assets::btc::Asset::Btc);
                                        btc_monitor_command_sender.send(MonitorCommand::Remove(deposit_address)).unwrap();
                                    }
                                }}}}
                                Err(error) => {
                                    error!("Failed to decode events at block {}. {error}", current_block_header.number);
                        }
                    }

                    // All nodes must send a heartbeat regardless of their validator status (at least for now).
                    // We send it every `blocks_per_heartbeat` from the block they started up at.
                    if ((current_block_header.number - last_heartbeat_submitted_at) >= blocks_per_heartbeat
                        // Submitting earlier than one minute in may falsely indicate liveness.
                        ) && has_submitted_init_heartbeat.load(Ordering::Relaxed)
                    {
                        info!("Sending heartbeat at block: {}", current_block_header.number);
                        state_chain_client
                            .submit_signed_extrinsic(
                                pallet_cf_reputation::Call::heartbeat {},
                            )
                            .await;

                        last_heartbeat_submitted_at = current_block_header.number;
                    }
                }
                None => {
                    error!("Exiting as State Chain block stream ended");
                    break;
                }
            }
        }
        Err(anyhow!("State Chain block stream ended"))
    }.instrument(info_span!("SCObserver")).boxed()).await
}
