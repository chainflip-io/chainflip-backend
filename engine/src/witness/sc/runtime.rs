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

use substrate_subxt::balances::BalancesEventTypeRegistry;
use substrate_subxt::session::SessionEventTypeRegistry;
use substrate_subxt::sudo::SudoEventTypeRegistry;

use substrate_subxt::sp_runtime::OpaqueExtrinsic;

use super::{staking, transactions};

// use substrate_subxt::system::SystemEventTypeRegistry;

// Runtime template for use in decoding

/// Concrete type definitions compatible with the state chain node
///
/// # Note
///
/// Main difference is `type Address = AccountId`.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct StateChainRuntime;

impl Runtime for StateChainRuntime {
    type Signature = MultiSignature;
    type Extra = DefaultExtra<Self>;

    fn register_type_sizes(event_type_registry: &mut EventTypeRegistry<Self>) {
        event_type_registry.with_session();
        event_type_registry.with_sudo();
        event_type_registry.with_balances();
        register_default_type_sizes(event_type_registry);

        // TODO Add any custom stuff here
        event_type_registry.register_type_size::<u32>("AccountId32");
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

impl transactions::Transactions for StateChainRuntime {}

impl staking::Staking for StateChainRuntime {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_register_state_chain_runtime() {
        EventTypeRegistry::<StateChainRuntime>::new();
    }
}
