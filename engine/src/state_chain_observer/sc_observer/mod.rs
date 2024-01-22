mod crypto_compat;
#[cfg(test)]
mod tests;

use anyhow::{anyhow, Context};
use cf_chains::{
	btc::{self, PreviousOrCurrent},
	Chain,
};
use cf_primitives::{BlockNumber, CeremonyId, EpochIndex};
use crypto_compat::CryptoCompat;
use futures::{FutureExt, StreamExt};
use pallet_cf_cfe_interface::{ThresholdSignatureRequest, TxBroadcastRequest};

type CfeEvent = pallet_cf_cfe_interface::CfeEvent<Runtime>;

use sp_runtime::AccountId32;
use state_chain_runtime::{
	AccountId, BitcoinInstance, EthereumInstance, PolkadotInstance, Runtime, RuntimeCall,
};
use std::{
	collections::BTreeSet,
	sync::{
		atomic::{AtomicBool, Ordering},
		Arc,
	},
	time::Duration,
};
use tokio::sync::mpsc::UnboundedSender;
use tracing::{debug, error, info, info_span, Instrument};

use crate::{
	btc::retry_rpc::BtcRetryRpcApi,
	dot::retry_rpc::DotRetryRpcApi,
	eth::retry_rpc::EthersRetrySigningRpcApi,
	p2p::{PeerInfo, PeerUpdate},
	state_chain_observer::client::{
		extrinsic_api::{
			signed::{SignedExtrinsicApi, UntilFinalized},
			unsigned::UnsignedExtrinsicApi,
		},
		storage_api::StorageApi,
		stream_api::{StreamApi, FINALIZED},
	},
};
use multisig::{
	bitcoin::BtcCryptoScheme, client::MultisigClientApi, eth::EvmCryptoScheme,
	polkadot::PolkadotCryptoScheme, ChainSigning, CryptoScheme, KeyId,
	SignatureToThresholdSignature,
};
use utilities::task_scope::{task_scope, Scope};

use super::client::chain_api::ChainApi;

async fn handle_keygen_request<'a, StateChainClient, MultisigClient, C, I>(
	scope: &Scope<'a, anyhow::Error>,
	multisig_client: &'a MultisigClient,
	state_chain_client: Arc<StateChainClient>,
	ceremony_id: CeremonyId,
	epoch_index: EpochIndex,
	keygen_participants: BTreeSet<AccountId32>,
) where
	MultisigClient: MultisigClientApi<C::CryptoScheme>,
	StateChainClient: SignedExtrinsicApi + 'static + Send + Sync,
	Runtime: pallet_cf_vaults::Config<I>,
	C: ChainSigning<
			ChainCrypto = <<Runtime as pallet_cf_vaults::Config<I>>::Chain as Chain>::ChainCrypto,
		> + 'static,
	I: CryptoCompat<C, C::ChainCrypto> + 'static + Sync + Send,
	RuntimeCall: From<pallet_cf_vaults::Call<Runtime, I>>,
{
	if keygen_participants.contains(&state_chain_client.account_id()) {
		// We initiate keygen outside of the spawn to avoid requesting ceremonies out of order
		let keygen_result_future =
			multisig_client.initiate_keygen(ceremony_id, epoch_index, keygen_participants);
		scope.spawn(async move {
			state_chain_client
				.finalize_signed_extrinsic(
					pallet_cf_vaults::Call::<Runtime, I>::report_keygen_outcome {
						ceremony_id,
						reported_outcome: keygen_result_future
							.await
							.map(I::pubkey_to_aggkey)
							.map_err(|(bad_account_ids, _reason)| bad_account_ids),
					},
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
	MultisigClient: MultisigClientApi<BtcCryptoScheme>,
	StateChainClient: SignedExtrinsicApi + 'static + Send + Sync,
	Runtime: pallet_cf_vaults::Config<BitcoinInstance>,
	RuntimeCall: From<pallet_cf_vaults::Call<Runtime, BitcoinInstance>>,
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
				.finalize_signed_extrinsic(pallet_cf_vaults::Call::<
					Runtime,
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
	Runtime: pallet_cf_threshold_signature::Config<I>,
	RuntimeCall: From<pallet_cf_threshold_signature::Call<Runtime, I>>,
	Vec<C::Signature>: SignatureToThresholdSignature<
		<Runtime as pallet_cf_threshold_signature::Config<I>>::TargetChainCrypto,
	>,
{
	if signers.contains(&state_chain_client.account_id()) {
		// We initiate signing outside of the spawn to avoid requesting ceremonies out of order
		let signing_result_future =
			multisig_client.initiate_signing(ceremony_id, signers, signing_info);

		scope.spawn(async move {
			match signing_result_future.await {
				Ok(signatures) => {
					let _result = state_chain_client
						.submit_unsigned_extrinsic(pallet_cf_threshold_signature::Call::<
							Runtime,
							I,
						>::signature_success {
							ceremony_id,
							signature: signatures.to_threshold_signature(),
						})
						.await;
				},
				Err((bad_account_ids, _reason)) => {
					state_chain_client
						.finalize_signed_extrinsic(pallet_cf_threshold_signature::Call::<
							Runtime,
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
	eth_rpc: EthRpc,
	dot_rpc: DotRpc,
	btc_rpc: BtcRpc,
	eth_multisig_client: EthMultisigClient,
	dot_multisig_client: PolkadotMultisigClient,
	btc_multisig_client: BitcoinMultisigClient,
	peer_update_sender: UnboundedSender<PeerUpdate>,
) -> Result<(), anyhow::Error>
where
	BlockStream: StreamApi<FINALIZED>,
	EthRpc: EthersRetrySigningRpcApi + Send + Sync + 'static,
	DotRpc: DotRetryRpcApi + Send + Sync + 'static,
	BtcRpc: BtcRetryRpcApi + Send + Sync + 'static,
	EthMultisigClient: MultisigClientApi<EvmCryptoScheme> + Send + Sync + 'static,
	PolkadotMultisigClient: MultisigClientApi<PolkadotCryptoScheme> + Send + Sync + 'static,
	BitcoinMultisigClient: MultisigClientApi<BtcCryptoScheme> + Send + Sync + 'static,
	StateChainClient:
		StorageApi + ChainApi + UnsignedExtrinsicApi + SignedExtrinsicApi + 'static + Send + Sync,
{
	task_scope(|scope| async {
        let account_id = state_chain_client.account_id();

        let heartbeat_block_interval = {
            use frame_support::traits::TypedGet;
            <Runtime as pallet_cf_reputation::Config>::HeartbeatBlockInterval::get()
        };

        // Ensure we don't submit initial heartbeat too early. Early heartbeats could falsely indicate
        // liveness
        let has_submitted_init_heartbeat = Arc::new(AtomicBool::new(false));
        scope.spawn({
            let state_chain_client = state_chain_client.clone();
            let has_submitted_init_heartbeat = has_submitted_init_heartbeat.clone();
            async move {
                tokio::time::sleep(Duration::from_secs(60)).await;
                state_chain_client
                    .finalize_signed_extrinsic(
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
                Some(current_block) => {
                    debug!("Processing SC block {} with block hash: {:#x}", current_block.number, current_block.hash);

                    match state_chain_client
                        .storage_value::<pallet_cf_cfe_interface::CfeEvents<Runtime>>(
                            current_block.hash,
                        )
                        .await {

                        Ok(events) => {
                            for event in events {
                                match_event! {event, {
                                    CfeEvent::EthThresholdSignatureRequest(req) => {
                                        handle_signing_request::<_, _, _, EthereumInstance>(
                                        scope,
                                        &eth_multisig_client,
                                        state_chain_client.clone(),
                                        req.ceremony_id,
                                        req.signatories,
                                        vec![(
                                            KeyId::new(req.epoch_index, req.key),
                                            multisig::eth::SigningPayload(req.payload.0)
                                        )],
                                        ).await;
                                    }
                                    CfeEvent::DotThresholdSignatureRequest(req) => {

                                        handle_signing_request::<_, _, _, PolkadotInstance>(
                                            scope,
                                            &dot_multisig_client,
                                            state_chain_client.clone(),
                                            req.ceremony_id,
                                            req.signatories,
                                            vec![(
                                                KeyId::new(req.epoch_index, req.key),
                                                multisig::polkadot::SigningPayload::new(req.payload.0)
                                                    .expect("Payload should be correct size")
                                            )],
                                        ).await;

                                    }
                                    CfeEvent::BtcThresholdSignatureRequest(ThresholdSignatureRequest::<Runtime, _> { ceremony_id, epoch_index, key, signatories, payload : payloads }) => {


                                        if payloads.len() > multisig::MAX_BTC_SIGNING_PAYLOADS {
                                            error!(ceremony_id = ceremony_id, "Too many payloads, ignoring Bitcoin signing request ({}/{})", payloads.len(), multisig::MAX_BTC_SIGNING_PAYLOADS);
                                            btc_multisig_client.update_latest_ceremony_id(ceremony_id);
                                        } else {
                                            let signing_info = payloads.into_iter().map(|(previous_or_current, payload)| {
                                                    (
                                                        KeyId::new(
                                                            epoch_index,
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
                                    }
                                    CfeEvent::EthKeygenRequest(req) => {
                                        handle_keygen_request::<_, _, _, EthereumInstance>(
                                            scope,
                                            &eth_multisig_client,
                                            state_chain_client.clone(),
                                            req.ceremony_id,
                                            req.epoch_index,
                                            req.participants,
                                        ).await;
                                    }
                                    CfeEvent::BtcKeygenRequest(req) => {
                                        handle_keygen_request::<_, _, _, BitcoinInstance>(
                                            scope,
                                            &btc_multisig_client,
                                            state_chain_client.clone(),
                                            req.ceremony_id,
                                            req.epoch_index,
                                            req.participants,
                                        ).await;
                                    }
                                    CfeEvent::DotKeygenRequest(req) => {
                                        handle_keygen_request::<_, _, _, PolkadotInstance>(
                                            scope,
                                            &dot_multisig_client,
                                            state_chain_client.clone(),
                                            req.ceremony_id,
                                            req.epoch_index,
                                            req.participants,
                                        ).await;
                                    }
                                    CfeEvent::BtcKeyHandoverRequest(req) => {

                                        handle_key_handover_request::<_, _>(
                                            scope,
                                            &btc_multisig_client,
                                            state_chain_client.clone(),
                                            req.ceremony_id,
                                            req.from_epoch,
                                            req.to_epoch,
                                            req.sharing_participants,
                                            req.receiving_participants,
                                            req.key_to_share,
                                            req.new_key,
                                        ).await;
                                    }
                                    CfeEvent::BtcTxBroadcastRequest(TxBroadcastRequest::<Runtime, _> { broadcast_id, nominee, payload }) => {
                                        if nominee == account_id {
                                            let btc_rpc = btc_rpc.clone();
                                            let state_chain_client = state_chain_client.clone();
                                            scope.spawn(async move {
                                                match btc_rpc.send_raw_transaction(payload.encoded_transaction).await {
                                                    Ok(tx_hash) => info!("Bitcoin TransactionBroadcastRequest {broadcast_id:?} success: tx_hash: {tx_hash:#x}"),
                                                    Err(error) => {
                                                        error!("Error on Bitcoin TransactionBroadcastRequest {broadcast_id:?}: {error:?}");
                                                        state_chain_client.finalize_signed_extrinsic(
                                                            RuntimeCall::BitcoinBroadcaster(
                                                                pallet_cf_broadcast::Call::transaction_failed {
                                                                    broadcast_id,
                                                                },
                                                            ),
                                                        )
                                                        .await;
                                                    }
                                                }
                                                Ok(())
                                            });
                                        }
                                    }
                                    CfeEvent::DotTxBroadcastRequest(TxBroadcastRequest::<Runtime, _> { broadcast_id, nominee, payload }) => {
                                        if nominee == account_id {
                                            let dot_rpc = dot_rpc.clone();
                                            let state_chain_client = state_chain_client.clone();
                                            scope.spawn(async move {
                                                match dot_rpc.submit_raw_encoded_extrinsic(payload.encoded_extrinsic).await {
                                                    Ok(tx_hash) => info!("Polkadot TransactionBroadcastRequest {broadcast_id:?} success: tx_hash: {tx_hash:#x}"),
                                                    Err(error) => {
                                                        error!("Error on Polkadot TransactionBroadcastRequest {broadcast_id:?}: {error:?}");
                                                        state_chain_client.finalize_signed_extrinsic(
                                                            RuntimeCall::PolkadotBroadcaster(
                                                                pallet_cf_broadcast::Call::transaction_failed {
                                                                    broadcast_id,
                                                                },
                                                            ),
                                                        )
                                                        .await;
                                                    }
                                                }
                                                Ok(())
                                            });
                                        }
                                    }
                                    CfeEvent::EthTxBroadcastRequest(TxBroadcastRequest::<Runtime, _> { broadcast_id, nominee, payload }) => {
                                        if nominee == account_id {
                                            let eth_rpc = eth_rpc.clone();
                                            let state_chain_client = state_chain_client.clone();
                                            scope.spawn(async move {
                                                match eth_rpc.broadcast_transaction(payload).await {
                                                    Ok(tx_hash) => info!("Ethereum TransactionBroadcastRequest {broadcast_id:?} success: tx_hash: {tx_hash:#x}"),
                                                    Err(error) => {
                                                        // Note: this error can indicate that we failed to estimate gas, or that there is
                                                        // a problem with the ethereum rpc node, or with the configured account. For example
                                                        // if the account balance is too low to pay for required gas.
                                                        error!("Error on Ethereum TransactionBroadcastRequest {broadcast_id:?}: {error:?}");
                                                        state_chain_client.finalize_signed_extrinsic(
                                                            RuntimeCall::EthereumBroadcaster(
                                                                pallet_cf_broadcast::Call::transaction_failed {
                                                                    broadcast_id,
                                                                },
                                                            ),
                                                        )
                                                        .await;
                                                    }
                                                }
                                                Ok(())
                                            })
                                        }
                                    }
                                    CfeEvent::PeerIdRegistered { account_id, pubkey, port, ip } => {
                                        peer_update_sender
                                            .send(PeerUpdate::Registered(
                                                    PeerInfo::new(account_id, pubkey, ip.into(), port)
                                                )
                                            )
                                            .unwrap();
                                    }
                                    CfeEvent::PeerIdDeregistered { account_id, pubkey } => {
                                        peer_update_sender
                                            .send(PeerUpdate::Deregistered(account_id, pubkey))
                                            .unwrap();
                                    }
                                }}
                            }
                        }
                        Err(error) => {
                            error!("Failed to decode events at block {}. {error}", current_block.number);
                        }
                    }

                    // All nodes must send a heartbeat regardless of their validator status (at least for now).
                    // We send it every `blocks_per_heartbeat` from the block they started up at.
                    if ((current_block.number - last_heartbeat_submitted_at) >= blocks_per_heartbeat
                        // Submitting earlier than one minute in may falsely indicate liveness.
                        ) && has_submitted_init_heartbeat.load(Ordering::Relaxed)
                    {
                        info!("Sending heartbeat at block: {}", current_block.number);
                        state_chain_client
                            .finalize_signed_extrinsic(
                                pallet_cf_reputation::Call::heartbeat {},
                            )
                            .await;

                        last_heartbeat_submitted_at = current_block.number;
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
