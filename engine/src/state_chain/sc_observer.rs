use anyhow::Result;
use futures::Future;
use pallet_cf_vaults::rotation::{ChainParams, VaultRotationResponse};
use slog::o;
use sp_core::Hasher;
use sp_runtime::traits::Keccak256;
use substrate_subxt::{Client, EventSubscription, PairSigner};
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    eth::{CFContract, EthBroadcaster},
    logging::COMPONENT_KEY,
    p2p,
    signing::{KeyId, KeygenInfo, MessageHash, MultisigInstruction, SigningInfo},
    state_chain::{
        pallets::vaults::{
            VaultRotationResponseCallExt,
            VaultsEvent::{EthSignTxRequestEvent, KeygenRequestEvent, VaultRotationRequestEvent},
        },
        sc_event::SCEvent::{AuctionEvent, StakingEvent, ValidatorEvent, VaultsEvent},
    },
};

use sp_keyring::AccountKeyring;

use super::{runtime::StateChainRuntime, sc_event::raw_event_to_sc_event};

pub async fn start(
    subxt_client: Client<StateChainRuntime>,
    eth_broadcaster: EthBroadcaster,
    multisig_instruction_sender: UnboundedSender<MultisigInstruction>,
    logger: &slog::Logger,
) {
    let logger = logger.new(o!(COMPONENT_KEY => "SCObserver"));
    // subscribe to all finalised events, and then redirect them
    let sub = subxt_client
        .subscribe_finalized_events()
        .await
        .expect("Could not subscribe to state chain events");

    let decoder = subxt_client.events_decoder();
    let mut sub = EventSubscription::new(sub, decoder);
    while let Some(res_event) = sub.next().await {
        let raw_event = match res_event {
            Ok(raw_event) => raw_event,
            Err(e) => {
                slog::error!(logger, "Next event could not be read: {}", e);
                continue;
            }
        };

        if let None =
            raw_event_to_sc_event(&raw_event).expect("Could not convert substrate event to SCEvent")
        {
            slog::trace!(logger, "No action for raw event: {:?}", raw_event);
            continue;
        }

        match sc_event.expect("Not None, checked above") {
            AuctionEvent(_) => todo!(),
            ValidatorEvent(_) => todo!(),
            StakingEvent(_) => todo!(),
            VaultsEvent(event) => match event {
                KeygenRequestEvent(keygen_request_event) => {
                    let validators: Vec<_> = keygen_request_event
                        .keygen_request
                        .validator_candidates
                        .iter()
                        .map(|v| p2p::ValidatorId(v.clone().into()))
                        .collect();
                    // TODO: Should this be request index? @andy
                    let key_gen_info =
                        KeygenInfo::new(KeyId(keygen_request_event.request_index), validators);
                    let gen_new_key_event = MultisigInstruction::KeyGen(key_gen_info);
                    // We need the Sender for Multisig instructions channel here
                    multisig_instruction_sender
                        .send(gen_new_key_event)
                        .map_err(|_| "Receiver should exist")
                        .unwrap();
                }
                EthSignTxRequestEvent(eth_sign_tx_request) => {
                    let validators: Vec<_> = eth_sign_tx_request
                        .eth_signing_tx_request
                        .validators
                        .iter()
                        .map(|v| p2p::ValidatorId(v.clone().into()))
                        .collect();

                    // TODO: Should this hash be on the state chain or the signing module?
                    // https://github.com/chainflip-io/chainflip-backend/issues/446
                    let hash =
                        Keccak256::hash(&eth_sign_tx_request.eth_signing_tx_request.payload[..]);
                    let message_hash = MessageHash(hash.0);

                    // TODO: we want to use some notion of "KeyId"
                    // https://github.com/chainflip-io/chainflip-backend/issues/442
                    let signing_info =
                        SigningInfo::new(KeyId(eth_sign_tx_request.request_index), validators);

                    let sign_tx = MultisigInstruction::Sign(message_hash, signing_info);

                    multisig_instruction_sender
                        .send(sign_tx)
                        .map_err(|_| "Receiver should exist")
                        .unwrap();
                }
                VaultRotationRequestEvent(vault_rotation_request_event) => {
                    let alice = AccountKeyring::Alice.pair();
                    let pair_signer = PairSigner::new(alice);
                    match vault_rotation_request_event.vault_rotation_request.chain {
                        ChainParams::Ethereum(tx) => {
                            slog::debug!(logger, "Broadcasting to ETH: {:?}", tx);
                            match eth_broadcaster
                                .sign_and_broadcast_to(tx.clone(), CFContract::KeyManager)
                                .await
                            {
                                Ok(tx_hash) => {
                                    slog::debug!(
                                        logger,
                                        "Broadcast set_agg_key_with_agg_key tx, tx_hash: {}",
                                        tx_hash
                                    );
                                    subxt_client
                                        .vault_rotation_response(
                                            &pair_signer,
                                            vault_rotation_request_event.request_index,
                                            VaultRotationResponse::Success {
                                                // TODO: Add the actual keys here
                                                // why are these being added here? The SC should know these already? right?
                                                old_key: Vec::default(),
                                                new_key: Vec::default(),
                                                tx,
                                            },
                                        )
                                        .await
                                        .unwrap(); // TODO: Handle error
                                }
                                Err(e) => {
                                    slog::error!(
                                        logger,
                                        "Failed to broadcast set_agg_key_with_agg_key tx: {}",
                                        e
                                    );
                                    subxt_client
                                        .vault_rotation_response(
                                            &pair_signer,
                                            vault_rotation_request_event.request_index,
                                            VaultRotationResponse::Failure,
                                        )
                                        .await
                                        .unwrap(); // TODO: Handle error
                                }
                            }
                        }
                        // Leave this to be explicit about future chains being added
                        ChainParams::Other(_) => todo!("Chain::Other does not exist"),
                    }
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use substrate_subxt::ClientBuilder;

    use crate::{logging, mq::nats_client::NatsMQClient, settings};

    use super::*;

    #[tokio::test]
    #[ignore = "runs forever, useful for testing without having to start the whole CFE"]
    async fn run_the_sc_observer() {
        let settings = settings::test_utils::new_test_settings().unwrap();

        start(
            NatsMQClient::new(&settings.message_queue).await.unwrap(),
            ClientBuilder::<StateChainRuntime>::new()
                .set_url(&settings.state_chain.ws_endpoint)
                .build()
                .await
                .expect("Should create subxt client"),
            &logging::test_utils::create_test_logger(),
        )
        .await;
    }
}
