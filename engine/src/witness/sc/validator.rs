// Implements support for the validator module

use std::marker::PhantomData;

use chainflip_common::types::addresses::{Address, EthereumAddress};
use codec::{Codec, Decode, Encode};
use hex;
use serde::{Deserialize, Serialize};
use substrate_subxt::{
    module,
    sp_runtime::{app_crypto::RuntimePublic, traits::Member},
    system::System,
    Event,
};

use state_chain_runtime::Event as SCEvent;

#[module]
pub trait Validator: System {}

#[derive(Clone, Debug, Eq, PartialEq, Event, Decode, Serialize)]
pub struct MaximumValidatorsChangedEvent<V: Validator> {
    // There is a type alias type ValidatorSize = u32;
    // ideally we use that, not sure if there's a better way than making
    // that type pub
    pub before: u32,
    pub now: u32,
    pub _phantom: PhantomData<V>,
}

#[derive(Clone, Debug, Eq, PartialEq, Event, Decode, Serialize)]
pub struct EpochChangedEvent<V: Validator> {
    pub from: V::BlockNumber,
    pub to: V::BlockNumber,
}

// RawEvent {
//     module: "Validator",
//     variant: "EpochChanged",
//     data: "040000000a000000",
// }

mod tests {

    use pallet_cf_validator::Event;
    use substrate_subxt::RawEvent;

    use crate::witness::sc::runtime::StateChainRuntime;

    use super::*;

    // This test works
    #[test]
    fn test_epoch_changed() {
        let raw_event = RawEvent {
            module: "Validator".to_string(),
            variant: "EpochChanged".to_string(),
            data: hex::decode("040000000a000000").unwrap(),
        };

        let event =
            EpochChangedEvent::<StateChainRuntime>::decode(&mut &raw_event.data[..]).unwrap();

        let epoch_changed_event = EpochChangedEvent::<StateChainRuntime> { from: 4, to: 10 };

        println!("Here's the event: {:#?}", event);

        assert_eq!(event, epoch_changed_event);
    }

    #[test]
    fn epoch_encoding_working() {
        use codec::Encode;
        use state_chain_runtime::Runtime as SCRuntime;
        let hex_event_from_subxt = hex::decode("040000000a000000").unwrap();
        println!("Hex event from subxt: {:#?}", hex_event_from_subxt);

        let event =
            EpochChangedEvent::<StateChainRuntime>::decode(&mut &hex_event_from_subxt[..]).unwrap();

        println!("Event decoded into custom subxt struct: {:#?}", event);

        let epoch_changed_evt = pallet_cf_validator::Event::<SCRuntime>::EpochChanged(4, 10);
        println!("Epoch changed event: {:#?}", epoch_changed_evt);
        let encoded_epoch = epoch_changed_evt.encode();
        println!("Encoded epoch: {:#?}", encoded_epoch);
    }

    // #[test]
    // fn test_with_real_events() {
    //     use codec::{Decode, Encode};
    //     use state_chain_runtime::Runtime as SCRuntime;

    //     // pallet_cf_validator(crate::Event::MaximumValidatorsChanged(0, 2))
    //     let event = SCEvent::pallet_cf_validator(
    //         pallet_cf_validator::Event::<SCRuntime>::MaximumValidatorsChanged(0, 250),
    //     );

    //     let event = pallet_cf_validator::Event::<SCRuntime>::MaximumValidatorsChanged(0, 250);
    //     println!("Event: {:#?}", event);

    //     let raw_event = RawEvent {
    //         module: "Hello".to_string(),
    //         variant: "Goodbye".to_string(),
    //         data: event.to_vec(),
    //     };

    //     let raw_bytes = hex::encode(&event);
    //     println!("Event wrapped: {:#?}", evt);

    // let event_encoded = hex::encode(event.into());
    // println!("Event encoded: {:#?}", event_encoded);

    // let decoded_event = MaximumValidatorsChanged::<StateChainRuntime>::decode(
    //     &mut &event_encoded.as_bytes()[..],
    // );

    // assert!(decoded_event.is_ok());
    // let decoded_event = decoded_event.unwrap();

    // println!("Decoded event: {:#?}", decoded_event);
    // }
}
