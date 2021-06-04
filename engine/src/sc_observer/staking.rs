// Implements support for the staking module

use std::marker::PhantomData;

use codec::{Decode, Encode, FullCodec};
use frame_support::{pallet_prelude::*, sp_runtime::traits::Verify};
use sp_runtime::traits::{AtLeast32BitUnsigned, CheckedSub};
use substrate_subxt::{module, sp_core::crypto::AccountId32, system::System, Call, Event};

use serde::{Deserialize, Serialize};
use sp_core::ecdsa::Signature;

use super::{runtime::StateChainRuntime, sc_event::SCEvent};

// StakeManager in the state chain runtime
#[module]
pub trait StakeManager: System {
    /// Numeric type denomination for the staked asset.
    type TokenAmount: Member
        + FullCodec
        + Copy
        + Default
        + AtLeast32BitUnsigned
        + MaybeSerializeDeserialize
        + CheckedSub;

    /// Ethereum address type, should correspond to [u8; 20], but defined globally for the runtime.
    type EthereumAddress: Member + FullCodec + Copy;

    type Nonce: Member
        + FullCodec
        + Copy
        + Default
        + AtLeast32BitUnsigned
        + MaybeSerializeDeserialize
        + CheckedSub;
}

#[derive(Call, Encode)]
pub struct MinExtCall<T: StakeManager> {
    pub _runtime: PhantomData<T>,
}

/// Funds have been staked to an account via the StakeManager smart contract
#[derive(Call, Encode)]
pub struct StakedCall<'a, T: StakeManager> {
    /// Runtime marker
    _runtime: PhantomData<T>,

    /// Call arguments
    // ??
    // account_id: <<Signature as Verify>::Signer as IdentifyAccount>::AccountId,
    account_id: &'a state_chain_runtime::AccountId,

    amount: T::TokenAmount,

    refund_address: &'a T::EthereumAddress,
}

#[derive(Call, Encode)]
pub struct WitnessStakedCall<T: StakeManager> {
    /// Runtime marker
    _runtime: PhantomData<T>,

    staker_account_id: AccountId32,

    amount: T::TokenAmount,

    refund_address: T::EthereumAddress,
}

// The order of these fields matter for decoding
#[derive(Clone, Debug, Eq, PartialEq, Event, Encode, Decode, Serialize, Deserialize)]
pub struct ClaimSigRequestedEvent<S: StakeManager> {
    /// The AccountId of the validator wanting to claim
    pub who: AccountId32,

    pub eth_address: [u8; 20],

    pub nonce: u64,

    pub amount: u128,

    pub _phantom: PhantomData<S>,
}
// The order of these fields matter for decoding
#[derive(Clone, Debug, Eq, PartialEq, Event, Decode, Encode, Serialize, Deserialize)]
pub struct StakedEvent<S: StakeManager> {
    pub who: AccountId32,

    pub stake_added: u128,

    pub total_stake: u128,

    pub _phantom: PhantomData<S>,
}

// The order of these fields matter for decoding
#[derive(Clone, Debug, Eq, PartialEq, Event, Decode, Encode, Serialize, Deserialize)]
pub struct ClaimedEvent<S: StakeManager> {
    pub who: AccountId32,

    pub amount: u128,

    pub _phantom: PhantomData<S>,
}

// The order of these fields matter for decoding
#[derive(Clone, Debug, Eq, PartialEq, Event, Decode, Encode, Serialize, Deserialize)]
pub struct StakeRefundEvent<S: StakeManager> {
    pub who: AccountId32,

    pub amount: u128,

    pub eth_address: [u8; 20],

    pub _phantom: PhantomData<S>,
}

// The order of these fields matter for decoding
#[derive(Clone, Debug, Eq, PartialEq, Event, Decode, Encode, Serialize, Deserialize)]
pub struct ClaimSignatureIssuedEvent<S: StakeManager> {
    pub who: AccountId32,

    pub amount: u128,

    pub nonce: u64,

    pub eth_address: [u8; 20],

    pub signature: Signature,

    pub _phantom: PhantomData<S>,
}

/// Wrapper for all Staking event types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum StakingEvent<S: StakeManager> {
    ClaimSigRequestedEvent(ClaimSigRequestedEvent<S>),

    ClaimSignatureIssuedEvent(ClaimSignatureIssuedEvent<S>),

    StakedEvent(StakedEvent<S>),

    StakeRefundEvent(StakeRefundEvent<S>),

    ClaimedEvent(ClaimedEvent<S>),
}

impl From<ClaimSigRequestedEvent<StateChainRuntime>> for SCEvent {
    fn from(claim_sig_requested: ClaimSigRequestedEvent<StateChainRuntime>) -> Self {
        SCEvent::StakingEvent(StakingEvent::ClaimSigRequestedEvent(claim_sig_requested))
    }
}

impl From<ClaimSignatureIssuedEvent<StateChainRuntime>> for SCEvent {
    fn from(claim_sig_issued: ClaimSignatureIssuedEvent<StateChainRuntime>) -> Self {
        SCEvent::StakingEvent(StakingEvent::ClaimSignatureIssuedEvent(claim_sig_issued))
    }
}

impl From<ClaimedEvent<StateChainRuntime>> for SCEvent {
    fn from(claimed: ClaimedEvent<StateChainRuntime>) -> Self {
        SCEvent::StakingEvent(StakingEvent::ClaimedEvent(claimed))
    }
}

impl From<StakedEvent<StateChainRuntime>> for SCEvent {
    fn from(staked: StakedEvent<StateChainRuntime>) -> Self {
        SCEvent::StakingEvent(StakingEvent::StakedEvent(staked))
    }
}

impl From<StakeRefundEvent<StateChainRuntime>> for SCEvent {
    fn from(stake_refund: StakeRefundEvent<StateChainRuntime>) -> Self {
        SCEvent::StakingEvent(StakingEvent::StakeRefundEvent(stake_refund))
    }
}

#[cfg(test)]
mod tests {
    use crate::sc_observer::runtime::StateChainRuntime;

    use super::*;

    use codec::Encode;
    use pallet_cf_staking::Config;
    use state_chain_runtime::Runtime as SCRuntime;

    use sp_keyring::AccountKeyring;

    #[test]
    fn claim_sig_requested_decode_test() {
        let who = AccountKeyring::Alice.to_account_id();
        let eth_address: [u8; 20] = [
            00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 02, 01,
        ];
        let event: <SCRuntime as Config>::Event =
            pallet_cf_staking::Event::<SCRuntime>::ClaimSigRequested(
                who.clone(),
                eth_address,
                123u64,
                123u128,
            )
            .into();

        let encoded_claim_sig_requested = event.encode();

        // the first 2 bytes are (module_index, event_variant_index), these can be stripped
        let encoded_claim_sig_requested = encoded_claim_sig_requested[2..].to_vec();

        let decoded_event = ClaimSigRequestedEvent::<StateChainRuntime>::decode(
            &mut &encoded_claim_sig_requested[..],
        )
        .unwrap();

        let expecting = ClaimSigRequestedEvent {
            who,
            eth_address,
            nonce: 123u64,
            amount: 123u128,
            _phantom: PhantomData,
        };

        assert_eq!(decoded_event, expecting);
    }

    #[test]
    fn staked_decode_test() {
        let who = AccountKeyring::Alice.to_account_id();

        let event: <SCRuntime as Config>::Event =
            pallet_cf_staking::Event::<SCRuntime>::Staked(who.clone(), 100u128, 150u128).into();

        let encoded_staked = event.encode();

        // the first 2 bytes are (module_index, event_variant_index), these can be stripped
        let encoded_staked = encoded_staked[2..].to_vec();

        let decoded_event =
            StakedEvent::<StateChainRuntime>::decode(&mut &encoded_staked[..]).unwrap();

        let expecting = StakedEvent {
            who,
            stake_added: 100u128,
            total_stake: 150u128,
            _phantom: PhantomData,
        };

        assert_eq!(decoded_event, expecting);
    }

    #[test]
    fn claimed_decode_test() {
        let who = AccountKeyring::Alice.to_account_id();

        let event: <SCRuntime as Config>::Event =
            pallet_cf_staking::Event::<SCRuntime>::Claimed(who.clone(), 150u128).into();

        let encoded_claimed = event.encode();

        // the first 2 bytes are (module_index, event_variant_index), these can be stripped
        let encoded_claimed = encoded_claimed[2..].to_vec();

        let decoded_event =
            ClaimedEvent::<StateChainRuntime>::decode(&mut &encoded_claimed[..]).unwrap();

        let expecting = ClaimedEvent {
            who,
            amount: 150u128,
            _phantom: PhantomData,
        };

        assert_eq!(decoded_event, expecting);
    }

    #[test]
    fn stake_refund_decode_test() {
        let who = AccountKeyring::Alice.to_account_id();

        let eth_address: [u8; 20] = [
            00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 02, 01,
        ];

        let event: <SCRuntime as Config>::Event =
            pallet_cf_staking::Event::<SCRuntime>::StakeRefund(who.clone(), 150u128, eth_address)
                .into();

        let encoded_stake_refund = event.encode();

        // the first 2 bytes are (module_index, event_variant_index), these can be stripped
        let encoded_stake_refund = encoded_stake_refund[2..].to_vec();

        let decoded_event =
            StakeRefundEvent::<StateChainRuntime>::decode(&mut &encoded_stake_refund[..]).unwrap();

        let expecting = StakeRefundEvent {
            who,
            amount: 150u128,
            eth_address,
            _phantom: PhantomData,
        };

        assert_eq!(decoded_event, expecting);
    }

    #[test]
    fn claim_sig_issued_decode_test() {
        let who = AccountKeyring::Alice.to_account_id();

        let eth_address: [u8; 20] = [
            00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 02, 01,
        ];

        let sig: [u8; 65] = [0; 65];

        let sig = Signature(sig);

        let event: <SCRuntime as Config>::Event =
            pallet_cf_staking::Event::<SCRuntime>::ClaimSignatureIssued(
                who.clone(),
                150u128,
                1u64,
                eth_address,
                sig.clone(),
            )
            .into();

        let encoded_claim_sig_issued = event.encode();

        // the first 2 bytes are (module_index, event_variant_index), these can be stripped
        let encoded_claim_sig_issued = encoded_claim_sig_issued[2..].to_vec();

        let decoded_event = ClaimSignatureIssuedEvent::<StateChainRuntime>::decode(
            &mut &encoded_claim_sig_issued[..],
        )
        .unwrap();

        let expecting = ClaimSignatureIssuedEvent {
            who,
            amount: 150u128,
            nonce: 1u64,
            eth_address,
            signature: sig,
            _phantom: PhantomData,
        };

        assert_eq!(decoded_event, expecting);
    }
}
