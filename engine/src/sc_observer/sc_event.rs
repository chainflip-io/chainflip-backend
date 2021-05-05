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
    validator::ValidatorEvent,
};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SCEvent {
    ValidatorEvent(ValidatorEvent<StateChainRuntime>),
    StakingEvent(StakingEvent<StateChainRuntime>),
}

pub(super) fn subxt_event_from_sc_event(raw_event: RawEvent) -> Result<Option<SCEvent>> {
    let event = match raw_event.module.as_str() {
        "System" => Ok(None),
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
            // This doesn't need to go anywhere, this is just a confirmation emitted, perhaps for block explorers
            "Claimed" => Ok(Some(
                ClaimedEvent::<StateChainRuntime>::decode(&mut &raw_event.data[..])?.into(),
            )),
            "Staked" => Ok(Some(
                StakedEvent::<StateChainRuntime>::decode(&mut &raw_event.data[..])?.into(),
            )),
            _ => Ok(None),
        },
        // "Validator" => match raw_event.variant.as_str() {
        //     "AuctionEnded" => None,
        //     "AuctionStarted" => None,
        //     "ForceRotationRequested" => None,
        //     "EpochDurationChanged" => None,
        //     _ => None,
        // },
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
            _ => None,
        },
        _ => None,
    };
    subject
}

#[cfg(test)]
mod tests {

    use super::*;

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
}
