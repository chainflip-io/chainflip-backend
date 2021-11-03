use anyhow::Result;
use codec::{Decode, Encode};
use frame_support::metadata::RuntimeMetadataPrefixed;
use frame_support::unsigned::TransactionValidityError;
use frame_system::Phase;
use futures::compat::{Future01CompatExt, Stream01CompatExt};
use futures::Stream;
use futures::StreamExt;
use itertools::Itertools;
use jsonrpc_core::{Error, ErrorCode};
use jsonrpc_core_client::RpcError;
use sp_core::H256;
use sp_core::{
    storage::{StorageChangeSet, StorageKey},
    Bytes, Pair,
};
use sp_runtime::generic::Era;
use sp_runtime::traits::{BlakeTwo256, Hash};
use sp_runtime::AccountId32;
use std::convert::TryFrom;
use std::fmt::Debug;
use std::sync::atomic::{AtomicU32, Ordering};
use std::{marker::PhantomData, sync::Arc};
use substrate_subxt::{
    extrinsic::{
        CheckEra, CheckGenesis, CheckNonce, CheckSpecVersion, CheckTxVersion, CheckWeight,
    },
    system::System,
    Runtime, SignedExtension, SignedExtra,
};

use crate::common;
use crate::settings;

#[cfg(test)]
use mockall::automock;

use async_trait::async_trait;

////////////////////
// IMPORTANT: The types used here must match those in the state chain

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
// Substrate_subxt's Runtime trait allows us to use it's extrinsic signing code
impl Runtime for RuntimeImplForSigningExtrinsics {
    type Signature = state_chain_runtime::Signature;
    type Extra = SCDefaultExtra<Self>;
    fn register_type_sizes(_event_type_registry: &mut substrate_subxt::EventTypeRegistry<Self>) {
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
    #[allow(clippy::type_complexity)]
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
    type AdditionalSigned = <<Self as SignedExtra<T>>::Extra as SignedExtension>::AdditionalSigned;
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

/// Number of times to retry if the nonce is wrong
const MAX_RETRY_ATTEMPTS: usize = 10;

pub struct StateChainRpcClient {
    events_storage_key: StorageKey,
    runtime_version: sp_version::RuntimeVersion,
    genesis_hash: state_chain_runtime::Hash,
    pub signer:
        substrate_subxt::PairSigner<RuntimeImplForSigningExtrinsics, sp_core::sr25519::Pair>,
    author_rpc_client: AuthorRpcClient,
    state_rpc_client: StateRpcClient,
    chain_rpc_client: ChainRpcClient,
}

/// Wraps the substrate client library methods
#[cfg_attr(test, automock)]
#[async_trait]
pub trait StateChainRpcApi {
    async fn submit_extrinsic_rpc<Extrinsic>(
        &self,
        nonce: u32,
        extrinsic: Extrinsic,
    ) -> Result<sp_core::H256, RpcError>
    where
        state_chain_runtime::Call: std::convert::From<Extrinsic>,
        Extrinsic: 'static + std::fmt::Debug + Clone + Send;

    /// Get a block
    async fn get_block(
        &self,
        block_hash: state_chain_runtime::Hash,
    ) -> Result<Option<state_chain_runtime::SignedBlock>>;

    /// Watches *only* submitted extrinsics. I.e. Cannot watch for chain called extrinsics.
    async fn watch_submitted_extrinsic_rpc<BlockStream>(
        &self,
        ext_hash: state_chain_runtime::Hash,
        block_stream: Arc<common::Mutex<BlockStream>>,
    ) -> Result<Vec<state_chain_runtime::Event>>
    where
        BlockStream:
            Stream<Item = anyhow::Result<state_chain_runtime::Header>> + Unpin + Send + 'static;

    /// Get events for a particular block
    async fn get_events(
        &self,
        block_header: &state_chain_runtime::Header,
    ) -> Result<Vec<EventInfo>>;
}

#[async_trait]
impl StateChainRpcApi for StateChainRpcClient {
    async fn submit_extrinsic_rpc<Extrinsic>(
        &self,
        nonce: u32,
        extrinsic: Extrinsic,
    ) -> Result<sp_core::H256, RpcError>
    where
        state_chain_runtime::Call: std::convert::From<Extrinsic>,
        Extrinsic: 'static + std::fmt::Debug + Clone + Send,
    {
        self.author_rpc_client
            .submit_extrinsic(Bytes::from(
                substrate_subxt::extrinsic::create_signed::<RuntimeImplForSigningExtrinsics>(
                    &self.runtime_version,
                    self.genesis_hash,
                    nonce,
                    substrate_subxt::Encoded(state_chain_runtime::Call::from(extrinsic).encode()),
                    &self.signer,
                )
                .await
                .expect("Should be able to sign")
                .encode(),
            ))
            .compat()
            .await
    }

    async fn get_block(
        &self,
        block_hash: state_chain_runtime::Hash,
    ) -> Result<Option<state_chain_runtime::SignedBlock>> {
        let block = self
            .chain_rpc_client
            .block(Some(block_hash))
            .compat()
            .await
            .unwrap();
        Ok(block)
    }

    async fn watch_submitted_extrinsic_rpc<BlockStream>(
        &self,
        extrinsic_hash: state_chain_runtime::Hash,
        block_stream: Arc<common::Mutex<BlockStream>>,
    ) -> Result<Vec<state_chain_runtime::Event>>
    where
        BlockStream:
            Stream<Item = anyhow::Result<state_chain_runtime::Header>> + Unpin + Send + 'static,
    {
        let mut events_for_extrinsic = Vec::new();
        let mut found_event = false;
        while let Some(block_header) = block_stream.lock().await.next().await {
            let header = block_header?;
            let hash = header.hash();
            if let Some(signed_block) = self.get_block(hash).await? {
                let extrinsic_index_found = signed_block.block.extrinsics.iter().position(|ext| {
                    let hash = BlakeTwo256::hash_of(ext);
                    hash == extrinsic_hash
                });

                let events_for_block = self.get_events(&header).await?;
                for (phase, event, _) in events_for_block {
                    if let Phase::ApplyExtrinsic(i) = phase {
                        if let Some(extrinsic_index) = extrinsic_index_found {
                            if i as usize != extrinsic_index {
                                continue;
                            }
                            found_event = true;
                            events_for_extrinsic.push(event);
                        }
                    }
                }
            };
            if found_event {
                break;
            };
        }
        Ok(events_for_extrinsic)
    }

    async fn get_events(
        &self,
        block_header: &state_chain_runtime::Header,
    ) -> Result<Vec<EventInfo>> {
        self.state_rpc_client
            .query_storage_at(
                vec![self.events_storage_key.clone()],
                Some(block_header.hash()),
            )
            .compat()
            .await
            .map_err(anyhow::Error::msg)?
            .into_iter()
            .map(|storage_change_set| {
                let StorageChangeSet { block: _, changes } = storage_change_set;
                changes
                    .into_iter()
                    .filter_map(|(_storage_key, option_data)| {
                        option_data.map(|data| {
                            Vec::<EventInfo>::decode(&mut &data.0[..]).map_err(anyhow::Error::msg)
                        })
                    })
                    .flatten_ok()
            })
            .flatten()
            .collect::<Result<Vec<_>>>()
    }
}

pub struct StateChainClient<RpcClient: StateChainRpcApi> {
    metadata: substrate_subxt::Metadata,
    nonce: AtomicU32,
    /// Our Node's AcccountId
    pub our_account_id: AccountId32,

    state_chain_rpc_client: RpcClient,
}

impl<RpcClient: StateChainRpcApi> StateChainClient<RpcClient> {
    /// Submit an extrinsic and retry if it fails on an invalid nonce
    pub async fn submit_extrinsic<Extrinsic>(
        &self,
        logger: &slog::Logger,
        extrinsic: Extrinsic,
    ) -> Result<H256>
    where
        state_chain_runtime::Call: std::convert::From<Extrinsic>,
        Extrinsic: 'static + std::fmt::Debug + Clone + Send,
    {
        for _ in 0..MAX_RETRY_ATTEMPTS {
            // use the previous value but increment it for the next thread that loads/fetches it
            let nonce = self.nonce.fetch_add(1, Ordering::Relaxed);
            match self
                .state_chain_rpc_client
                .submit_extrinsic_rpc(nonce, extrinsic.clone())
                .await
            {
                Ok(tx_hash) => {
                    slog::trace!(
                        logger,
                        "Extrinsic submitted successfully with tx_hash: {}",
                        tx_hash
                    );
                    return Ok(tx_hash);
                }
                Err(rpc_err) => match rpc_err {
                    RpcError::JsonRpcError(Error {
                        // this is the error returned when the "priority is too low" i.e. nonce is too low
                        code: ErrorCode::ServerError(1014),
                        ..
                    }) => {
                        slog::error!(logger, "Extrinsic submission failed with nonce: {}", nonce);
                    }
                    err => {
                        slog::error!(logger, "Error: {}", err);
                        self.nonce.fetch_sub(1, Ordering::Relaxed);
                        return Err(anyhow::Error::msg(err));
                    }
                },
            }
        }
        slog::error!(logger, "Exceeded maximum number of retry attempts");
        Err(anyhow::Error::msg(
            "Exceeded maximum number of retry attempts",
        ))
    }

    /// Get all the events from a particular block
    pub async fn get_events(
        &self,
        block_header: &state_chain_runtime::Header,
    ) -> Result<Vec<EventInfo>> {
        self.state_chain_rpc_client.get_events(block_header).await
    }

    pub async fn watch_submitted_extrinsic<BlockStream>(
        &self,
        ext_hash: state_chain_runtime::Hash,
        block_stream: Arc<common::Mutex<BlockStream>>,
    ) -> Result<Vec<state_chain_runtime::Event>>
    where
        BlockStream:
            Stream<Item = anyhow::Result<state_chain_runtime::Header>> + Unpin + Send + 'static,
    {
        self.state_chain_rpc_client
            .watch_submitted_extrinsic_rpc(ext_hash, block_stream)
            .await
    }

    pub fn get_metadata(&self) -> substrate_subxt::Metadata {
        self.metadata.clone()
    }

    pub fn get_heartbeat_block_interval(&self) -> u32 {
        self.metadata
            .module("Reputation")
            .expect("No module 'Reputation' in chain metadata")
            .constant("HeartbeatBlockInterval")
            .expect(
                "No constant 'HeartbeatBlockInterval' in chain metadata for module 'Reputation'",
            )
            .value::<u32>()
            .expect("Could not decode HeartbeatBlockInterval to u32")
    }
}

#[allow(clippy::eval_order_dependence)]
pub async fn connect_to_state_chain(
    state_chain_settings: &settings::StateChain,
) -> Result<(
    Arc<StateChainClient<StateChainRpcClient>>,
    impl Stream<Item = Result<state_chain_runtime::Header>>,
)> {
    fn try_unwrap_value<T, E>(
        lorv: sp_rpc::list::ListOrValue<Option<T>>,
        error: E,
    ) -> Result<T, E> {
        match lorv {
            sp_rpc::list::ListOrValue::Value(Some(value)) => Ok(value),
            _ => Err(error),
        }
    }

    use substrate_subxt::Signer;
    let signer = substrate_subxt::PairSigner::<
        RuntimeImplForSigningExtrinsics,
        sp_core::sr25519::Pair,
    >::new(sp_core::sr25519::Pair::from_seed(
        &(<[u8; 32]>::try_from(
            hex::decode(
                &std::fs::read_to_string(&state_chain_settings.signing_key_file)?.replace("\"", ""),
            )
            .map_err(anyhow::Error::new)?,
        )
        .map_err(|_err| anyhow::Error::msg("Signing key seed is the wrong length."))?),
    ));

    let rpc_server_url = &url::Url::parse(state_chain_settings.ws_endpoint.as_str())?;

    // TODO connect only once (Using a single RpcChannel)

    let author_rpc_client =
        crate::common::alt_jsonrpc_connect::connect::<AuthorRpcClient>(rpc_server_url)
            .compat()
            .await
            .map_err(anyhow::Error::msg)?;

    let chain_rpc_client =
        crate::common::alt_jsonrpc_connect::connect::<ChainRpcClient>(rpc_server_url)
            .compat()
            .await
            .map_err(anyhow::Error::msg)?;

    let state_rpc_client =
        crate::common::alt_jsonrpc_connect::connect::<StateRpcClient>(rpc_server_url)
            .compat()
            .await
            .map_err(anyhow::Error::msg)?;

    let latest_block_hash = Some(try_unwrap_value(
        chain_rpc_client
            .block_hash(None)
            .compat()
            .await
            .map_err(anyhow::Error::msg)?,
        anyhow::Error::msg("Failed to get latest block hash"),
    )?);

    let metadata = substrate_subxt::Metadata::try_from(RuntimeMetadataPrefixed::decode(
        &mut &state_rpc_client
            .metadata(latest_block_hash)
            .compat()
            .await
            .map_err(anyhow::Error::msg)?[..],
    )?)?;

    let system_pallet_metadata = metadata.module("System")?.clone();
    let state_chain_rpc_client = StateChainRpcClient {
        events_storage_key: system_pallet_metadata.clone().storage("Events")?.prefix(),
        runtime_version: state_rpc_client
            .runtime_version(latest_block_hash)
            .compat()
            .await
            .map_err(anyhow::Error::msg)?,
        genesis_hash: try_unwrap_value(
            chain_rpc_client
                .block_hash(Some(sp_rpc::number::NumberOrHex::from(0u64).into()))
                .compat()
                .await
                .map_err(anyhow::Error::msg)?,
            anyhow::Error::msg("Genesis block doesn't exist?"),
        )?,
        signer: signer.clone(),
        author_rpc_client,
        state_rpc_client,
        chain_rpc_client: chain_rpc_client.clone(),
    };

    let our_account_id = signer.account_id().to_owned();

    Ok((
        Arc::new(StateChainClient {
            metadata,
            nonce: AtomicU32::new({
                let account_info: frame_system::AccountInfo<
                    <RuntimeImplForSigningExtrinsics as System>::Index,
                    <RuntimeImplForSigningExtrinsics as System>::AccountData,
                > = Decode::decode(
                    &mut &state_chain_rpc_client
                        .state_rpc_client
                        .storage(
                            system_pallet_metadata
                                .storage("Account")?
                                .map()?
                                .key(&our_account_id),
                            latest_block_hash,
                        )
                        .compat()
                        .await
                        .map_err(anyhow::Error::msg)?
                        .ok_or_else(|| {
                            anyhow::format_err!(
                                "AccountId {:?} doesn't exist on the state chain.",
                                our_account_id,
                            )
                        })?
                        .0[..],
                )?;
                account_info.nonce
            }),
            state_chain_rpc_client,
            our_account_id,
        }),
        chain_rpc_client
            .subscribe_finalized_heads() // TODO: We cannot control at what block this stream begins (Could be a problem)
            .compat()
            .await
            .map_err(anyhow::Error::msg)?
            .compat()
            .map(|result_header| result_header.map_err(anyhow::Error::msg)),
    ))
}

#[cfg(test)]
mod tests {

    use std::convert::TryInto;

    use crate::{logging::test_utils::new_test_logger, testing::assert_ok};

    use super::*;

    #[tokio::test]
    async fn nonce_increments_on_success() {
        let logger = new_test_logger();
        let bytes: [u8; 32] =
            hex::decode("276dabe5c09f607729280c91c3de2dc588cd0e6ccba24db90cae050d650b3fc3")
                .unwrap()
                .try_into()
                .unwrap();
        let tx_hash = H256::from(bytes);

        let mut mock_state_chain_rpc_client = MockStateChainRpcApi::new();
        mock_state_chain_rpc_client
            .expect_submit_extrinsic_rpc()
            .times(1)
            // with verifies it's call with the correct argument
            .returning(move |_nonce: u32, _call: state_chain_runtime::Call| Ok(tx_hash.clone()));

        let state_chain_client = StateChainClient {
            metadata: substrate_subxt::Metadata::default(),
            nonce: AtomicU32::new(0),
            our_account_id: AccountId32::new([0; 32]),
            state_chain_rpc_client: mock_state_chain_rpc_client,
        };

        let force_rotation_call: state_chain_runtime::Call =
            pallet_cf_governance::Call::propose_governance_extrinsic(Box::new(
                pallet_cf_validator::Call::force_rotation().into(),
            ))
            .into();

        assert_ok!(
            state_chain_client
                .submit_extrinsic(&logger, force_rotation_call)
                .await
        );

        assert_eq!(state_chain_client.nonce.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn tx_retried_and_nonce_incremented_on_fail_due_to_nonce_each_time() {
        let logger = new_test_logger();

        let mut mock_state_chain_rpc_client = MockStateChainRpcApi::new();
        mock_state_chain_rpc_client
            .expect_submit_extrinsic_rpc()
            .times(MAX_RETRY_ATTEMPTS)
            // with verifies it's call with the correct argument
            .returning(move |_nonce: u32, _call: state_chain_runtime::Call| {
                Err(RpcError::JsonRpcError(Error {
                    code: ErrorCode::ServerError(1014),
                    message: "Priority too low".to_string(),
                    data: None,
                }))
            });

        let state_chain_client = StateChainClient {
            metadata: substrate_subxt::Metadata::default(),
            nonce: AtomicU32::new(0),
            our_account_id: AccountId32::new([0; 32]),
            state_chain_rpc_client: mock_state_chain_rpc_client,
        };

        let force_rotation_call: state_chain_runtime::Call =
            pallet_cf_governance::Call::propose_governance_extrinsic(Box::new(
                pallet_cf_validator::Call::force_rotation().into(),
            ))
            .into();

        state_chain_client
            .submit_extrinsic(&logger, force_rotation_call)
            .await
            .unwrap_err();

        assert_eq!(state_chain_client.nonce.load(Ordering::Relaxed), 10);
    }

    #[tokio::test]
    async fn tx_fails_for_reason_unrelated_to_nonce_does_not_retry_does_not_increment_nonce() {
        let logger = new_test_logger();

        // Return a non-nonce related error, we submit two extrinsics that fail in the same way
        let mut mock_state_chain_rpc_client = MockStateChainRpcApi::new();
        mock_state_chain_rpc_client
            .expect_submit_extrinsic_rpc()
            .times(1)
            // with verifies it's call with the correct argument
            .returning(move |_nonce: u32, _call: state_chain_runtime::Call| Err(RpcError::Timeout));

        let state_chain_client = StateChainClient {
            metadata: substrate_subxt::Metadata::default(),
            nonce: AtomicU32::new(0),
            our_account_id: AccountId32::new([0; 32]),
            state_chain_rpc_client: mock_state_chain_rpc_client,
        };

        let force_rotation_call: state_chain_runtime::Call =
            pallet_cf_governance::Call::propose_governance_extrinsic(Box::new(
                pallet_cf_validator::Call::force_rotation().into(),
            ))
            .into();

        state_chain_client
            .submit_extrinsic(&logger, force_rotation_call.clone())
            .await
            .unwrap_err();

        assert_eq!(state_chain_client.nonce.load(Ordering::Relaxed), 0);
    }

    // 1. We submit a tx
    // 2. Tx fails with a nonce error, so we leave the nonce incremented
    // 3. We call again (incrementing the nonce for next time) nonce for this call is 1.
    // 4. We succeed, therefore the nonce for the next call is 2.
    #[tokio::test]
    async fn tx_fails_due_to_nonce_increments_nonce_then_exits_when_successful() {
        let logger = new_test_logger();

        let bytes: [u8; 32] =
            hex::decode("276dabe5c09f607729280c91c3de2dc588cd0e6ccba24db90cae050d650b3fc3")
                .unwrap()
                .try_into()
                .unwrap();
        let tx_hash = H256::from(bytes);

        // Return a non-nonce related error, we submit two extrinsics that fail in the same way
        let mut mock_state_chain_rpc_client = MockStateChainRpcApi::new();
        mock_state_chain_rpc_client
            .expect_submit_extrinsic_rpc()
            .times(1)
            // with verifies it's call with the correct argument
            .returning(move |_nonce: u32, _call: state_chain_runtime::Call| {
                Err(RpcError::JsonRpcError(Error {
                    code: ErrorCode::ServerError(1014),
                    message: "Priority too low".to_string(),
                    data: None,
                }))
            });

        mock_state_chain_rpc_client
            .expect_submit_extrinsic_rpc()
            .times(1)
            .returning(move |_nonce: u32, _call: state_chain_runtime::Call| Ok(tx_hash.clone()));

        let state_chain_client = StateChainClient {
            metadata: substrate_subxt::Metadata::default(),
            nonce: AtomicU32::new(0),
            our_account_id: AccountId32::new([0; 32]),
            state_chain_rpc_client: mock_state_chain_rpc_client,
        };

        let force_rotation_call: state_chain_runtime::Call =
            pallet_cf_governance::Call::propose_governance_extrinsic(Box::new(
                pallet_cf_validator::Call::force_rotation().into(),
            ))
            .into();

        assert_ok!(
            state_chain_client
                .submit_extrinsic(&logger, force_rotation_call.clone())
                .await
        );

        assert_eq!(state_chain_client.nonce.load(Ordering::Relaxed), 2);
    }
}
