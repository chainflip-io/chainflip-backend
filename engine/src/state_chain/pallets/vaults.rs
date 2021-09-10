use std::marker::PhantomData;

use codec::{Decode, Encode};
use pallet_cf_vaults::{
    rotation::VaultRotationResponse, CeremonyId, KeygenRequest, KeygenResponse, SchnorrSignature,
    ThresholdSignatureRequest, ThresholdSignatureResponse, VaultRotationRequest,
};
use sp_runtime::AccountId32;
use substrate_subxt::{module, system::System, Call, Event};

use crate::state_chain::{runtime::StateChainRuntime, sc_event::SCEvent};

#[module]
pub trait Vaults: System {}

// ===== Events =====

// The order of these fields matter for decoding
#[derive(Clone, Debug, Eq, PartialEq, Event, Decode, Encode)]
pub struct KeygenRequestEvent<V: Vaults> {
    pub ceremony_id: CeremonyId,

    pub keygen_request: KeygenRequest<AccountId32>,

    pub _runtime: PhantomData<V>,
}

// The order of these fields matter for decoding
#[derive(Clone, Debug, Eq, PartialEq, Event, Decode, Encode)]
pub struct ThresholdSignatureRequestEvent<V: Vaults> {
    pub ceremony_id: CeremonyId,

    pub threshold_signature_request: ThresholdSignatureRequest<Vec<u8>, AccountId32>,

    pub _runtime: PhantomData<V>,
}

// The order of these fields matter for decoding
#[derive(Clone, Debug, Eq, PartialEq, Event, Decode, Encode)]
pub struct VaultRotationRequestEvent<V: Vaults> {
    pub ceremony_id: CeremonyId,

    pub vault_rotation_request: VaultRotationRequest,

    pub _runtime: PhantomData<V>,
}

// ===== Calls / Extrinsics =====

#[derive(Clone, Debug, PartialEq, Call, Encode)]
pub struct KeygenResponseCall<T: Vaults> {
    pub ceremony_id: CeremonyId,

    pub response: KeygenResponse<AccountId32, Vec<u8>>,

    pub _runtime: PhantomData<T>,
}

#[derive(Clone, Debug, PartialEq, Call, Encode)]
pub struct ThresholdSignatureResponseCall<T: Vaults> {
    pub ceremony_id: CeremonyId,

    pub response: ThresholdSignatureResponse<AccountId32, SchnorrSignature>,

    pub _runtime: PhantomData<T>,
}

#[derive(Clone, Debug, PartialEq, Call, Encode)]
pub struct VaultRotationResponseCall<T: Vaults> {
    pub ceremony_id: CeremonyId,

    pub response: VaultRotationResponse<Vec<u8>>,

    pub _runtime: PhantomData<T>,
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

impl_vaults_event_enum!(
    KeygenRequestEvent,
    ThresholdSignatureRequestEvent,
    VaultRotationRequestEvent
);
