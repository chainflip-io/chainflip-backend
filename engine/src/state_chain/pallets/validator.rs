// Implements support for the validator module

use std::marker::PhantomData;

use crate::state_chain::{runtime::StateChainRuntime, sc_event::SCEvent};
use codec::{Decode, Encode};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use substrate_subxt::{module, sp_runtime::traits::Member, Call, Event};

#[module]
pub trait Validator: substrate_subxt::system::System {
    type EpochIndex: Member + Encode + Decode + Serialize + DeserializeOwned;
}

#[derive(Clone, Debug, Eq, PartialEq, Call, Decode, Encode, Serialize, Deserialize)]
pub struct ForceRotationCall<T: Validator> {
    /// Runtime marker
    _runtime: PhantomData<T>,
}

#[derive(Clone, Debug, Eq, PartialEq, Event, Decode, Encode, Serialize, Deserialize)]
pub struct EpochDurationChangedEvent<V: Validator> {
    pub from: V::BlockNumber,
    pub to: V::BlockNumber,
}

#[derive(Clone, Debug, Eq, PartialEq, Event, Decode, Encode, Serialize, Deserialize)]
pub struct ForceRotationRequestedEvent<V: Validator> {
    pub _phantom: PhantomData<V>,
}

#[derive(Clone, Debug, Eq, PartialEq, Event, Decode, Encode, Serialize, Deserialize)]
pub struct NewEpochEvent<V: Validator> {
    pub epoch_index: V::EpochIndex,
}

/// Derives an enum for the listed events and corresponding implementations of `From`.
macro_rules! impl_validator_event_enum {
    ( $( $name:tt ),+ ) => {
        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
        pub enum ValidatorEvent<Runtime: Validator> {
            $(
                $name($name<Runtime>),
            )+
        }

        $(
            impl From<$name<StateChainRuntime>> for SCEvent {
                fn from(staking_event: $name<StateChainRuntime>) -> Self {
                    SCEvent::ValidatorEvent(ValidatorEvent::$name(staking_event))
                }
            }
        )+
    };
}

impl_validator_event_enum!(
    EpochDurationChangedEvent,
    ForceRotationRequestedEvent,
    NewEpochEvent
);

#[cfg(test)]
mod tests {

    use pallet_cf_validator::Config;

    use codec::Encode;
    use state_chain_runtime::Runtime as SCRuntime;

    use crate::state_chain::runtime::StateChainRuntime;

    use super::*;

    #[test]
    fn epoch_changed_decoding() {
        let event: <SCRuntime as Config>::Event =
            pallet_cf_validator::Event::<SCRuntime>::EpochDurationChanged(4, 10).into();

        let encoded_epoch = event.encode();
        // the first 2 bytes are (module_index, event_variant_index), these can be stripped
        let encoded_epoch = encoded_epoch[2..].to_vec();

        let decoded_event =
            EpochDurationChangedEvent::<StateChainRuntime>::decode(&mut &encoded_epoch[..])
                .unwrap();

        let expecting = EpochDurationChangedEvent { from: 4, to: 10 };

        assert_eq!(decoded_event, expecting);
    }

    #[test]
    fn new_epoch_decoding() {
        // AuctionConfirmed(EpochIndex)
        let event: <SCRuntime as Config>::Event =
            pallet_cf_validator::Event::<SCRuntime>::NewEpoch(1).into();

        let encoded_new_epoch = event.encode();
        // the first 2 bytes are (module_index, event_variant_index), these can be stripped
        let encoded_new_epoch = encoded_new_epoch[2..].to_vec();

        let decoded_event =
            NewEpochEvent::<StateChainRuntime>::decode(&mut &encoded_new_epoch[..]).unwrap();

        let expecting = NewEpochEvent { epoch_index: 1 };

        assert_eq!(decoded_event, expecting);
    }

    #[test]
    fn force_rotation_requested_decoding() {
        let event: <SCRuntime as Config>::Event =
            pallet_cf_validator::Event::<SCRuntime>::ForceRotationRequested().into();

        let encodeded_force_rotation = event.encode();
        // the first 2 bytes are (module_index, event_variant_index), these can be stripped
        let encodeded_force_rotation = encodeded_force_rotation[2..].to_vec();

        let decoded_event = ForceRotationRequestedEvent::<StateChainRuntime>::decode(
            &mut &encodeded_force_rotation[..],
        )
        .unwrap();

        let expecting = ForceRotationRequestedEvent {
            _phantom: PhantomData,
        };

        assert_eq!(decoded_event, expecting);
    }
}
