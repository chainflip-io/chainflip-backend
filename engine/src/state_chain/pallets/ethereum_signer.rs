//! Implements subxt support for the signing pallet

use codec::{Decode, Encode};
use frame_support::{pallet_prelude::Member, Parameter};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sp_runtime::AccountId32;
use substrate_subxt::{module, system::System, Event};

pub type CeremonyId = u64;

#[module]
pub trait EthereumThresholdSigner: System {
    type KeyId: Member + Parameter + Serialize + DeserializeOwned;
    type Payload: Member + Parameter + Serialize + DeserializeOwned;
}

// The order of these fields matter for decoding
#[derive(Clone, Debug, Eq, PartialEq, Event, Encode, Decode, Serialize, Deserialize)]
pub struct ThresholdSignatureRequest<S: EthereumThresholdSigner> {
    /// The ceremony id.
    ceremony_id: CeremonyId,
    /// The id of the key to be used for signing.
    key_id: S::KeyId,
    /// The list of participants to the signing ceremony.
    signatories: Vec<AccountId32>,
    /// The payload to be signed.
    payload: S::Payload,
}

#[cfg(test)]
mod test_events_decoding {
    use super::*;
    use crate::state_chain::runtime::StateChainRuntime;
    use pallet_cf_threshold_signature::{Event as ThresholdSigningEvent, Instance0};
    use sp_core::H256;
    use sp_keyring::AccountKeyring;
    use state_chain_runtime::{Event, Runtime};

    #[test]
    fn test_threshold_signature_request() {
        let signatories = vec![AccountKeyring::Alice.to_account_id()];
        let key_id: <StateChainRuntime as EthereumThresholdSigner>::KeyId = b"current_key".to_vec();
        const CEREMONY_ID: u64 = 42;
        const PAYLOAD: <StateChainRuntime as EthereumThresholdSigner>::Payload = H256([0xcf; 32]);

        let event: Event = ThresholdSigningEvent::<Runtime, Instance0>::ThresholdSignatureRequest(
            CEREMONY_ID,
            key_id.clone(),
            signatories.clone(),
            PAYLOAD,
        )
        .into();

        let expected_subxt_event = ThresholdSignatureRequest::<StateChainRuntime> {
            ceremony_id: CEREMONY_ID,
            key_id,
            signatories,
            payload: PAYLOAD,
        };

        let encoded = event.encode()[2..].to_vec();

        assert_eq!(
            ThresholdSignatureRequest::<StateChainRuntime>::decode(&mut &encoded[..]).unwrap(),
            expected_subxt_event
        );
    }
}
