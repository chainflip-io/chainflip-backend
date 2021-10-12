use anyhow::Result;
use codec::{Decode, Encode};
use frame_support::metadata::RuntimeMetadataPrefixed;
use frame_support::unsigned::TransactionValidityError;
use frame_system::Phase;
use futures::compat::{Future01CompatExt, Stream01CompatExt};
use futures::StreamExt;
use futures::{Stream, TryFutureExt};
use itertools::Itertools;
use sp_core::{
    storage::{StorageChangeSet, StorageKey},
    Bytes, Pair,
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

// Substrate_subxt's Runtime trait allows us to use it's extrinsic signing code
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

pub struct StateChainClient {
    pub metadata: substrate_subxt::Metadata,
    events_storage_key: StorageKey,
    runtime_version: sp_version::RuntimeVersion,
    genesis_hash: state_chain_runtime::Hash,
    nonce: Mutex<<RuntimeImplForSigningExtrinsics as System>::Index>,
    pub signer:
        substrate_subxt::PairSigner<RuntimeImplForSigningExtrinsics, sp_core::sr25519::Pair>,
    author_rpc_client: AuthorRpcClient,
    state_rpc_client: StateRpcClient,
}
impl StateChainClient {
    async fn inner_submit_extrinsic<Extrinsic>(
        &self,
        nonce: u32,
        extrinsic: Extrinsic,
    ) -> Result<sp_core::H256>
    where
        state_chain_runtime::Call: std::convert::From<Extrinsic>,
        Extrinsic: std::fmt::Debug + Clone,
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
                .await?
                .encode(),
            ))
            .compat()
            .map_err(anyhow::Error::msg)
            .await
    }

    pub async fn submit_extrinsic<Extrinsic>(&self, logger: &slog::Logger, extrinsic: Extrinsic)
    where
        state_chain_runtime::Call: std::convert::From<Extrinsic>,
        Extrinsic: std::fmt::Debug + Clone,
    {
        slog::trace!(logger, "Submitting extrinsic: {:?}", extrinsic);
        let mut nonce = self.nonce.lock().await;

        match self.inner_submit_extrinsic(*nonce, extrinsic.clone()).await {
            Ok(_) => *nonce += 1,
            Err(error) => slog::error!(
                logger,
                "Could not submit extrinsic: {:?}, {}",
                extrinsic,
                error
            ),
        }
    }
    pub async fn events(
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

#[allow(clippy::eval_order_dependence)]
pub async fn connect_to_state_chain(
    settings: &settings::Settings,
) -> Result<(
    Arc<StateChainClient>,
    impl Stream<Item = Result<state_chain_runtime::Header>>,
)> {
    use substrate_subxt::Signer;
    let signer = substrate_subxt::PairSigner::<
        RuntimeImplForSigningExtrinsics,
        sp_core::sr25519::Pair,
    >::new(sp_core::sr25519::Pair::from_seed(
        &(<[u8; 32]>::try_from(
            hex::decode(
                &std::fs::read_to_string(&settings.state_chain.signing_key_file)?.replace("\"", ""),
            )
            .map_err(anyhow::Error::new)?,
        )
        .map_err(|_err| anyhow::Error::msg("Signing key seed is the wrong length."))?),
    ));

    let rpc_server_url = &url::Url::parse(settings.state_chain.ws_endpoint.as_str())?;

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

    let metadata = substrate_subxt::Metadata::try_from(RuntimeMetadataPrefixed::decode(
        &mut &state_rpc_client
            .metadata(None)
            .compat()
            .await
            .map_err(anyhow::Error::msg)?[..],
    )?)?;

    let system_pallet_metadata = metadata.module("System")?;

    Ok((
        Arc::new(StateChainClient {
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
                            system_pallet_metadata
                                .storage("Account")?
                                .map()?
                                .key(&signer.account_id()),
                            None,
                        )
                        .compat()
                        .await
                        .map_err(anyhow::Error::msg)?
                        .ok_or_else(|| anyhow::Error::msg("Account doesn't exist"))?
                        .0[..],
                )?;
                account_info.nonce
            }),
            author_rpc_client,
            state_rpc_client,
            events_storage_key: system_pallet_metadata.storage("Events")?.prefix(),
            metadata,
            signer,
        }),
        chain_rpc_client
            .subscribe_finalized_heads()
            .compat()
            .await
            .map_err(anyhow::Error::msg)?
            .compat()
            .map(|result_header| result_header.map_err(anyhow::Error::msg)),
    ))
}
