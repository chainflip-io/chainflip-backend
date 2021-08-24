use codec::Decode;
use substrate_subxt::RawEvent;

use crate::mq::Subject;

use anyhow::Result;

use super::{
    pallets::auction::{AuctionCompletedEvent, AuctionEvent},
    pallets::staking::StakingEvent,
    pallets::validator::ValidatorEvent,
    pallets::vaults::{KeygenRequestEvent, VaultsEvent},
    runtime::StateChainRuntime,
};

// TODO: Do we need these?
#[derive(Debug, Clone, PartialEq)]
pub enum SCEvent {
    AuctionEvent(AuctionEvent<StateChainRuntime>),
    ValidatorEvent(ValidatorEvent<StateChainRuntime>),
    StakingEvent(StakingEvent<StateChainRuntime>),
    VaultsEvent(VaultsEvent<StateChainRuntime>),
}

/// Raw substrate event to Subject and SCEvent
pub fn raw_event_to_subject_and_sc_event(
    raw_event: &RawEvent,
) -> Result<Option<(Subject, SCEvent)>> {
    let event = match raw_event.module.as_str() {
        "Auction" => match raw_event.variant.as_str() {
            "AuctionCompleted" => Ok(Some((
                Subject::AuctionCompleted,
                AuctionCompletedEvent::<StateChainRuntime>::decode(&mut &raw_event.data[..])?
                    .into(),
            ))),
            _ => Ok(None),
        },
        "Vaults" => match raw_event.variant.as_str() {
            "KeygenRequest" => Ok(Some((
                Subject::AuctionCompleted,
                KeygenRequestEvent::<StateChainRuntime>::decode(&mut &raw_event.data[..])?.into(),
            ))),
            _ => Ok(None),
        },
        _ => Ok(None),
    };
    event
}
