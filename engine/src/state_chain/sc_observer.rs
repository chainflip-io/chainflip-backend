use std::marker::PhantomData;

use anyhow::Result;
use pallet_cf_vaults::rotation::{ChainParams, VaultRotationResponse};
use slog::o;
use sp_core::Hasher;
use sp_runtime::traits::Keccak256;
use substrate_subxt::{Client, EventSubscription, PairSigner};

use crate::{
    eth::{CFContract, Web3Signer},
    logging::COMPONENT_KEY,
    mq::{IMQClient, Subject},
    p2p,
    signing::{KeyId, KeygenInfo, MessageHash, MultisigInstruction, SigningInfo},
    state_chain::{
        pallets::vaults::{
            VaultRotationResponseCallExt,
            VaultsEvent::{EthSignTxRequestEvent, KeygenRequestEvent, VaultRotationRequestEvent},
        },
        sc_event::SCEvent::{AuctionEvent, StakingEvent, ValidatorEvent, VaultsEvent},
    },
    types::chain::Chain,
};

use sp_keyring::AccountKeyring;

use super::{runtime::StateChainRuntime, sc_event::raw_event_to_subject_and_sc_event};

pub async fn start<M: IMQClient>(
    mq_client: M,
    subxt_client: Client<StateChainRuntime>,
    web3_signer: Web3Signer,
    logger: &slog::Logger,
) {
    SCObserver::new(mq_client, subxt_client, web3_signer, logger)
        .await
        .run()
        .await
        .expect("SC Observer has died!");
}

pub struct SCObserver<M: IMQClient> {
    mq_client: M,
    subxt_client: Client<StateChainRuntime>,
    web3_signer: Web3Signer,
    logger: slog::Logger,
}

impl<M: IMQClient> SCObserver<M> {
    pub async fn new(
        mq_client: M,
        subxt_client: Client<StateChainRuntime>,
        web3_signer: Web3Signer,
        logger: &slog::Logger,
    ) -> Self {
        Self {
            mq_client,
            subxt_client,
            web3_signer,
            logger: logger.new(o!(COMPONENT_KEY => "SCObserver")),
        }
    }

    pub async fn run(&self) -> Result<()> {
        // subscribe to all finalised events, and then redirect them
        let sub = self
            .subxt_client
            .subscribe_finalized_events()
            .await
            .expect("Could not subscribe to state chain events");
        let decoder = self.subxt_client.events_decoder();
        let mut sub = EventSubscription::new(sub, decoder);

        while let Some(res_event) = sub.next().await {
            let raw_event = match res_event {
                Ok(raw_event) => raw_event,
                Err(e) => {
                    slog::error!(self.logger, "Next event could not be read: {}", e);
                    continue;
                }
            };

            let subject_and_sc_event = raw_event_to_subject_and_sc_event(&raw_event)?;

            if let None = subject_and_sc_event {
                slog::trace!(self.logger, "Discarding {:?}", raw_event);
                continue;
            }

            let (_subject, sc_event) =
                subject_and_sc_event.expect("Must be Some due to condition above");

            match sc_event {
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
                        self.mq_client
                            .publish(Subject::MultisigInstruction, &gen_new_key_event)
                            .await
                            .expect("Should publish to MQ");
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
                        let hash = Keccak256::hash(
                            &eth_sign_tx_request.eth_signing_tx_request.payload[..],
                        );
                        let message_hash = MessageHash(hash.0);

                        // TODO: we want to use some notion of "KeyId"
                        // https://github.com/chainflip-io/chainflip-backend/issues/442
                        let signing_info =
                            SigningInfo::new(KeyId(eth_sign_tx_request.request_index), validators);

                        let sign_tx = MultisigInstruction::Sign(message_hash, signing_info);

                        self.mq_client
                            .publish(Subject::MultisigInstruction, &sign_tx)
                            .await
                            .expect("should publish to MQ");

                        // receive the signed message via message queue here?
                        // eventually this will be a one shot channel
                    }
                    VaultRotationRequestEvent(vault_rotation_request_event) => {
                        // broadcast the transaction to the eth chain
                        match vault_rotation_request_event.vault_rotation_request.chain {
                            // TODO: The broadcasting should contain some reference to the request_id, so we can report
                            // failure back to the SC that a particular request id failed
                            ChainParams::Ethereum(tx) => {
                                slog::debug!(self.logger, "Broadcasting to ETH: {:?}", tx);
                                match self
                                    .web3_signer
                                    .sign_and_broadcast_to(tx.clone(), CFContract::KeyManager)
                                    .await
                                {
                                    Ok(tx_hash) => {
                                        // broadcast was successful. Yay!
                                        let alice = AccountKeyring::Alice.pair();
                                        let pair_signer = PairSigner::new(alice);

                                        let vault_rotation_response = VaultRotationResponse {
                                            old_key: Vec::default(),
                                            new_key: Vec::default(),
                                            tx,
                                        };

                                        self.subxt_client
                                            .vault_rotation_response(
                                                &pair_signer,
                                                vault_rotation_request_event.request_index,
                                                vault_rotation_response,
                                            )
                                            .await?;
                                    }
                                    Err(e) => {}
                                }
                            }
                            // Leave this to be explicit about future chains being added
                            ChainParams::Other(_) => todo!("Chain::Other does not exist"),
                        }
                    }
                },
            }
        }

        let err_msg = "State Chain Observer stopped subscribing to events!";
        slog::error!(self.logger, "{}", err_msg);
        Err(anyhow::Error::msg(err_msg))
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
