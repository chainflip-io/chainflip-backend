// Implements support for the staking module

use std::marker::PhantomData;
use std::time::Duration;

use codec::{Decode, Encode, FullCodec};
use frame_support::pallet_prelude::*;
use sp_runtime::traits::{AtLeast32BitUnsigned, CheckedSub};
use substrate_subxt::{module, sp_core::crypto::AccountId32, system::System, Call, Event};

use serde::{Deserialize, Serialize};
use sp_core::ecdsa::Signature;

use super::{runtime::StateChainRuntime, sc_event::SCEvent};

type Nonce = u64;
type FlipBalance = u128;

#[module]
pub trait Staking: System {
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
pub struct WitnessStakedCall<T: Staking> {
    /// Runtime marker
    _runtime: PhantomData<T>,

    staker_account_id: AccountId32,

    amount: T::TokenAmount,

    tx_hash: [u8; 32],
}

#[derive(Call, Encode)]
pub struct WitnessClaimedCall<T: Staking> {
    /// Runtime marker
    _runtime: PhantomData<T>,

    // Account id of the claiming account
    account_id: AccountId32,

    amount: T::TokenAmount,

    tx_hash: [u8; 32],
}

// The order of these fields matter for decoding
#[derive(Clone, Debug, Eq, PartialEq, Event, Encode, Decode, Serialize, Deserialize)]
pub struct ClaimSigRequestedEvent<S: Staking> {
    /// The AccountId of the validator wanting to claim
    pub who: AccountId32,

    pub msg_hash: [u8; 32],

    pub _runtime: PhantomData<S>,
}

// The order of these fields matter for decoding
#[derive(Clone, Debug, Eq, PartialEq, Event, Decode, Encode, Serialize, Deserialize)]
pub struct StakedEvent<S: Staking> {
    pub who: AccountId32,

    pub stake_added: FlipBalance,

    pub total_stake: FlipBalance,

    pub _runtime: PhantomData<S>,
}

// The order of these fields matter for decoding
#[derive(Clone, Debug, Eq, PartialEq, Event, Decode, Encode, Serialize, Deserialize)]
pub struct ClaimSettledEvent<S: Staking> {
    pub who: AccountId32,

    pub amount: FlipBalance,

    pub _runtime: PhantomData<S>,
}

// The order of these fields matter for decoding
#[derive(Clone, Debug, Eq, PartialEq, Event, Decode, Encode, Serialize, Deserialize)]
pub struct StakeRefundEvent<S: Staking> {
    pub who: AccountId32,

    pub amount: FlipBalance,

    pub eth_address: [u8; 20],

    pub _runtime: PhantomData<S>,
}

// The order of these fields matter for decoding
#[derive(Clone, Debug, Eq, PartialEq, Event, Decode, Encode, Serialize, Deserialize)]
pub struct ClaimSignatureIssuedEvent<S: Staking> {
    pub who: AccountId32,

    pub amount: FlipBalance,

    pub nonce: Nonce,

    pub eth_address: [u8; 20],

    pub expiry: Duration,

    pub signature: Signature,

    pub _runtime: PhantomData<S>,
}

#[derive(Clone, Debug, Eq, PartialEq, Event, Decode, Encode, Serialize, Deserialize)]
pub struct AccountRetired<S: Staking> {
    pub who: AccountId32,

    pub _runtime: PhantomData<S>,
}

#[derive(Clone, Debug, Eq, PartialEq, Event, Decode, Encode, Serialize, Deserialize)]
pub struct AccountActivated<S: Staking> {
    pub who: AccountId32,

    pub _runtime: PhantomData<S>,
}

#[derive(Clone, Debug, Eq, PartialEq, Event, Decode, Encode, Serialize, Deserialize)]
pub struct ClaimExpired<S: Staking> {
    pub who: AccountId32,

    pub nonce: Nonce,

    pub flip_balance: FlipBalance,

    pub _runtime: PhantomData<S>,
}

/// Wrapper for all Staking event types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum StakingEvent<S: Staking> {
    StakedEvent(StakedEvent<S>),

    ClaimSettledEvent(ClaimSettledEvent<S>),

    StakeRefundEvent(StakeRefundEvent<S>),

    ClaimSigRequestedEvent(ClaimSigRequestedEvent<S>),

    ClaimSignatureIssuedEvent(ClaimSignatureIssuedEvent<S>),

    AccountRetired(AccountRetired<S>),

    AccountActivated(AccountActivated<S>),

    ClaimExpired(ClaimExpired<S>),
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

impl From<ClaimSettledEvent<StateChainRuntime>> for SCEvent {
    fn from(claimed: ClaimSettledEvent<StateChainRuntime>) -> Self {
        SCEvent::StakingEvent(StakingEvent::ClaimSettledEvent(claimed))
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

impl From<AccountRetired<StateChainRuntime>> for SCEvent {
    fn from(account_retired: AccountRetired<StateChainRuntime>) -> Self {
        SCEvent::StakingEvent(StakingEvent::AccountRetired(account_retired))
    }
}

impl From<AccountActivated<StateChainRuntime>> for SCEvent {
    fn from(account_activated: AccountActivated<StateChainRuntime>) -> Self {
        SCEvent::StakingEvent(StakingEvent::AccountActivated(account_activated))
    }
}

impl From<ClaimExpired<StateChainRuntime>> for SCEvent {
    fn from(claim_expired: ClaimExpired<StateChainRuntime>) -> Self {
        SCEvent::StakingEvent(StakingEvent::ClaimExpired(claim_expired))
    }
}

#[cfg(test)]
mod tests {
    use crate::state_chain::runtime::StateChainRuntime;

    use super::*;

    use codec::Encode;
    use pallet_cf_staking::Config;
    use state_chain_runtime::Runtime as SCRuntime;

    use sp_keyring::AccountKeyring;

    const ETH_ADDRESS: [u8; 20] = [
        00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 02, 01,
    ];

    #[test]
    fn claim_sig_requested_decode_test() {
        let who = AccountKeyring::Alice.to_account_id();
        let msg_hash = [21u8; 32];

        let event: <SCRuntime as Config>::Event =
            pallet_cf_staking::Event::<SCRuntime>::ClaimSigRequested(who.clone(), msg_hash).into();

        let encoded_claim_sig_requested = event.encode();

        // the first 2 bytes are (module_index, event_variant_index), these can be stripped
        let encoded_claim_sig_requested = encoded_claim_sig_requested[2..].to_vec();

        let decoded_event = ClaimSigRequestedEvent::<StateChainRuntime>::decode(
            &mut &encoded_claim_sig_requested[..],
        )
        .unwrap();

        let expecting = ClaimSigRequestedEvent {
            who,
            msg_hash,
            _runtime: PhantomData,
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
            _runtime: PhantomData,
        };

        assert_eq!(decoded_event, expecting);
    }

    #[test]
    fn claimed_decode_test() {
        let who = AccountKeyring::Alice.to_account_id();

        let event: <SCRuntime as Config>::Event =
            pallet_cf_staking::Event::<SCRuntime>::ClaimSettled(who.clone(), 150u128).into();

        let encoded_claimed = event.encode();

        // the first 2 bytes are (module_index, event_variant_index), these can be stripped
        let encoded_claimed = encoded_claimed[2..].to_vec();

        let decoded_event =
            ClaimSettledEvent::<StateChainRuntime>::decode(&mut &encoded_claimed[..]).unwrap();

        let expecting = ClaimSettledEvent {
            who,
            amount: 150u128,
            _runtime: PhantomData,
        };

        assert_eq!(decoded_event, expecting);
    }

    #[test]
    fn stake_refund_decode_test() {
        let who = AccountKeyring::Alice.to_account_id();

        let event: <SCRuntime as Config>::Event =
            pallet_cf_staking::Event::<SCRuntime>::StakeRefund(who.clone(), 150u128, ETH_ADDRESS)
                .into();

        let encoded_stake_refund = event.encode();

        // the first 2 bytes are (module_index, event_variant_index), these can be stripped
        let encoded_stake_refund = encoded_stake_refund[2..].to_vec();

        let decoded_event =
            StakeRefundEvent::<StateChainRuntime>::decode(&mut &encoded_stake_refund[..]).unwrap();

        let expecting = StakeRefundEvent {
            who,
            amount: 150u128,
            eth_address: ETH_ADDRESS,
            _runtime: PhantomData,
        };

        assert_eq!(decoded_event, expecting);
    }

    #[test]
    fn claim_sig_issued_decode_test() {
        let who = AccountKeyring::Alice.to_account_id();

        let sig: [u8; 65] = [0; 65];

        let sig = Signature(sig);
        let expiry = Duration::from_secs(1);

        let event: <SCRuntime as Config>::Event =
            pallet_cf_staking::Event::<SCRuntime>::ClaimSignatureIssued(
                who.clone(),
                150u128,
                1u64,
                ETH_ADDRESS,
                expiry,
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
            eth_address: ETH_ADDRESS,
            signature: sig,
            expiry,
            _runtime: PhantomData,
        };

        assert_eq!(decoded_event, expecting);
    }

    #[test]
    fn account_retired_decode_test() {
        let who = AccountKeyring::Alice.to_account_id();

        let event: <SCRuntime as Config>::Event =
            pallet_cf_staking::Event::<SCRuntime>::AccountRetired(who.clone()).into();

        let encoded_account_retired = event.encode();

        // the first 2 bytes are (module_index, event_variant_index), these can be stripped
        let encoded_account_retired = encoded_account_retired[2..].to_vec();

        let decoded_event =
            AccountRetired::<StateChainRuntime>::decode(&mut &encoded_account_retired[..]).unwrap();

        let expecting = AccountRetired {
            who,
            _runtime: PhantomData,
        };

        assert_eq!(decoded_event, expecting);
    }

    #[test]
    fn account_activated_decode_test() {
        let who = AccountKeyring::Alice.to_account_id();

        let event: <SCRuntime as Config>::Event =
            pallet_cf_staking::Event::<SCRuntime>::AccountActivated(who.clone()).into();

        let encoded_account_activated = event.encode();

        // the first 2 bytes are (module_index, event_variant_index), these can be stripped
        let encoded_account_activated = encoded_account_activated[2..].to_vec();

        let decoded_event =
            AccountActivated::<StateChainRuntime>::decode(&mut &encoded_account_activated[..])
                .unwrap();

        let expecting = AccountActivated {
            who,
            _runtime: PhantomData,
        };

        assert_eq!(decoded_event, expecting);
    }

    #[test]
    fn claim_expired_decode_test() {
        let who = AccountKeyring::Alice.to_account_id();

        let nonce = 123u64;

        let flip_balance = 1000u128;

        let event: <SCRuntime as Config>::Event =
            pallet_cf_staking::Event::<SCRuntime>::ClaimExpired(who.clone(), nonce, flip_balance)
                .into();

        let encoded_account_retired = event.encode();

        // the first 2 bytes are (module_index, event_variant_index), these can be stripped
        let encoded_account_retired = encoded_account_retired[2..].to_vec();

        let decoded_event =
            ClaimExpired::<StateChainRuntime>::decode(&mut &encoded_account_retired[..]).unwrap();

        let expecting = ClaimExpired {
            who,
            nonce,
            flip_balance,
            _runtime: PhantomData,
        };

        assert_eq!(decoded_event, expecting);
    }
}
