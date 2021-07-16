use std::{marker::PhantomData, ops::Add};

use cf_traits::AuctionRange;
use codec::{Decode, Encode};

use frame_support::{pallet_prelude::MaybeSerializeDeserialize, Parameter};
use serde::{Deserialize, Serialize};
use sp_runtime::traits::One;
use substrate_subxt::{module, sp_runtime::traits::Member, system::System, Call, Event};

use super::{runtime::StateChainRuntime, sc_event::SCEvent};

#[module]
pub trait Auction: System {
    type AuctionIndex: Member + Parameter + Default + Add + One + Copy + MaybeSerializeDeserialize;

    type ValidatorId: Member + Parameter + MaybeSerializeDeserialize;
}

#[derive(Call, Encode)]
pub struct WitnessAuctionConfirmationCall<T: Auction> {
    auction_index: <T as Auction>::AuctionIndex,
}

// The order of these fields matter for decoding
#[derive(Clone, Debug, Eq, PartialEq, Event, Decode, Encode, Serialize, Deserialize)]
pub struct AuctionStartedEvent<A: Auction> {
    pub auction_index: <A as Auction>::AuctionIndex,
}

// The order of these fields matter for decoding
#[derive(Clone, Debug, Eq, PartialEq, Event, Decode, Encode, Serialize, Deserialize)]
pub struct AuctionCompletedEvent<T: Auction> {
    pub auction_index: <T as Auction>::AuctionIndex,

    pub validators: Vec<<T as Auction>::ValidatorId>,
}

#[derive(Clone, Debug, Eq, PartialEq, Event, Decode, Encode, Serialize, Deserialize)]
pub struct AuctionConfirmedEvent<A: Auction> {
    pub auction_index: <A as Auction>::AuctionIndex,
}

#[derive(Clone, Debug, Eq, PartialEq, Event, Decode, Encode, Serialize, Deserialize)]
pub struct AwaitingBiddersEvent<A: Auction> {
    _runtime: PhantomData<A>,
}

#[derive(Clone, Debug, Eq, PartialEq, Event, Decode, Encode, Serialize, Deserialize)]
pub struct AuctionRangeChangedEvent<A: Auction> {
    pub before: AuctionRange,
    pub now: AuctionRange,
    pub _runtime: PhantomData<A>,
}

#[derive(Clone, Debug, Eq, PartialEq, Event, Decode, Encode, Serialize, Deserialize)]
pub struct AuctionAbortedEvent<A: Auction> {
    pub auction_index: <A as Auction>::AuctionIndex,
}

/// Derives an enum for the listed events and corresponding implementations of `From`.
macro_rules! impl_auction_event_enum {
    ( $( $name:tt ),+ ) => {
        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
        pub enum AuctionEvent<Runtime: Auction> {
            $(
                $name($name<Runtime>),
            )+
        }

        $(
            impl From<$name<StateChainRuntime>> for SCEvent {
                fn from(auction_event: $name<StateChainRuntime>) -> Self {
                    SCEvent::AuctionEvent(AuctionEvent::$name(auction_event))
                }
            }
        )+
    };
}

impl_auction_event_enum!(
    AuctionStartedEvent,
    AuctionCompletedEvent,
    AuctionConfirmedEvent,
    AwaitingBiddersEvent,
    AuctionRangeChangedEvent,
    AuctionAbortedEvent
);

#[cfg(test)]
mod tests {
    use super::*;

    use pallet_cf_auction::Config;

    use state_chain_runtime::Runtime as SCRuntime;

    #[test]
    fn auction_started_decoding() {
        let event: <SCRuntime as Config>::Event =
            pallet_cf_auction::Event::<SCRuntime>::AuctionStarted(1).into();

        let encoded_auction_started = event.encode();
        // the first 2 bytes are (module_index, event_variant_index), these can be stripped
        let encoded_auction_started = encoded_auction_started[2..].to_vec();

        let decoded_event =
            AuctionStartedEvent::<StateChainRuntime>::decode(&mut &encoded_auction_started[..])
                .unwrap();

        let expecting = AuctionStartedEvent { auction_index: 1 };

        assert_eq!(decoded_event, expecting);
    }

    #[test]
    fn auction_confirmed_decoding() {
        let event: <SCRuntime as Config>::Event =
            pallet_cf_auction::Event::<SCRuntime>::AuctionConfirmed(1).into();

        let encoded_auction_confirmed = event.encode();
        // the first 2 bytes are (module_index, event_variant_index), these can be stripped
        let encoded_auction_confirmed = encoded_auction_confirmed[2..].to_vec();

        let decoded_event =
            AuctionConfirmedEvent::<StateChainRuntime>::decode(&mut &encoded_auction_confirmed[..])
                .unwrap();

        let expecting = AuctionConfirmedEvent { auction_index: 1 };

        assert_eq!(decoded_event, expecting);
    }

    #[test]
    fn auction_range_changed_decoding() {
        // AuctionRangeChanged(AuctionRange, AuctionRange)
        let event: <SCRuntime as Config>::Event =
            pallet_cf_auction::Event::<SCRuntime>::AuctionRangeChanged((0, 1), (0, 2)).into();

        let encoded_auction_range_changed = event.encode();
        // the first 2 bytes are (module_index, event_variant_index), these can be stripped
        let encoded_auction_range_changed = encoded_auction_range_changed[2..].to_vec();

        let decoded_event = AuctionRangeChangedEvent::<StateChainRuntime>::decode(
            &mut &encoded_auction_range_changed[..],
        )
        .unwrap();

        let expecting = AuctionRangeChangedEvent {
            before: (0, 1),
            now: (0, 2),
            _runtime: PhantomData,
        };

        assert_eq!(decoded_event, expecting);
    }

    #[test]
    fn auction_completed_decoding() {
        let alice = sp_keyring::AccountKeyring::Alice.to_account_id();
        let bob = sp_keyring::AccountKeyring::Bob.to_account_id();

        let validator_ids = vec![alice.clone(), bob.clone()];
        let event: <SCRuntime as Config>::Event =
            pallet_cf_auction::Event::<SCRuntime>::AuctionCompleted(1, validator_ids).into();

        let encoded_auction_completed = event.encode();
        // the first 2 bytes are (module_index, event_variant_index), these can be stripped
        let encoded_auction_completed = encoded_auction_completed[2..].to_vec();

        let decoded_event =
            AuctionCompletedEvent::<StateChainRuntime>::decode(&mut &encoded_auction_completed[..])
                .unwrap();

        let expecting = AuctionCompletedEvent {
            auction_index: 1,
            validators: vec![alice, bob],
        };

        assert_eq!(decoded_event, expecting);
    }

    #[test]
    fn auction_aborted_decoding() {
        let event: <SCRuntime as Config>::Event =
            pallet_cf_auction::Event::<SCRuntime>::AuctionAborted(1).into();

        let encoded_auction_aborted = event.encode();
        // the first 2 bytes are (module_index, event_variant_index), these can be stripped
        let encoded_auction_aborted = encoded_auction_aborted[2..].to_vec();

        let decoded_event =
            AuctionAbortedEvent::<StateChainRuntime>::decode(&mut &encoded_auction_aborted[..])
                .unwrap();

        let expecting = AuctionAbortedEvent { auction_index: 1 };

        assert_eq!(decoded_event, expecting);
    }

    #[test]
    fn awaiting_bidders() {
        let event: <SCRuntime as Config>::Event =
            pallet_cf_auction::Event::<SCRuntime>::AwaitingBidders.into();

        let awaiting_bidders_encoded = event.encode();
        // the first 2 bytes are (module_index, event_variant_index), these can be stripped
        let awaiting_bidders_encoded = awaiting_bidders_encoded[2..].to_vec();

        let decoded_event =
            AwaitingBiddersEvent::<StateChainRuntime>::decode(&mut &awaiting_bidders_encoded[..])
                .unwrap();

        let expecting = AwaitingBiddersEvent {
            _runtime: PhantomData,
        };

        assert_eq!(decoded_event, expecting);
    }
}
