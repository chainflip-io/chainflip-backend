// Implements support for the validator module

use std::marker::PhantomData;

use codec::{Decode, Encode};
use pallet_cf_validator::ValidatorSize;
use serde::{Deserialize, Serialize};
use substrate_subxt::{module, system::System, Event};

use super::{runtime::StateChainRuntime, sc_event::SCEvent};

/// The Epoch index will never exceed the max value of a u32.
///
/// Defining this here avoids having to derive `Serialize` and `Deserialize` on the EpochIndex wrapper type defined in 
/// the pallet. 
type EpochIndex = u32;

#[module]
pub trait Validator: System {}

#[derive(Clone, Debug, Eq, PartialEq, Event, Decode, Encode, Serialize, Deserialize)]
pub struct MaximumValidatorsChangedEvent<V: Validator> {
    pub before: ValidatorSize,
    pub now: ValidatorSize,
    pub _phantom: PhantomData<V>,
}

#[derive(Clone, Debug, Eq, PartialEq, Event, Decode, Encode, Serialize, Deserialize)]
pub struct EpochDurationChangedEvent<V: Validator> {
    pub from: V::BlockNumber,
    pub to: V::BlockNumber,
}

#[derive(Clone, Debug, Eq, PartialEq, Event, Decode, Encode, Serialize, Deserialize)]
pub struct AuctionStartedEvent<V: Validator> {
    // TODO:  Ideally we use V::EpochIndex here, however we do that
    pub epoch_index: EpochIndex,

    pub _phantom: PhantomData<V>,
}

#[derive(Clone, Debug, Eq, PartialEq, Event, Decode, Encode, Serialize, Deserialize)]
pub struct AuctionEndedEvent<V: Validator> {
    // TODO:  Ideally we use V::EpochIndex here, however we do that
    pub epoch_index: EpochIndex,

    pub _phantom: PhantomData<V>,
}

#[derive(Clone, Debug, Eq, PartialEq, Event, Decode, Encode, Serialize, Deserialize)]
pub struct ForceRotationRequestedEvent<V: Validator> {
    pub _phantom: PhantomData<V>,
}

/// Wrapper for all Validator events
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ValidatorEvent<V: Validator> {
    MaximumValidatorsChangedEvent(MaximumValidatorsChangedEvent<V>),

    EpochDurationChangedEvent(EpochDurationChangedEvent<V>),

    AuctionStartedEvent(AuctionStartedEvent<V>),

    AuctionEndedEvent(AuctionEndedEvent<V>),

    ForceRotationRequestedEvent(ForceRotationRequestedEvent<V>),
}

impl From<MaximumValidatorsChangedEvent<StateChainRuntime>> for SCEvent {
    fn from(max_validators_changed: MaximumValidatorsChangedEvent<StateChainRuntime>) -> Self {
        SCEvent::ValidatorEvent(ValidatorEvent::MaximumValidatorsChangedEvent(
            max_validators_changed,
        ))
    }
}

impl From<EpochDurationChangedEvent<StateChainRuntime>> for SCEvent {
    fn from(epoch_duration_changed: EpochDurationChangedEvent<StateChainRuntime>) -> Self {
        SCEvent::ValidatorEvent(ValidatorEvent::EpochDurationChangedEvent(
            epoch_duration_changed,
        ))
    }
}

impl From<AuctionStartedEvent<StateChainRuntime>> for SCEvent {
    fn from(auction_started: AuctionStartedEvent<StateChainRuntime>) -> Self {
        SCEvent::ValidatorEvent(ValidatorEvent::AuctionStartedEvent(auction_started))
    }
}

impl From<AuctionEndedEvent<StateChainRuntime>> for SCEvent {
    fn from(auction_ended: AuctionEndedEvent<StateChainRuntime>) -> Self {
        SCEvent::ValidatorEvent(ValidatorEvent::AuctionEndedEvent(auction_ended))
    }
}

impl From<ForceRotationRequestedEvent<StateChainRuntime>> for SCEvent {
    fn from(force_rotation_requested: ForceRotationRequestedEvent<StateChainRuntime>) -> Self {
        SCEvent::ValidatorEvent(ValidatorEvent::ForceRotationRequestedEvent(
            force_rotation_requested,
        ))
    }
}

#[cfg(test)]
mod tests {

    use pallet_cf_validator::Config;

    use codec::Encode;
    use state_chain_runtime::Runtime as SCRuntime;

    use crate::sc_observer::runtime::StateChainRuntime;

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
    fn auction_started_decoding() {
        // AuctionStarted(EpochIndex)
        let event: <SCRuntime as Config>::Event =
            pallet_cf_validator::Event::<SCRuntime>::AuctionStarted(1).into();

        let encoded_auction_started = event.encode();
        // the first 2 bytes are (module_index, event_variant_index), these can be stripped
        let encoded_auction_started = encoded_auction_started[2..].to_vec();

        let decoded_event =
            AuctionStartedEvent::<StateChainRuntime>::decode(&mut &encoded_auction_started[..])
                .unwrap();

        let expecting = AuctionStartedEvent {
            epoch_index: 1,
            _phantom: PhantomData,
        };

        assert_eq!(decoded_event, expecting);
    }

    #[test]
    fn auction_ended_decoding() {
        // AuctionEnded(EpochIndex)
        let event: <SCRuntime as Config>::Event =
            pallet_cf_validator::Event::<SCRuntime>::AuctionEnded(1).into();

        let encoded_auction_ended = event.encode();
        // the first 2 bytes are (module_index, event_variant_index), these can be stripped
        let encoded_auction_ended = encoded_auction_ended[2..].to_vec();

        let decoded_event =
            AuctionEndedEvent::<StateChainRuntime>::decode(&mut &encoded_auction_ended[..])
                .unwrap();

        let expecting = AuctionEndedEvent {
            epoch_index: 1,
            _phantom: PhantomData,
        };

        assert_eq!(decoded_event, expecting);
    }

    #[test]
    fn max_validators_changed_decoding() {
        // MaximumValidatorsChanged(ValidatorSize, ValidatorSize)
        let event: <SCRuntime as Config>::Event =
            pallet_cf_validator::Event::<SCRuntime>::MaximumValidatorsChanged(1, 4).into();

        let encoded_max_validators_changed = event.encode();
        // the first 2 bytes are (module_index, event_variant_index), these can be stripped
        let encoded_max_validators_changed = encoded_max_validators_changed[2..].to_vec();

        let decoded_event = MaximumValidatorsChangedEvent::<StateChainRuntime>::decode(
            &mut &encoded_max_validators_changed[..],
        )
        .unwrap();

        let expecting = MaximumValidatorsChangedEvent {
            before: 1,
            now: 4,
            _phantom: PhantomData,
        };

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
