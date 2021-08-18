//! Implements subxt support for the staking pallet

use std::marker::PhantomData;
use std::time::Duration;

use codec::{Decode, Encode, FullCodec};
use frame_support::pallet_prelude::*;
use sp_core::U256;
use sp_runtime::traits::{AtLeast32BitUnsigned, CheckedSub};
use substrate_subxt::{module, sp_core::crypto::AccountId32, system::System, Event};

use serde::{Deserialize, Serialize};

use crate::state_chain::{runtime::StateChainRuntime, sc_event::SCEvent};

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

type MsgHash = U256;
type EthereumAddress = [u8; 20];
type Signature = U256;

// The order of these fields matter for decoding
#[derive(Clone, Debug, Eq, PartialEq, Event, Encode, Decode, Serialize, Deserialize)]
pub struct ClaimSigRequestedEvent<S: Staking> {
    /// The AccountId of the validator wanting to claim
    pub who: AccountId32,

    pub msg_hash: MsgHash,

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

    pub eth_address: EthereumAddress,

    pub _runtime: PhantomData<S>,
}

// The order of these fields matter for decoding
#[derive(Clone, Debug, Eq, PartialEq, Event, Decode, Encode, Serialize, Deserialize)]
pub struct ClaimSignatureIssuedEvent<S: Staking> {
    pub msg_hash: MsgHash,

    pub nonce: u64,

    pub signature: Signature,

    pub who: AccountId32,

    pub amount: FlipBalance,

    pub eth_address: EthereumAddress,

    pub expiry: Duration,

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

/// Derives an enum for the listed events and corresponding implementations of `From`.
macro_rules! impl_staking_event_enum {
    ( $( $name:tt ),+ ) => {
        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
        pub enum StakingEvent<S: Staking> {
            $(
                $name($name<S>),
            )+
        }

        $(
            impl From<$name<StateChainRuntime>> for SCEvent {
                fn from(staking_event: $name<StateChainRuntime>) -> Self {
                    SCEvent::StakingEvent(StakingEvent::$name(staking_event))
                }
            }
        )+
    };
}

impl_staking_event_enum!(
    StakedEvent,
    ClaimSettledEvent,
    StakeRefundEvent,
    ClaimSigRequestedEvent,
    ClaimSignatureIssuedEvent,
    AccountRetired,
    AccountActivated,
    ClaimExpired
);

#[cfg(test)]
mod tests {
    use crate::state_chain::runtime::StateChainRuntime;

    use super::*;

    use codec::Encode;
    use pallet_cf_staking::Config;

    use sp_core::U256;
    use sp_keyring::AccountKeyring;

    const ETH_ADDRESS: [u8; 20] = [
        00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 02, 01,
    ];

    #[test]
    fn claim_sig_requested_decode_test() {
        let who = AccountKeyring::Alice.to_account_id();
        let msg_hash = MsgHash::from([21u8; 32]);

        let event: <StateChainRuntime as Config>::Event =
            pallet_cf_staking::Event::<StateChainRuntime>::ClaimSigRequested(who.clone(), msg_hash)
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
            msg_hash,
            _runtime: PhantomData,
        };

        assert_eq!(decoded_event, expecting);
    }

    #[test]
    fn staked_decode_test() {
        let who = AccountKeyring::Alice.to_account_id();

        let event: <StateChainRuntime as Config>::Event =
            pallet_cf_staking::Event::<StateChainRuntime>::Staked(who.clone(), 100u128, 150u128)
                .into();

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

        let event: <StateChainRuntime as Config>::Event =
            pallet_cf_staking::Event::<StateChainRuntime>::ClaimSettled(who.clone(), 150u128)
                .into();

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

        let event: <StateChainRuntime as Config>::Event = pallet_cf_staking::Event::<
            StateChainRuntime,
        >::StakeRefund(
            who.clone(), 150u128, ETH_ADDRESS
        )
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

        let msg_hash = U256::from([0u8; 32]);
        let sig = U256::zero();
        let expiry = Duration::from_secs(1);

        let event: <StateChainRuntime as Config>::Event =
            pallet_cf_staking::Event::<StateChainRuntime>::ClaimSignatureIssued(
                msg_hash,
                1u64,
                sig.clone(),
                who.clone(),
                150u128,
                ETH_ADDRESS,
                expiry,
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
            msg_hash,
            nonce: 1u64,
            signature: sig,
            who,
            amount: 150u128,
            eth_address: ETH_ADDRESS,
            expiry,
            _runtime: PhantomData,
        };

        assert_eq!(decoded_event, expecting);
    }

    #[test]
    fn account_retired_decode_test() {
        let who = AccountKeyring::Alice.to_account_id();

        let event: <StateChainRuntime as Config>::Event =
            pallet_cf_staking::Event::<StateChainRuntime>::AccountRetired(who.clone()).into();

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

        let event: <StateChainRuntime as Config>::Event =
            pallet_cf_staking::Event::<StateChainRuntime>::AccountActivated(who.clone()).into();

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

        let event: <StateChainRuntime as Config>::Event = pallet_cf_staking::Event::<
            StateChainRuntime,
        >::ClaimExpired(
            who.clone(), nonce, flip_balance
        )
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
