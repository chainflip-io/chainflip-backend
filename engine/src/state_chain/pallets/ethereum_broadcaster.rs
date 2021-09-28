//! Implements subxt support for the broadcast pallet

use cf_chains::eth::{RawSignedTransaction, UnsignedTransaction};
use codec::{Decode, Encode};
use serde::{Deserialize, Serialize};
use sp_runtime::AccountId32;
use std::marker::PhantomData;
use substrate_subxt::{module, system::System, Event};

pub use pallet_cf_broadcast::BroadcastAttemptId;

#[module]
pub trait EthereumBroadcaster: System {}

// The order of these fields matters for decoding
#[derive(Clone, Debug, Eq, PartialEq, Event, Encode, Decode, Serialize, Deserialize)]
pub struct TransactionSigningRequest<B: EthereumBroadcaster> {
    /// The broadcast attempt id.
    broadcast_attempt_id: BroadcastAttemptId,
    /// The account nominated for signing this transaction.
    nominated_signer: AccountId32,
    /// The transaction to sign.
    unsigned_transaction: UnsignedTransaction,
    /// Runtime marker
    _marker: PhantomData<B>,
}

// The order of these fields matters for decoding
#[derive(Clone, Debug, Eq, PartialEq, Event, Encode, Decode, Serialize, Deserialize)]
pub struct BroadcastRequest<B: EthereumBroadcaster> {
    /// The broadcast attempt id.
    broadcast_attempt_id: BroadcastAttemptId,
    /// The raw signed transaction.
    signed_transaction: RawSignedTransaction,
    /// Runtime marker
    _marker: PhantomData<B>,
}

#[cfg(test)]
mod test_events_decoding {
    use super::*;
    use crate::state_chain::runtime::StateChainRuntime;
    use pallet_cf_broadcast::{Event as BroadcastEvent, Instance0};
    use sp_keyring::AccountKeyring;
    use state_chain_runtime::{Event, Runtime};

    #[test]
    fn test_transaction_sigining_request() {
        let nominated_signer = AccountKeyring::Alice.to_account_id();
        const ATTEMPT_ID: u64 = 42;
        let unsigned_transaction = UnsignedTransaction::default();

        let event: Event = BroadcastEvent::<Runtime, Instance0>::TransactionSigningRequest(
            ATTEMPT_ID,
            nominated_signer.clone(),
            unsigned_transaction.clone(),
        )
        .into();

        let expected_subxt_event = TransactionSigningRequest::<StateChainRuntime> {
            broadcast_attempt_id: ATTEMPT_ID,
            nominated_signer,
            unsigned_transaction,
            _marker: Default::default(),
        };

        let encoded = event.encode()[2..].to_vec();

        assert_eq!(
            TransactionSigningRequest::<StateChainRuntime>::decode(&mut &encoded[..]).unwrap(),
            expected_subxt_event
        );
    }

    #[test]
    fn test_broadcast_request() {
        const ATTEMPT_ID: u64 = 42;
        let signed_transaction = RawSignedTransaction::default();

        let event: Event = BroadcastEvent::<Runtime, Instance0>::BroadcastRequest(
            ATTEMPT_ID,
            signed_transaction.clone(),
        )
        .into();

        let expected_subxt_event = BroadcastRequest::<StateChainRuntime> {
            broadcast_attempt_id: ATTEMPT_ID,
            signed_transaction,
            _marker: Default::default(),
        };

        let encoded = event.encode()[2..].to_vec();

        assert_eq!(
            BroadcastRequest::<StateChainRuntime>::decode(&mut &encoded[..]).unwrap(),
            expected_subxt_event
        );
    }
}
