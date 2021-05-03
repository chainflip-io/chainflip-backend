use serde::Serialize;
use substrate_subxt::{
    balances::{AccountData, Balances},
    extrinsic::DefaultExtra,
    register_default_type_sizes,
    session::Session,
    sp_runtime::{
        self,
        generic::Header,
        traits::{BlakeTwo256, IdentifyAccount, Verify},
        MultiSignature,
    },
    sudo::Sudo,
    system::System,
    BasicSessionKeys, EventTypeRegistry, Runtime,
};

use substrate_subxt::{
    balances::BalancesEventTypeRegistry, session::SessionEventTypeRegistry,
    sudo::SudoEventTypeRegistry, system::SystemEventTypeRegistry,
};

use substrate_subxt::sp_runtime::OpaqueExtrinsic;

use super::{staking, validator};

// Runtime template for use in decoding by subxt

/// Concrete type definitions compatible with the state chain node
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct StateChainRuntime;

impl Runtime for StateChainRuntime {
    type Signature = MultiSignature;
    type Extra = DefaultExtra<Self>;

    fn register_type_sizes(event_type_registry: &mut EventTypeRegistry<Self>) {
        event_type_registry.with_system();
        event_type_registry.with_session();
        event_type_registry.with_sudo();
        event_type_registry.with_balances();
        register_default_type_sizes(event_type_registry);

        // Add any custom types here...
        event_type_registry.register_type_size::<<Self as System>::BlockNumber>("EpochIndex");

        // This doesn't seem the correct way to do this, but it works :shrug:
        event_type_registry.register_type_size::<u32>("T::BlockNumber");
    }
}

impl System for StateChainRuntime {
    type Index = u32;
    type BlockNumber = u32;
    type Hash = sp_core::H256;
    type Hashing = BlakeTwo256;
    type AccountId = <<MultiSignature as Verify>::Signer as IdentifyAccount>::AccountId;
    type Address = sp_runtime::MultiAddress<Self::AccountId, u32>;
    type Header = Header<Self::BlockNumber, BlakeTwo256>;
    type Extrinsic = OpaqueExtrinsic;
    type AccountData = AccountData<<Self as Balances>::Balance>;
}

impl Balances for StateChainRuntime {
    type Balance = u128;
}

impl Session for StateChainRuntime {
    type ValidatorId = <Self as System>::AccountId;
    type Keys = BasicSessionKeys;
}

impl Sudo for StateChainRuntime {}

// Must implement the custom events for the runtime

impl staking::Staking for StateChainRuntime {}

impl validator::Validator for StateChainRuntime {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_register_state_chain_runtime() {
        EventTypeRegistry::<StateChainRuntime>::new();
    }
}
