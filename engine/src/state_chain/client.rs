use anyhow::Result;
use cf_chains::ChainId;
use cf_traits::{ChainflipAccountData, EpochIndex};
use codec::{Decode, Encode};
use frame_support::metadata::RuntimeMetadataPrefixed;
use frame_support::unsigned::TransactionValidityError;
use frame_system::{AccountInfo, Phase};
use futures::{Stream, StreamExt, TryStreamExt};
use jsonrpc_core::{Error, ErrorCode};
use jsonrpc_core_client::RpcError;
use pallet_cf_vaults::Vault;
use sp_core::H256;
use sp_core::{
    storage::{StorageChangeSet, StorageKey},
    Bytes, Pair,
};
use sp_runtime::generic::Era;
use sp_runtime::traits::{BlakeTwo256, Hash};
use sp_runtime::AccountId32;
use state_chain_runtime::{Index, SignedBlock};
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

use crate::common::into_anyhow_error;
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

    async fn storage_events_at(
        &self,
        block_hash: Option<state_chain_runtime::Hash>,
        storage_key: StorageKey,
    ) -> Result<Vec<StorageChangeSet<state_chain_runtime::Hash>>>;

    async fn get_block(&self, block_hash: state_chain_runtime::Hash)
        -> Result<Option<SignedBlock>>;

    async fn latest_block_hash(&self) -> Result<state_chain_runtime::Hash>;
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
            .await
    }

    async fn get_block(
        &self,
        block_hash: state_chain_runtime::Hash,
    ) -> Result<Option<SignedBlock>> {
        self.chain_rpc_client
            .block(Some(block_hash))
            .await
            .map_err(into_anyhow_error)
    }

    async fn storage_events_at(
        &self,
        block_hash: Option<state_chain_runtime::Hash>,
        storage_key: StorageKey,
    ) -> Result<Vec<StorageChangeSet<state_chain_runtime::Hash>>> {
        self.state_rpc_client
            .query_storage_at(vec![storage_key], block_hash)
            .await
            .map_err(into_anyhow_error)
    }

    async fn latest_block_hash(&self) -> Result<state_chain_runtime::Hash> {
        try_unwrap_value(
            self.chain_rpc_client
                .block_hash(None)
                .await
                .map_err(into_anyhow_error)?,
            anyhow::Error::msg("Failed to get latest block hash"),
        )
    }
}

pub struct StateChainClient<RpcClient: StateChainRpcApi> {
    metadata: substrate_subxt::Metadata,
    account_storage_key: StorageKey,
    events_storage_key: StorageKey,
    nonce: AtomicU32,
    /// Our Node's AccountId
    pub our_account_id: AccountId32,

    state_chain_rpc_client: RpcClient,
}

impl<RpcClient: StateChainRpcApi> StateChainClient<RpcClient> {
    /// Get the latest block hash at the time of the call
    pub async fn get_latest_block_hash(&self) -> Result<state_chain_runtime::Hash> {
        self.state_chain_rpc_client.latest_block_hash().await
    }

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
                        return Err(into_anyhow_error(err));
                    }
                },
            }
        }
        slog::error!(logger, "Exceeded maximum number of retry attempts");
        Err(anyhow::Error::msg(
            "Exceeded maximum number of retry attempts",
        ))
    }

    /// Watches *only* submitted extrinsics. I.e. Cannot watch for chain called extrinsics.
    pub async fn watch_submitted_extrinsic<BlockStream>(
        &self,
        extrinsic_hash: state_chain_runtime::Hash,
        block_stream: &mut BlockStream,
    ) -> Result<Vec<state_chain_runtime::Event>>
    where
        BlockStream:
            Stream<Item = anyhow::Result<state_chain_runtime::Header>> + Unpin + Send + 'static,
    {
        while let Some(result_header) = block_stream.next().await {
            let header = result_header?;
            let block_hash = header.hash();
            if let Some(signed_block) = self.state_chain_rpc_client.get_block(block_hash).await? {
                match signed_block.block.extrinsics.iter().position(|ext| {
                    let hash = BlakeTwo256::hash_of(ext);
                    hash == extrinsic_hash
                }) {
                    Some(extrinsic_index_found) => {
                        let events_for_block = self.get_events(block_hash).await?;
                        return Ok(events_for_block
                            .into_iter()
                            .filter_map(|(phase, event, _)| {
                                if let Phase::ApplyExtrinsic(i) = phase {
                                    if i as usize != extrinsic_index_found {
                                        None
                                    } else {
                                        Some(event)
                                    }
                                } else {
                                    None
                                }
                            })
                            .collect::<Vec<_>>());
                    }
                    None => continue,
                }
            };
        }
        Err(anyhow::Error::msg(
            "Block stream loop exited, no event found",
        ))
    }

    async fn get_from_storage_with_key<StorageType: Decode + Debug>(
        &self,
        block_hash: state_chain_runtime::Hash,
        storage_key: StorageKey,
    ) -> Result<Vec<StorageType>> {
        let storage_updates: Vec<_> = self
            .state_chain_rpc_client
            .storage_events_at(Some(block_hash), storage_key)
            .await?
            .into_iter()
            .map(|storage_change_set| {
                let StorageChangeSet { block: _, changes } = storage_change_set;
                changes
                    .into_iter()
                    .filter_map(|(_storage_key, option_data)| {
                        option_data.map(|data| {
                            StorageType::decode(&mut &data.0[..]).map_err(anyhow::Error::msg)
                        })
                    })
            })
            .flatten()
            .collect::<Result<_>>()?;

        Ok(storage_updates)
    }

    // TODO: work out how to get all vaults with a single query... not sure if possible
    pub async fn get_vault(
        &self,
        block_hash: state_chain_runtime::Hash,
        epoch_index: EpochIndex,
        chain_id: ChainId,
    ) -> Result<Vault> {
        let vault_for_epoch_key = self
            .get_metadata()
            .module("Vaults")?
            .storage("Vaults")?
            .double_map()?
            .key(&epoch_index, &chain_id);

        let vaults = self
            .get_from_storage_with_key::<Vault>(block_hash, vault_for_epoch_key)
            .await?;

        Ok(vaults.last().expect("should have a vault").to_owned())
    }

    pub async fn get_environment_value<ValueType: Debug + Decode + Clone>(
        &self,
        block_hash: state_chain_runtime::Hash,
        value: &'static str,
    ) -> Result<ValueType> {
        let value_key = self
            .get_metadata()
            .module("Environment")?
            .storage(value)?
            .plain()?
            .key();
        let value_changes = self
            .get_from_storage_with_key::<ValueType>(block_hash, value_key)
            .await?;

        Ok(value_changes
            .last()
            .expect("Failed to find value in environment storage")
            .to_owned())
    }

    /// Get all the events from a particular block
    pub async fn get_events(
        &self,
        block_hash: state_chain_runtime::Hash,
    ) -> Result<Vec<EventInfo>> {
        let events = self
            .get_from_storage_with_key::<Vec<EventInfo>>(
                block_hash,
                self.events_storage_key.clone(),
            )
            .await?;
        if let Some(events) = events.last() {
            Ok(events.to_owned())
        } else {
            Ok(vec![])
        }
    }

    /// Get the status of the node at a particular block
    pub async fn get_account_data(
        &self,
        block_hash: state_chain_runtime::Hash,
    ) -> Result<ChainflipAccountData> {
        let account_info = self
            .get_from_storage_with_key::<AccountInfo<Index, ChainflipAccountData>>(
                block_hash,
                self.account_storage_key.clone(),
            )
            .await?;

        Ok(account_info
            .last()
            .expect("should have account data")
            .to_owned()
            .data)
    }

    /// Get the epoch number of the latest block
    pub async fn epoch_at_block(
        &self,
        block_hash: state_chain_runtime::Hash,
    ) -> Result<EpochIndex> {
        let epoch_storage_key = self
            .get_metadata()
            .module("Validator")?
            .storage("CurrentEpoch")?
            .plain()?
            .key();
        let epoch = self
            .get_from_storage_with_key::<EpochIndex>(block_hash, epoch_storage_key)
            .await?;

        Ok(epoch.last().expect("should have epoch").to_owned())
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

fn try_unwrap_value<T, E>(lorv: sp_rpc::list::ListOrValue<Option<T>>, error: E) -> Result<T, E> {
    match lorv {
        sp_rpc::list::ListOrValue::Value(Some(value)) => Ok(value),
        _ => Err(error),
    }
}

#[allow(clippy::eval_order_dependence)]
pub async fn connect_to_state_chain(
    state_chain_settings: &settings::StateChain,
) -> Result<(
    Arc<StateChainClient<StateChainRpcClient>>,
    impl Stream<Item = Result<state_chain_runtime::Header>>,
    H256,
)> {
    use substrate_subxt::Signer;
    let signer = substrate_subxt::PairSigner::<
        RuntimeImplForSigningExtrinsics,
        sp_core::sr25519::Pair,
    >::new(sp_core::sr25519::Pair::from_seed(
        &(<[u8; 32]>::try_from(
            hex::decode(
                &std::fs::read_to_string(&state_chain_settings.signing_key_file)?
                    .replace("\"", "")
                    // allow inserting the private key with or without the 0x
                    .replace("0x", ""),
            )
            .map_err(anyhow::Error::new)?,
        )
        .map_err(|_err| anyhow::Error::msg("Signing key seed is the wrong length."))?),
    ));

    let rpc_server_url = &url::Url::parse(state_chain_settings.ws_endpoint.as_str())?;

    // TODO connect only once (Using a single RpcChannel)

    let author_rpc_client =
        jsonrpc_core_client::transports::ws::connect::<AuthorRpcClient>(rpc_server_url)
            .await
            .map_err(into_anyhow_error)?;

    let chain_rpc_client =
        jsonrpc_core_client::transports::ws::connect::<ChainRpcClient>(rpc_server_url)
            .await
            .map_err(into_anyhow_error)?;

    let state_rpc_client =
        jsonrpc_core_client::transports::ws::connect::<StateRpcClient>(rpc_server_url)
            .await
            .map_err(into_anyhow_error)?;

    let latest_block_hash = try_unwrap_value(
        chain_rpc_client
            .block_hash(None)
            .await
            .map_err(into_anyhow_error)?,
        anyhow::Error::msg("Failed to get latest block hash"),
    )?;

    let metadata = substrate_subxt::Metadata::try_from(RuntimeMetadataPrefixed::decode(
        &mut &state_rpc_client
            .metadata(Some(latest_block_hash))
            .await
            .map_err(into_anyhow_error)?[..],
    )?)?;

    let system_pallet_metadata = metadata.module("System")?.clone();
    let state_chain_rpc_client = StateChainRpcClient {
        runtime_version: state_rpc_client
            .runtime_version(Some(latest_block_hash))
            .await
            .map_err(into_anyhow_error)?,
        genesis_hash: try_unwrap_value(
            chain_rpc_client
                .block_hash(Some(sp_rpc::number::NumberOrHex::from(0u64).into()))
                .await
                .map_err(into_anyhow_error)?,
            anyhow::Error::msg("Genesis block doesn't exist?"),
        )?,
        signer: signer.clone(),
        author_rpc_client,
        state_rpc_client,
        chain_rpc_client: chain_rpc_client.clone(),
    };

    let our_account_id = signer.account_id().to_owned();

    let account_storage_key = system_pallet_metadata
        .storage("Account")?
        .map()?
        .key(&our_account_id);

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
                        .storage(account_storage_key.clone(), Some(latest_block_hash))
                        .await
                        .map_err(into_anyhow_error)?
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
            account_storage_key,
            events_storage_key: system_pallet_metadata.clone().storage("Events")?.prefix(),
        }),
        chain_rpc_client
            .subscribe_finalized_heads() // TODO: We cannot control at what block this stream begins (Could be a problem)
            .map_err(into_anyhow_error)?
            .map_err(into_anyhow_error),
        latest_block_hash,
    ))
}

#[cfg(test)]
mod tests {

    use std::convert::TryInto;

    use sp_core::H160;

    use crate::{logging::test_utils::new_test_logger, settings::Settings, testing::assert_ok};

    use super::*;

    #[ignore = "depends on running state chain, and a configured Local.toml file"]
    #[tokio::main]
    #[test]
    async fn test_finalised_storage_subs() {
        let settings = Settings::from_file("config/Local.toml").unwrap();
        let (state_chain_client, mut block_stream, _) =
            connect_to_state_chain(&settings.state_chain).await.unwrap();

        println!("My account id is: {}", state_chain_client.our_account_id);

        while let Some(block) = block_stream.next().await {
            let block_header = block.unwrap();
            let block_hash = block_header.hash();
            let block_number = block_header.number;
            println!(
                "Getting events from block {} with block_hash: {:?}",
                block_number, block_hash
            );
            let my_state_for_this_block = state_chain_client
                .get_environment_value::<H160>(block_hash, "KeyManagerAddress")
                .await
                .unwrap();

            println!(
                "Returning AccountData for this block: {:?}",
                my_state_for_this_block
            );
        }
    }

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
            account_storage_key: StorageKey(Vec::default()),
            events_storage_key: StorageKey(Vec::default()),
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
            account_storage_key: StorageKey(Vec::default()),
            events_storage_key: StorageKey(Vec::default()),
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
            account_storage_key: StorageKey(Vec::default()),
            events_storage_key: StorageKey(Vec::default()),
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
            account_storage_key: StorageKey(Vec::default()),
            events_storage_key: StorageKey(Vec::default()),
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
