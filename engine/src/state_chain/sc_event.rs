use codec::Decode;
use substrate_subxt::RawEvent;

use anyhow::Result;

use super::{
    pallets::auction::AuctionEvent,
    pallets::staking::StakingEvent,
    pallets::vaults::{KeygenRequestEvent, VaultRotationRequestEvent, VaultsEvent},
    pallets::{validator::ValidatorEvent, vaults::ThresholdSignatureRequestEvent},
    runtime::StateChainRuntime,
};

#[derive(Debug, Clone, PartialEq)]
pub enum SCEvent {
    AuctionEvent(AuctionEvent<StateChainRuntime>),
    ValidatorEvent(ValidatorEvent<StateChainRuntime>),
    StakingEvent(StakingEvent<StateChainRuntime>),
    VaultsEvent(VaultsEvent<StateChainRuntime>),
}

/// Convert raw Substrate event to `SCEvent`
/// Supported events are:
/// - Vaults
///   - KeygenRequest
///   - ThresholdSignatureRequest
///   - VaultRotationRequest
pub fn raw_event_to_sc_event(raw_event: &RawEvent) -> Result<Option<SCEvent>> {
    match raw_event.module.as_str() {
        "Vaults" => match raw_event.variant.as_str() {
            "KeygenRequest" => Ok(Some(
                KeygenRequestEvent::<StateChainRuntime>::decode(&mut &raw_event.data[..])?.into(),
            )),
            "ThresholdSignatureRequest" => Ok(Some(
                ThresholdSignatureRequestEvent::<StateChainRuntime>::decode(
                    &mut &raw_event.data[..],
                )?
                .into(),
            )),
            "VaultRotationRequest" => Ok(Some(
                VaultRotationRequestEvent::<StateChainRuntime>::decode(&mut &raw_event.data[..])?
                    .into(),
            )),
            _ => Ok(None),
        },
        _ => Ok(None),
    }
}