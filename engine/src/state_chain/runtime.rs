use std::marker::PhantomData;

use frame_support::unsigned::TransactionValidityError;
use sp_runtime::{
    generic::{self, Era},
    traits::{BlakeTwo256, IdentifyAccount, Verify},
    MultiSignature, OpaqueExtrinsic,
};
use substrate_subxt::{
    extrinsic::{
        CheckEra, CheckGenesis, CheckNonce, CheckSpecVersion, CheckTxVersion, CheckWeight,
        DefaultExtra,
    },
    register_default_type_sizes,
    system::System,
    EventTypeRegistry, Runtime, SignedExtension, SignedExtra,
};

use core::fmt::Debug;

use codec::{Decode, Encode};

use super::{staking, validator};

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
        // event_type_registry.with_session();
        // event_type_registry.with_sudo();
        register_default_type_sizes(event_type_registry);
    }
}

impl validator::Validator for StateChainRuntime {
    type EpochIndex = u32;
}

impl staking::Staking for StateChainRuntime {
    type TokenAmount = u128;

    type EthereumAddress = [u8; 20];

    type Nonce = u64;
}

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
