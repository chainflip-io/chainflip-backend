//! Implements subxt support for the staking pallet

use std::marker::PhantomData;

use codec::{Decode, Encode};
use substrate_subxt::{module, sp_core::crypto::AccountId32, system::System, Event};

use serde::{Deserialize, Serialize};

use crate::state_chain::{runtime::StateChainRuntime, sc_event::SCEvent};

pub type FlipBalance = u128;

#[module]
pub trait Staking: System {}

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
pub struct ClaimSignatureIssuedEvent<S: Staking> {
    pub node_id: AccountId32,

    pub signed_payload: Vec<u8>,

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
    ClaimSignatureIssuedEvent,
    AccountRetired,
    AccountActivated,
    ClaimExpired
);

#[cfg(test)]
mod tests {
    use super::*;

    use codec::Encode;
    use pallet_cf_staking::Config;

    use sp_keyring::AccountKeyring;

    use state_chain_runtime::Runtime as SCRuntime;

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
    fn claim_sig_issued_decode_test() {
        let who = AccountKeyring::Alice.to_account_id();

        let signed_payload = b"plz_i_want_to_claim".to_vec();

        let event: <SCRuntime as Config>::Event =
            pallet_cf_staking::Event::<SCRuntime>::ClaimSignatureIssued(
                who.clone(), 
                signed_payload.clone(),
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
            node_id: who, 
            signed_payload,
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

        let flip_balance = 1000u128;

        let event: <SCRuntime as Config>::Event =
            pallet_cf_staking::Event::<SCRuntime>::ClaimExpired(who.clone(), flip_balance)
                .into();

        let encoded_account_retired = event.encode();

        // the first 2 bytes are (module_index, event_variant_index), these can be stripped
        let encoded_account_retired = encoded_account_retired[2..].to_vec();

        let decoded_event =
            ClaimExpired::<StateChainRuntime>::decode(&mut &encoded_account_retired[..]).unwrap();

        let expecting = ClaimExpired {
            who,
            flip_balance,
            _runtime: PhantomData,
        };

        assert_eq!(decoded_event, expecting);
    }
}
