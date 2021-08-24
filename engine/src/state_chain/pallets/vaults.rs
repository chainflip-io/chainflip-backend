use std::marker::PhantomData;

use codec::{Decode, Encode};
use pallet_cf_vaults::KeygenRequest;
use sp_runtime::AccountId32;
use substrate_subxt::{module, system::System, Event};

use crate::state_chain::{runtime::StateChainRuntime, sc_event::SCEvent};

#[module]
pub trait Vaults: System {}

// The order of these fields matter for decoding
#[derive(Clone, Debug, Eq, PartialEq, Event, Decode, Encode)]
pub struct KeygenRequestEvent<V: Vaults> {
    pub request_index: u64,

    pub keygen_request: KeygenRequest<AccountId32>,

    pub _runtime: PhantomData<V>,
}

/// Derives an enum for the listed events and corresponding implementations of `From`.
macro_rules! impl_vaults_event_enum {
    ( $( $name:tt ),+ ) => {
        #[derive(Debug, Clone, PartialEq)]
        pub enum VaultsEvent<V: Vaults> {
            $(
                $name($name<V>),
            )+
        }

        $(
            impl From<$name<StateChainRuntime>> for SCEvent {
                fn from(vaults_event: $name<StateChainRuntime>) -> Self {
                    SCEvent::VaultsEvent(VaultsEvent::$name(vaults_event))
                }
            }
        )+
    };
}

impl_vaults_event_enum!(KeygenRequestEvent);
