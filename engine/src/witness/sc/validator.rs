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

#[derive(Clone, Debug, Eq, PartialEq, Event, Decode)]
pub struct EpochChangedEvent<V: Validator> {
    pub from: V::BlockNumber,
    pub to: V::BlockNumber,
}

#[derive(Clone, Debug, Eq, PartialEq, Event, Decode)]
pub struct AuctionStartedEvent<V: Validator> {
    // TODO:  Ideally we use V::EpochIndex here, however we do that
    pub epoch_index: u32,

    pub _phantom: PhantomData<V>,
}

// RawEvent {
//     module: "Validator",
//     variant: "EpochChanged",
//     data: "040000000a000000",
// }

mod tests {

    use pallet_cf_validator::{Config, Event};
    use substrate_subxt::RawEvent;

    use crate::witness::sc::runtime::StateChainRuntime;
    use codec::Encode;
    use state_chain_runtime::Runtime as SCRuntime;

    use super::*;

    #[test]
    fn epoch_changed_decoding() {
        let event: <SCRuntime as Config>::Event =
            pallet_cf_validator::Event::<SCRuntime>::EpochChanged(4, 10).into();

        let encoded_epoch = event.encode();
        // the first 2 bytes are (module_index, event_variant_index), these can be stripped
        let encoded_epoch = encoded_epoch[2..].to_vec();

        let decoded_event =
            EpochChangedEvent::<StateChainRuntime>::decode(&mut &encoded_epoch[..]).unwrap();

        let expecting = EpochChangedEvent { from: 4, to: 10 };

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
        todo!()
    }

    #[test]
    fn max_validators_changed_decoding() {
        // MaximumValidatorsChangedDecoding(ValidatorSize, ValidatorSize)
        todo!()
    }

    #[test]
    fn force_rotation_requested_decoding() {
        // ForceRotationRequested()
        todo!()
    }
}
