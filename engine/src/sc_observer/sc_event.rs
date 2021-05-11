use chainflip_common::types::coin::Coin;
use codec::Decode;
use substrate_subxt::RawEvent;

use crate::mq::Subject;

use anyhow::Result;

use super::{
    runtime::StateChainRuntime,
    staking::{
        ClaimSigRequestedEvent, ClaimSignatureIssuedEvent, ClaimedEvent, StakeRefundEvent,
        StakedEvent, StakingEvent,
    },
    validator::{
        AuctionEndedEvent, AuctionStartedEvent, EpochDurationChangedEvent,
        ForceRotationRequestedEvent, MaximumValidatorsChangedEvent, ValidatorEvent,
    },
};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SCEvent {
    ValidatorEvent(ValidatorEvent<StateChainRuntime>),
    StakingEvent(StakingEvent<StateChainRuntime>),
}

/// Decode a raw event (substrate codec) into a SCEvent wrapper enum
pub(super) fn sc_event_from_raw_event(raw_event: RawEvent) -> Result<Option<SCEvent>> {
    let event = match raw_event.module.as_str() {
        "Staking" => match raw_event.variant.as_str() {
            "ClaimSigRequested" => Ok(Some(
                ClaimSigRequestedEvent::<StateChainRuntime>::decode(&mut &raw_event.data[..])?
                    .into(),
            )),
            "StakeRefund" => Ok(Some(
                StakeRefundEvent::<StateChainRuntime>::decode(&mut &raw_event.data[..])?.into(),
            )),
            "ClaimSignatureIssued" => Ok(Some(
                ClaimSignatureIssuedEvent::<StateChainRuntime>::decode(&mut &raw_event.data[..])?
                    .into(),
            )),
            "Claimed" => Ok(Some(
                ClaimedEvent::<StateChainRuntime>::decode(&mut &raw_event.data[..])?.into(),
            )),
            "Staked" => Ok(Some(
                StakedEvent::<StateChainRuntime>::decode(&mut &raw_event.data[..])?.into(),
            )),
            _ => Ok(None),
        },
        "Validator" => match raw_event.variant.as_str() {
            "AuctionEnded" => Ok(Some(
                AuctionEndedEvent::<StateChainRuntime>::decode(&mut &raw_event.data[..])?.into(),
            )),
            "AuctionStarted" => Ok(Some(
                AuctionStartedEvent::<StateChainRuntime>::decode(&mut &raw_event.data[..])?.into(),
            )),
            "ForceRotationRequested" => Ok(Some(
                ForceRotationRequestedEvent::<StateChainRuntime>::decode(&mut &raw_event.data[..])?
                    .into(),
            )),
            "EpochDurationChanged" => Ok(Some(
                EpochDurationChangedEvent::<StateChainRuntime>::decode(&mut &raw_event.data[..])?
                    .into(),
            )),
            "MaximumValidatorsChanged" => Ok(Some(
                MaximumValidatorsChangedEvent::<StateChainRuntime>::decode(
                    &mut &raw_event.data[..],
                )?
                .into(),
            )),
            _ => Ok(None),
        },
        _ => Ok(None),
    };
    event
}

/// Returns the subject to publish the data of a raw event to
pub(super) fn subject_from_raw_event(event: &RawEvent) -> Option<Subject> {
    let subject = match event.module.as_str() {
        "System" => None,
        "Staking" => match event.variant.as_str() {
            "ClaimSigRequested" => Some(Subject::Claim),
            // All Stake refunds are ETH, how are these refunds coming out though? as batches or individual txs?
            "StakeRefund" => Some(Subject::Batch(Coin::ETH)),
            "ClaimSignatureIssued" => Some(Subject::Claim),
            // This doesn't need to go anywhere, this is just a confirmation emitted, perhaps for block explorers
            "Claimed" => None,
            _ => None,
        },
        "Validator" => match event.variant.as_str() {
            "AuctionEnded" => None,
            "AuctionStarted" => None,
            "ForceRotationRequested" => Some(Subject::Rotate),
            "EpochDurationChanged" => None,
            "MaximumValidatorsChanged" => None,
            _ => None,
        },
        _ => None,
    };
    subject
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
    fn subject_from_raw_event_test() {
        // test success case
        let raw_event = substrate_subxt::RawEvent {
            // Module and variant are defined by the state chain node
            module: "Staking".to_string(),
            variant: "ClaimSigRequested".to_string(),
            data: "Test data".as_bytes().to_owned(),
        };

        let subject = subject_from_raw_event(&raw_event);
        assert_eq!(subject, Some(Subject::Claim));

        // test "fail" case
        let raw_event_invalid = substrate_subxt::RawEvent {
            // Module and variant are defined by the state chain node
            module: "NotAModule".to_string(),
            variant: "NotAVariant".to_string(),
            data: "Test data".as_bytes().to_owned(),
        };
        let subject = subject_from_raw_event(&raw_event_invalid);
        assert_eq!(subject, None);
    }

    #[test]
    fn sc_event_from_raw_event_test() {
        let who = AccountKeyring::Alice.to_account_id();

        let event: <SCRuntime as Config>::Event =
            pallet_cf_staking::Event::<SCRuntime>::Claimed(who.clone(), 150u128).into();

        let encoded_claimed = event.encode();

        // the first 2 bytes are (module_index, event_variant_index), these can be stripped
        let encoded_claimed = encoded_claimed[2..].to_vec();

        let raw_event = RawEvent {
            module: "Staking".to_string(),
            variant: "Claimed".to_string(),
            data: encoded_claimed,
        };

        let sc_event = sc_event_from_raw_event(raw_event);
        assert!(sc_event.is_ok());
        let sc_event = sc_event.unwrap();

        let expected: SCEvent = ClaimedEvent {
            who,
            amount: 150u128,
            _phantom: PhantomData,
        }
        .into();

        assert_eq!(sc_event, Some(expected));
    }
}
