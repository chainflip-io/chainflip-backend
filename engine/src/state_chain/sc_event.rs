use codec::Decode;
use substrate_subxt::RawEvent;

use crate::mq::Subject;

use anyhow::Result;

use super::{
    pallets::auction::{
        AuctionAbortedEvent, AuctionCompletedEvent, AuctionConfirmedEvent, AuctionEvent,
        AuctionRangeChangedEvent, AuctionStartedEvent, AwaitingBiddersEvent,
    },
    pallets::staking::{
        AccountActivated, AccountRetired, ClaimExpired, ClaimSettledEvent, ClaimSigRequestedEvent,
        ClaimSignatureIssuedEvent, StakeRefundEvent, StakedEvent, StakingEvent,
    },
    pallets::validator::{
        EpochDurationChangedEvent, ForceRotationRequestedEvent, NewEpochEvent, ValidatorEvent,
    },
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
        "Staking" => match raw_event.variant.as_str() {
            "Staked" => Ok(Some(
                StakedEvent::<StateChainRuntime>::decode(&mut &raw_event.data[..])?.into(),
            )),
            "ClaimedSettled" => Ok(Some(
                ClaimSettledEvent::<StateChainRuntime>::decode(&mut &raw_event.data[..])?.into(),
            )),
            "StakeRefund" => Ok(Some(
                StakeRefundEvent::<StateChainRuntime>::decode(&mut &raw_event.data[..])?.into(),
            )),
            "ClaimSigRequested" => Ok(Some(
                ClaimSigRequestedEvent::<StateChainRuntime>::decode(&mut &raw_event.data[..])?
                    .into(),
            )),
            "ClaimSignatureIssued" => Ok(Some(
                ClaimSignatureIssuedEvent::<StateChainRuntime>::decode(&mut &raw_event.data[..])?
                    .into(),
            )),
            "AccountRetired" => Ok(Some(
                AccountRetired::<StateChainRuntime>::decode(&mut &raw_event.data[..])?.into(),
            )),
            "AccountActivated" => Ok(Some(
                AccountActivated::<StateChainRuntime>::decode(&mut &raw_event.data[..])?.into(),
            )),
            "ClaimExpired" => Ok(Some(
                ClaimExpired::<StateChainRuntime>::decode(&mut &raw_event.data[..])?.into(),
            )),
            _ => Ok(None),
        },
        "Validator" => match raw_event.variant.as_str() {
            "ForceRotationRequested" => Ok(Some(
                ForceRotationRequestedEvent::<StateChainRuntime>::decode(&mut &raw_event.data[..])?
                    .into(),
            )),
            "EpochDurationChanged" => Ok(Some(
                EpochDurationChangedEvent::<StateChainRuntime>::decode(&mut &raw_event.data[..])?
                    .into(),
            )),
            "NewEpoch" => Ok(Some(
                NewEpochEvent::<StateChainRuntime>::decode(&mut &raw_event.data[..])?.into(),
            )),
            _ => Ok(None),
        },
        "Auction" => match raw_event.variant.as_str() {
            "AuctionStarted" => Ok(Some(
                AuctionStartedEvent::<StateChainRuntime>::decode(&mut &raw_event.data[..])?.into(),
            )),
            "AuctionRangeChanged" => Ok(Some(
                AuctionRangeChangedEvent::<StateChainRuntime>::decode(&mut &raw_event.data[..])?
                    .into(),
            )),
            "AuctionCompleted" => Ok(Some(
                AuctionCompletedEvent::<StateChainRuntime>::decode(&mut &raw_event.data[..])?
                    .into(),
            )),
            "AuctionAborted" => Ok(Some(
                AuctionAbortedEvent::<StateChainRuntime>::decode(&mut &raw_event.data[..])?.into(),
            )),
            "AwaitingBidders" => Ok(Some(
                AwaitingBiddersEvent::<StateChainRuntime>::decode(&mut &raw_event.data[..])?.into(),
            )),
            "AuctionConfirmed" => Ok(Some(
                AuctionConfirmedEvent::<StateChainRuntime>::decode(&mut &raw_event.data[..])?
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
            "AuctionStarted" => Some(Subject::AuctionStarted),
            "AuctionConfirmed" => Some(Subject::AuctionConfirmed),
            "AuctionCompleted" => Some(Subject::AuctionCompleted),
            "AuctionAborted" => Some(Subject::AuctionAborted),
            "AuctionRangeChanged" => Some(Subject::AuctionRangeChanged),
            "AwaitingBidders" => Some(Subject::AwaitingBidders),
            _ => None,
        },
        "Staking" => match event.variant.as_str() {
            "ClaimSigRequested" => Some(Subject::ClaimSigRequested),
            "Staked" => Some(Subject::Staked),
            "ClaimSettled" => Some(Subject::ClaimSettled),
            "StakeRefund" => Some(Subject::StakeRefund),
            "ClaimSignatureIssued" => Some(Subject::ClaimSignatureIssued),
            "AccountRetired" => Some(Subject::AccountRetired),
            "AccountActivated" => Some(Subject::AccountActivated),
            _ => None,
        },
        "Validator" => match event.variant.as_str() {
            "ForceRotationRequested" => Some(Subject::ForceRotationRequested),
            "EpochDurationChanged" => Some(Subject::EpochDurationChanged),
            "NewEpoch" => Some(Subject::NewEpoch),
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

    #[test]
    fn raw_event_to_subject_test() {
        // test success case
        let raw_event = substrate_subxt::RawEvent {
            // Module and variant are defined by the state chain node
            module: "Staking".to_string(),
            variant: "ClaimSigRequested".to_string(),
            data: "Test data".as_bytes().to_owned(),
        };

        let subject = raw_event_to_subject(&raw_event);
        assert_eq!(subject, Some(Subject::ClaimSigRequested));

        // test "fail" case
        let raw_event_invalid = substrate_subxt::RawEvent {
            // Module and variant are defined by the state chain node
            module: "NotAModule".to_string(),
            variant: "NotAVariant".to_string(),
            data: "Test data".as_bytes().to_owned(),
        };
        let subject = raw_event_to_subject(&raw_event_invalid);
        assert_eq!(subject, None);
    }

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
