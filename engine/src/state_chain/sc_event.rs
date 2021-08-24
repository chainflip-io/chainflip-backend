use codec::Decode;
use substrate_subxt::RawEvent;

use crate::mq::Subject;

use anyhow::Result;

use super::{
    pallets::auction::{AuctionCompletedEvent, AuctionEvent},
    pallets::staking::StakingEvent,
    pallets::validator::{NewEpochEvent, ValidatorEvent},
    runtime::StateChainRuntime,
};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SCEvent {
    AuctionEvent(AuctionEvent<StateChainRuntime>),
    ValidatorEvent(ValidatorEvent<StateChainRuntime>),
    StakingEvent(StakingEvent<StateChainRuntime>),
}

/// Decode a raw event (substrate codec) into a SCEvent wrapper enum
pub fn sc_event_from_raw_event(raw_event: RawEvent) -> Result<Option<SCEvent>> {
    let event = match raw_event.module.as_str() {
        "Validator" => match raw_event.variant.as_str() {
            "NewEpoch" => Ok(Some(
                NewEpochEvent::<StateChainRuntime>::decode(&mut &raw_event.data[..])?.into(),
            )),
            _ => Ok(None),
        },
        "Auction" => match raw_event.variant.as_str() {
            "AuctionCompleted" => Ok(Some(
                AuctionCompletedEvent::<StateChainRuntime>::decode(&mut &raw_event.data[..])?
                    .into(),
            )),
            _ => Ok(None),
        },
        _ => Ok(None),
    };
    event
}

/// Returns the subject to publish the data of a raw event to
pub fn raw_event_to_subject(event: &RawEvent) -> Option<Subject> {
    match event.module.as_str() {
        "Auction" => match event.variant.as_str() {
            "AuctionCompleted" => Some(Subject::AuctionCompleted),
            _ => None,
        },
        "Validator" => match event.variant.as_str() {
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {

    use std::marker::PhantomData;

    use super::*;

    use codec::Encode;
    use pallet_cf_staking::Config;
    use sp_keyring::AccountKeyring;

    use state_chain_runtime::Runtime as SCRuntime;

    // #[test]
    // fn raw_event_to_subject_test() {
    //     // test success case
    //     let raw_event = substrate_subxt::RawEvent {
    //         // Module and variant are defined by the state chain node
    //         module: "Staking".to_string(),
    //         variant: "ClaimSigRequested".to_string(),
    //         data: "Test data".as_bytes().to_owned(),
    //     };

    //     let subject = raw_event_to_subject(&raw_event);
    //     assert_eq!(subject, Some(Subject::ClaimSigRequested));

    //     // test "fail" case
    //     let raw_event_invalid = substrate_subxt::RawEvent {
    //         // Module and variant are defined by the state chain node
    //         module: "NotAModule".to_string(),
    //         variant: "NotAVariant".to_string(),
    //         data: "Test data".as_bytes().to_owned(),
    //     };
    //     let subject = raw_event_to_subject(&raw_event_invalid);
    //     assert_eq!(subject, None);
    // }

    #[test]
    fn sc_event_from_raw_event_test() {
        let who = AccountKeyring::Alice.to_account_id();

        let event: <SCRuntime as Config>::Event =
            pallet_cf_staking::Event::<SCRuntime>::ClaimSettled(who.clone(), 150u128).into();

        let encoded_claimed = event.encode();

        // the first 2 bytes are (module_index, event_variant_index), these can be stripped
        let encoded_claimed = encoded_claimed[2..].to_vec();

        let raw_event = RawEvent {
            module: "Staking".to_string(),
            variant: "ClaimedSettled".to_string(),
            data: encoded_claimed,
        };

        let sc_event = sc_event_from_raw_event(raw_event);
        assert!(sc_event.is_ok());
        let sc_event = sc_event.unwrap();

        let expected: SCEvent = ClaimSettledEvent {
            who,
            amount: 150u128,
            _runtime: PhantomData,
        }
        .into();

        assert_eq!(sc_event, Some(expected));
    }
}
