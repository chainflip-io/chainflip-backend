use sp_runtime::{
    generic,
    traits::{BlakeTwo256, IdentifyAccount, Verify},
    MultiSignature, OpaqueExtrinsic,
};
use substrate_subxt::system::System;

use super::{stake_manager, system, validator};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct StateChainRuntime;

impl Runtime for StateChainRuntime {
    type Signature = MultiSignature;
    type Extra = DefaultExtra<Self>;

    fn register_type_sizes(event_type_registry: &mut EventTypeRegistry<Self>) {
        event_type_registry.with_balances();
        event_type_registry.with_session();
        event_type_registry.with_sudo();
        register_default_type_sizes(event_type_registry);
    }
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
