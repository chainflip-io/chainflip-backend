use std::{marker::PhantomData, time::Duration};

use frame_support::unsigned::TransactionValidityError;
use sp_runtime::{
    generic::{self, Era},
    traits::{BlakeTwo256, IdentifyAccount, Verify},
    AccountId32, MultiSignature, OpaqueExtrinsic,
};
use substrate_subxt::{
    extrinsic::{
        CheckEra, CheckGenesis, CheckNonce, CheckSpecVersion, CheckTxVersion, CheckWeight,
    },
    register_default_type_sizes,
    sudo::{Sudo, SudoEventTypeRegistry},
    system::{System, SystemEventTypeRegistry},
    EventTypeRegistry, Runtime, SignedExtension, SignedExtra,
};

use crate::state_chain::pallets::auction::AuctionEventTypeRegistry;
use crate::state_chain::pallets::emissions::EmissionsEventTypeRegistry;
use crate::state_chain::pallets::staking::StakingEventTypeRegistry;
use crate::state_chain::pallets::validator::ValidatorEventTypeRegistry;

use core::fmt::Debug;

use codec::{Decode, Encode};

use super::pallets::{auction, emissions, reputation, staking, validator, vaults, witness_api};

use pallet_cf_flip::ImbalanceSource;
use pallet_cf_reputation::OfflineCondition;
use pallet_cf_vaults::{KeygenRequest, ThresholdSignatureRequest, VaultRotationRequest};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct StateChainRuntime;

/// Default `SignedExtra` for the state chain runtimes.
#[derive(Encode, Decode, Clone, Eq, PartialEq, Debug)]
pub struct SCDefaultExtra<T: System> {
    spec_version: u32,
    tx_version: u32,
    nonce: T::Index,
    genesis_hash: T::Hash,
}

impl<T: System + Clone + Debug + Eq + Send + Sync> SignedExtra<T> for SCDefaultExtra<T> {
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

impl<T: System + Clone + Debug + Eq + Send + Sync> SignedExtension for SCDefaultExtra<T> {
    const IDENTIFIER: &'static str = "SCDefaultExtra";
    type AccountId = T::AccountId;
    type Call = ();
    type AdditionalSigned = <<Self as SignedExtra<T>>::Extra as SignedExtension>::AdditionalSigned;
    type Pre = ();

    fn additional_signed(&self) -> Result<Self::AdditionalSigned, TransactionValidityError> {
        self.extra().additional_signed()
    }
}

impl Runtime for StateChainRuntime {
    type Signature = MultiSignature;
    type Extra = SCDefaultExtra<Self>;

    fn register_type_sizes(event_type_registry: &mut EventTypeRegistry<Self>) {
        event_type_registry.with_system();
        event_type_registry.with_sudo();
        register_default_type_sizes(event_type_registry);

        // custom pallet event type registries
        event_type_registry.with_validator();
        event_type_registry.with_staking();
        event_type_registry.with_auction();
        event_type_registry.with_emissions();

        event_type_registry.register_type_size::<AccountId32>("AccountId<T>");
        event_type_registry.register_type_size::<u64>("T::AuctionIndex");
        event_type_registry.register_type_size::<(u32, u32)>("AuctionRange");
        event_type_registry.register_type_size::<u64>("T::Nonce");
        event_type_registry.register_type_size::<u64>("T::EpochIndex");
        event_type_registry.register_type_size::<u32>("T::BlockNumber");
        event_type_registry.register_type_size::<u32>("BlockNumberFor<T>");
        event_type_registry.register_type_size::<AccountId32>("T::AccountId");
        event_type_registry.register_type_size::<AccountId32>("<T as Config>::AccountId");
        event_type_registry.register_type_size::<u128>("T::Balance");
        event_type_registry.register_type_size::<Vec<u8>>("OpaqueTimeSlot");
        event_type_registry.register_type_size::<[u8; 32]>("U256");
        event_type_registry.register_type_size::<Duration>("Duration");
        event_type_registry.register_type_size::<u128>("FlipBalance<T>");
        event_type_registry.register_type_size::<u128>("T::FlipBalance");
        event_type_registry.register_type_size::<u32>("VoteCount");
        event_type_registry.register_type_size::<u32>("SessionIndex");
        event_type_registry.register_type_size::<[u8; 32]>("AggKeySignature");
        event_type_registry
            .register_type_size::<ImbalanceSource<AccountId32>>("ImbalanceSource<T::AccountId>");
        event_type_registry.register_type_size::<i32>("ReputationPoints");
        event_type_registry.register_type_size::<u32>("OnlineCreditsFor<T>");

        event_type_registry.register_type_size::<u64>("RequestIndex");
        event_type_registry.register_type_size::<OfflineCondition>("OfflineCondition");
        event_type_registry.register_type_size::<KeygenRequest<AccountId32>>(
            "KeygenRequest<T::AccountId, T::PublicKey>",
        );
        event_type_registry.register_type_size::<Vec<u8>>("T::PublicKey");
        event_type_registry.register_type_size::<ThresholdSignatureRequest<Vec<u8>, AccountId32>>(
            "ThresholdSignatureRequest<T::PublicKey, T::AccountId>",
        );
        event_type_registry.register_type_size::<VaultRotationRequest>("VaultRotationRequest");
    }
}

impl Sudo for StateChainRuntime {}

impl auction::Auction for StateChainRuntime {
    type AuctionIndex = u64;
}

impl validator::Validator for StateChainRuntime {
    type EpochIndex = u32;
}

impl staking::Staking for StateChainRuntime {
    type TokenAmount = u128;

    type EthereumAddress = [u8; 20];

    type Nonce = u64;
}

impl witness_api::WitnesserApi for StateChainRuntime {}

impl emissions::Emissions for StateChainRuntime {
    type FlipBalance = u128;
}

impl vaults::Vaults for StateChainRuntime {}

impl reputation::Reputation for StateChainRuntime {}

impl System for StateChainRuntime {
    type Index = u32;

    type BlockNumber = u32;

    type Hash = sp_core::H256;

    type Hashing = BlakeTwo256;

    type AccountId = <<MultiSignature as Verify>::Signer as IdentifyAccount>::AccountId;

    type Address = sp_runtime::MultiAddress<Self::AccountId, ()>;

    type Header = generic::Header<Self::BlockNumber, BlakeTwo256>;

    // Not sure on this one - grabbed from subxt
    type Extrinsic = OpaqueExtrinsic;

    type AccountData = ();
}
