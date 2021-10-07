use futures::{Stream, StreamExt};
use pallet_cf_vaults::{
    rotation::{ChainParams, VaultRotationResponse},
    KeygenResponse, ThresholdSignatureResponse,
};
use slog::o;
use sp_runtime::AccountId32;
use std::{convert::TryInto, sync::Arc};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::{
    eth::EthBroadcaster,
    logging::COMPONENT_KEY,
    p2p, settings,
    signing::{
        KeyId, KeygenInfo, KeygenOutcome, MessageHash, MultisigEvent, MultisigInstruction,
        SigningInfo, SigningOutcome,
    },
};

pub mod interface {
    use anyhow::Result;
    use codec::{Decode, Encode};
    use frame_support::metadata::RuntimeMetadataPrefixed;
    use frame_support::unsigned::TransactionValidityError;
    use frame_system::Phase;
    use futures::compat::{Future01CompatExt, Stream01CompatExt};
    use futures::StreamExt;
    use futures::{Stream, TryFutureExt};
    use sp_core::{
        storage::{StorageChangeSet, StorageKey},
        twox_128, Bytes, Pair,
    };
    use sp_runtime::generic::Era;
    use std::convert::TryFrom;
    use std::fmt::Debug;
    use std::{marker::PhantomData, sync::Arc};
    use substrate_subxt::{
        extrinsic::{
            CheckEra, CheckGenesis, CheckNonce, CheckSpecVersion, CheckTxVersion, CheckWeight,
        },
        system::System,
        Runtime, SignedExtension, SignedExtra,
    };

    use crate::{common::Mutex, settings};

    ////////////////////
    // IMPORTANT: The types used here must match those in the state chain

    // Substrate_subxt's Runtime trait allows use to use it's extrinsic signing code
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct RuntimeImplForSigningExtrinsics {}
    impl System for RuntimeImplForSigningExtrinsics {
        type Index = <state_chain_runtime::Runtime as frame_system::Config>::Index;
        type BlockNumber = <state_chain_runtime::Runtime as frame_system::Config>::BlockNumber;
        type Hash = <state_chain_runtime::Runtime as frame_system::Config>::Hash;
        type Hashing = <state_chain_runtime::Runtime as frame_system::Config>::Hashing;
        type AccountId = <state_chain_runtime::Runtime as frame_system::Config>::AccountId;
        type Address = state_chain_runtime::Address;
        type Header = <state_chain_runtime::Runtime as frame_system::Config>::Header;
        type Extrinsic = state_chain_runtime::UncheckedExtrinsic;
        type AccountData = <state_chain_runtime::Runtime as frame_system::Config>::AccountData;
    }
    impl Runtime for RuntimeImplForSigningExtrinsics {
        type Signature = state_chain_runtime::Signature;
        type Extra = SCDefaultExtra<Self>;
        fn register_type_sizes(
            _event_type_registry: &mut substrate_subxt::EventTypeRegistry<Self>,
        ) {
            unreachable!();
        }
    }

    /// Needed so we can use substrate_subxt's extrinsic signing code
    /// Defines extra parameters contained in an extrinsic
    #[derive(Encode, Decode, Clone, Eq, PartialEq, Debug)]
    pub struct SCDefaultExtra<T: System> {
        spec_version: u32,
        tx_version: u32,
        nonce: T::Index,
        genesis_hash: T::Hash,
    }
    impl<T> SignedExtra<T> for SCDefaultExtra<T>
    where
        T: System + Clone + Debug + Eq + Send + Sync,
    {
        type Extra = (
            CheckSpecVersion<T>,
            CheckTxVersion<T>,
            CheckGenesis<T>,
            CheckEra<T>,
            CheckNonce<T>,
            CheckWeight<T>,
        );
        fn new(spec_version: u32, tx_version: u32, nonce: T::Index, genesis_hash: T::Hash) -> Self {
            SCDefaultExtra {
                spec_version,
                tx_version,
                nonce,
                genesis_hash,
            }
        }
        fn extra(&self) -> Self::Extra {
            (
                CheckSpecVersion(PhantomData, self.spec_version),
                CheckTxVersion(PhantomData, self.tx_version),
                CheckGenesis(PhantomData, self.genesis_hash),
                CheckEra((Era::Immortal, PhantomData), self.genesis_hash),
                CheckNonce(self.nonce),
                CheckWeight(PhantomData),
            )
        }
    }
    impl<T> SignedExtension for SCDefaultExtra<T>
    where
        T: System + Clone + Debug + Eq + Send + Sync,
    {
        const IDENTIFIER: &'static str = "SCDefaultExtra";
        type AccountId = T::AccountId;
        type Call = ();
        type AdditionalSigned =
            <<Self as SignedExtra<T>>::Extra as SignedExtension>::AdditionalSigned;
        type Pre = ();
        fn additional_signed(&self) -> Result<Self::AdditionalSigned, TransactionValidityError> {
            self.extra().additional_signed()
        }
    }

    type AuthorRpcClient = sc_rpc_api::author::AuthorClient<
        state_chain_runtime::Hash,
        <state_chain_runtime::Block as sp_runtime::traits::Block>::Hash,
    >;
    type ChainRpcClient = sc_rpc_api::chain::ChainClient<
        state_chain_runtime::BlockNumber,
        state_chain_runtime::Hash,
        state_chain_runtime::Header,
        state_chain_runtime::SignedBlock,
    >;
    type StateRpcClient = sc_rpc_api::state::StateClient<state_chain_runtime::Hash>;

    pub type EventInfo = (
        Phase,
        state_chain_runtime::Event,
        Vec<state_chain_runtime::Hash>,
    );

    ////////////////////

    pub struct StateChainClient {
        pub metadata: substrate_subxt::Metadata,
        runtime_version: sp_version::RuntimeVersion,
        genesis_hash: state_chain_runtime::Hash,
        nonce: Mutex<<RuntimeImplForSigningExtrinsics as System>::Index>,
        signer:
            substrate_subxt::PairSigner<RuntimeImplForSigningExtrinsics, sp_core::sr25519::Pair>,
        author_rpc_client: AuthorRpcClient,
    }
    impl StateChainClient {
        pub async fn submit_extrinsic<Extrinsic>(&self, logger: &slog::Logger, extrinsic: Extrinsic)
        where
            state_chain_runtime::Call: std::convert::From<Extrinsic>,
            Extrinsic: std::fmt::Debug + Clone,
        {
            slog::trace!(logger, "Submitting state chain extrinsic: {:?}", extrinsic);
            let mut nonce = self.nonce.lock().await;

            match substrate_subxt::extrinsic::create_signed::<RuntimeImplForSigningExtrinsics>(
                &self.runtime_version,
                self.genesis_hash,
                *nonce,
                substrate_subxt::Encoded(
                    state_chain_runtime::Call::from(extrinsic.clone()).encode(),
                ),
                &self.signer,
            )
            .map_err(anyhow::Error::new)
            .and_then(|signed_extrinic| {
                self.author_rpc_client
                    .submit_extrinsic(Bytes::from(signed_extrinic.encode()))
                    .compat()
                    .map_err(anyhow::Error::msg)
            })
            .await
            {
                Ok(_) => {
                    *nonce += 1;
                }
                Err(error) => {
                    slog::error!(
                        logger,
                        "Could not submit extrinsic: {:?}, {}",
                        extrinsic,
                        error
                    );
                }
            }
        }
    }

    pub async fn connect_to_state_chain(
        settings: &settings::Settings,
    ) -> Result<(
        <RuntimeImplForSigningExtrinsics as System>::AccountId,
        Arc<StateChainClient>,
        impl Stream<Item = Result<EventInfo>>,
        impl Stream<Item = Result<state_chain_runtime::Header>>,
    )> {
        use substrate_subxt::Signer;
        let signer = substrate_subxt::PairSigner::<
            RuntimeImplForSigningExtrinsics,
            sp_core::sr25519::Pair,
        >::new(sp_core::sr25519::Pair::from_seed(
            &(<[u8; 32]>::try_from(
                hex::decode(
                    &std::fs::read_to_string(&settings.state_chain.signing_key_file)?
                        .replace("\"", ""),
                )
                .map_err(|err| anyhow::Error::new(err))?,
            )
            .map_err(|_err| anyhow::Error::msg("Signing key seed is the wrong length."))?),
        ));

        let rpc_server_url = &url::Url::parse(settings.state_chain.ws_endpoint.as_str())?;

        // TODO connect only once

        let author_rpc_client =
            crate::p2p::rpc::alt_jsonrpc_connect::connect::<AuthorRpcClient>(rpc_server_url)
                .compat()
                .await
                .map_err(anyhow::Error::msg)?;

        let chain_rpc_client =
            crate::p2p::rpc::alt_jsonrpc_connect::connect::<ChainRpcClient>(rpc_server_url)
                .compat()
                .await
                .map_err(anyhow::Error::msg)?;

        let state_rpc_client =
            crate::p2p::rpc::alt_jsonrpc_connect::connect::<StateRpcClient>(rpc_server_url)
                .compat()
                .await
                .map_err(anyhow::Error::msg)?;

        let metadata = substrate_subxt::Metadata::try_from(RuntimeMetadataPrefixed::decode(
            &mut &state_rpc_client
                .metadata(None)
                .compat()
                .await
                .map_err(anyhow::Error::msg)?[..],
        )?)?;

        Ok((
            signer.account_id().clone(),
            {
                Arc::new(StateChainClient {
                    metadata,
                    runtime_version: state_rpc_client
                        .runtime_version(None)
                        .compat()
                        .await
                        .map_err(anyhow::Error::msg)?,
                    genesis_hash: match chain_rpc_client
                        .block_hash(Some(sp_rpc::number::NumberOrHex::from(0u64).into()))
                        .compat()
                        .await
                        .map_err(anyhow::Error::msg)?
                    {
                        sp_rpc::list::ListOrValue::Value(Some(value)) => Ok(value),
                        _ => Err(anyhow::Error::msg("Genesis block doesn't exist?")),
                    }?,
                    nonce: Mutex::new({
                        let account_info: frame_system::AccountInfo<
                            <RuntimeImplForSigningExtrinsics as System>::Index,
                            <RuntimeImplForSigningExtrinsics as System>::AccountData,
                        > = Decode::decode(
                            &mut &state_rpc_client
                                .storage(
                                    StorageKey(
                                        std::array::IntoIter::new([
                                            std::array::IntoIter::new(twox_128(b"System")),
                                            std::array::IntoIter::new(twox_128(b"Account")),
                                            std::array::IntoIter::new(twox_128(
                                                &signer.account_id().encode()[..],
                                            )),
                                        ])
                                        .flatten()
                                        .collect::<Vec<_>>(),
                                    ),
                                    None,
                                )
                                .compat()
                                .await
                                .map_err(anyhow::Error::msg)?
                                .ok_or(anyhow::Error::msg("Account doesn't exist"))?
                                .0[..],
                        )?;
                        account_info.nonce
                    }),
                    signer,
                    author_rpc_client,
                })
            },
            // TODO: Remove duplicate finalized header subscriptions (By merging the heartbeat into the sc_observer)
            {
                let system_event_storage_key = vec![StorageKey(
                    std::array::IntoIter::new([
                        std::array::IntoIter::new(twox_128(b"System")),
                        std::array::IntoIter::new(twox_128(b"Events")),
                    ])
                    .flatten()
                    .collect::<Vec<_>>(),
                )];

                chain_rpc_client.subscribe_finalized_heads().compat().await.map_err(anyhow::Error::msg)?.compat().then(move |result_header| {
                    let state_rpc_client = state_rpc_client.clone();
                    let system_event_storage_key = system_event_storage_key.clone();
                    async move {
                        use itertools::Itertools;
                        tokio_stream::iter(std::iter::once(match result_header {
                            Ok(header) =>
                                state_rpc_client
                                    .query_storage_at(
                                        system_event_storage_key,
                                        Some(header.hash())
                                    ).compat().await.map_err(anyhow::Error::msg).map(|storage_change_sets| {
                                        storage_change_sets.into_iter().map(|storage_change_set| {
                                            let StorageChangeSet {
                                                block: _,
                                                changes
                                            } = storage_change_set;
                                            changes.into_iter().filter_map(|(_storage_key, option_data)| {
                                                option_data.map(|data| {
                                                    Vec::<(Phase, state_chain_runtime::Event, Vec<state_chain_runtime::Hash>)>::decode(&mut &data.0[..]).map_err(anyhow::Error::msg)
                                                })
                                            }).flatten_ok()
                                        }).flatten()
                                    }),
                            Err(error) => Err(anyhow::Error::msg(error))
                        }).flatten_ok().map(|result_result| result_result.and_then(std::convert::identity)))
                    }
                }).flatten()
            },
            chain_rpc_client
                .subscribe_finalized_heads()
                .compat()
                .await
                .map_err(anyhow::Error::msg)?
                .compat()
                .map(|result_header| result_header.map_err(anyhow::Error::msg)),
        ))
    }
}

pub async fn start<EventStream>(
    settings: &settings::Settings,
    state_chain_client: Arc<interface::StateChainClient>,
    sc_event_stream: EventStream,
    eth_broadcaster: EthBroadcaster,
    multisig_instruction_sender: UnboundedSender<MultisigInstruction>,
    mut multisig_event_receiver: UnboundedReceiver<MultisigEvent>,
    logger: &slog::Logger,
) where
    EventStream: Stream<Item = anyhow::Result<interface::EventInfo>>,
{
    let logger = logger.new(o!(COMPONENT_KEY => "SCObserver"));

    let mut sc_event_stream = Box::pin(sc_event_stream);
    while let Some(result_event) = sc_event_stream.next().await {
        match result_event {
            Ok((_phase, event, _topics)) => {
                match event {
                    state_chain_runtime::Event::pallet_cf_vaults(
                        pallet_cf_vaults::Event::KeygenRequest(ceremony_id, keygen_request),
                    ) => {
                        let signers: Vec<_> = keygen_request
                            .validator_candidates
                            .iter()
                            .map(|v| p2p::AccountId(v.clone().into()))
                            .collect();

                        let gen_new_key_event =
                            MultisigInstruction::KeyGen(KeygenInfo::new(ceremony_id, signers));

                        multisig_instruction_sender
                            .send(gen_new_key_event)
                            .map_err(|_| "Receiver should exist")
                            .unwrap();

                        let response = match multisig_event_receiver.recv().await {
                            Some(event) => match event {
                                MultisigEvent::KeygenResult(KeygenOutcome { id: _, result }) => {
                                    match result {
                                        Ok(pubkey) => {
                                            KeygenResponse::<AccountId32, Vec<u8>>::Success(
                                                pubkey.serialize().into(),
                                            )
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
                                            KeygenResponse::Error(bad_account_ids)
                                        }
                                    }
                                }
                                MultisigEvent::MessageSigningResult(message_signing_result) => {
                                    panic!(
                                        "Expecting KeygenResult, got: {:?}",
                                        message_signing_result
                                    );
                                }
                            },
                            None => todo!(),
                        };
                        state_chain_client
                            .submit_extrinsic(
                                &logger,
                                pallet_cf_witnesser_api::Call::witness_keygen_response(
                                    ceremony_id,
                                    response,
                                ),
                            )
                            .await;
                    }
                    state_chain_runtime::Event::pallet_cf_vaults(
                        pallet_cf_vaults::Event::ThresholdSignatureRequest(
                            ceremony_id,
                            threshold_signature_request,
                        ),
                    ) => {
                        let signers: Vec<_> = threshold_signature_request
                            .validators
                            .iter()
                            .map(|v| p2p::AccountId(v.clone().into()))
                            .collect();

                        let message_hash: [u8; 32] = threshold_signature_request
                            .payload
                            .try_into()
                            .expect("Should be a 32 byte hash");
                        let sign_tx = MultisigInstruction::Sign(SigningInfo::new(
                            ceremony_id,
                            KeyId(threshold_signature_request.public_key),
                            MessageHash(message_hash),
                            signers,
                        ));

                        // The below will be replaced with one shot channels
                        multisig_instruction_sender
                            .send(sign_tx)
                            .map_err(|_| "Receiver should exist")
                            .unwrap();

                        let response = match multisig_event_receiver.recv().await {
                            Some(event) => match event {
                                MultisigEvent::MessageSigningResult(SigningOutcome {
                                    id: _,
                                    result,
                                }) => match result {
                                    Ok(sig) => ThresholdSignatureResponse::<
                                        AccountId32,
                                        pallet_cf_vaults::SchnorrSigTruncPubkey,
                                    >::Success {
                                        message_hash,
                                        signature: sig.into(),
                                    },
                                    Err((err, bad_account_ids)) => {
                                        slog::error!(
                                            logger,
                                            "Signing failed with error: {:?}",
                                            err
                                        );
                                        let bad_account_ids: Vec<_> = bad_account_ids
                                            .iter()
                                            .map(|v| AccountId32::from(v.0))
                                            .collect();
                                        ThresholdSignatureResponse::Error(bad_account_ids)
                                    }
                                },
                                MultisigEvent::KeygenResult(keygen_result) => {
                                    panic!(
                                        "Expecting MessageSigningResult, got: {:?}",
                                        keygen_result
                                    );
                                }
                            },
                            _ => panic!("Channel closed"),
                        };
                        state_chain_client
                            .submit_extrinsic(
                                &logger,
                                pallet_cf_witnesser_api::Call::witness_threshold_signature_response(
                                    ceremony_id,
                                    response,
                                ),
                            )
                            .await;
                    }
                    state_chain_runtime::Event::pallet_cf_vaults(
                        pallet_cf_vaults::Event::VaultRotationRequest(
                            ceremony_id,
                            vault_rotation_request,
                        ),
                    ) => {
                        match vault_rotation_request.chain {
                            ChainParams::Ethereum(tx) => {
                                slog::debug!(
                                    logger,
                                    "Sending ETH vault rotation tx for ceremony {}: {:?}",
                                    ceremony_id,
                                    tx
                                );
                                // TODO: Contract address should come from the state chain
                                // https://github.com/chainflip-io/chainflip-backend/issues/459
                                let response = match eth_broadcaster
                                    .send(tx, settings.eth.key_manager_eth_address)
                                    .await
                                {
                                    Ok(tx_hash) => {
                                        slog::debug!(
                                            logger,
                                            "Broadcast set_agg_key_with_agg_key tx, tx_hash: {}",
                                            tx_hash
                                        );
                                        VaultRotationResponse::Success {
                                            tx_hash: tx_hash.as_bytes().to_vec(),
                                        }
                                    }
                                    Err(e) => {
                                        slog::error!(
                                            logger,
                                            "Failed to broadcast set_agg_key_with_agg_key tx: {}",
                                            e
                                        );
                                        VaultRotationResponse::Error
                                    }
                                };
                                state_chain_client.submit_extrinsic(
                                    &logger,
                                    pallet_cf_witnesser_api::Call::witness_vault_rotation_response(
                                        ceremony_id,
                                        response,
                                    ),
                                ).await;
                            }
                        }
                    }
                    ignored_event => {
                        // ignore events we don't care about
                        slog::trace!(logger, "Ignoring event: {:?}", ignored_event);
                    }
                }
            }
            Err(error) => {
                slog::error!(logger, "{}", error);
            }
        };
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
        let logger = logging::test_utils::create_test_logger();

        let (_account_id, state_chain_client, event_stream, _block_stream) =
            interface::connect_to_state_chain(&settings).await.unwrap();

        let (multisig_instruction_sender, _multisig_instruction_receiver) =
            tokio::sync::mpsc::unbounded_channel::<MultisigInstruction>();
        let (_multisig_event_sender, multisig_event_receiver) =
            tokio::sync::mpsc::unbounded_channel::<MultisigEvent>();

        let web3 = eth::new_synced_web3_client(&settings, &logger)
            .await
            .unwrap();
        let eth_broadcaster = EthBroadcaster::new(&settings, web3.clone()).unwrap();

        start(
            &settings,
            state_chain_client,
            event_stream,
            eth_broadcaster,
            multisig_instruction_sender,
            multisig_event_receiver,
            &logger,
        )
        .await;
    }
}
