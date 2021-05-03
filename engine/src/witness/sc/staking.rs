// Implements support for the staking module

use std::marker::PhantomData;

use chainflip_common::types::addresses::{Address, EthereumAddress};
use codec::{Codec, Decode, Encode};
use serde::{Deserialize, Serialize};
use substrate_subxt::{
    module,
    sp_runtime::{app_crypto::RuntimePublic, traits::Member},
    system::System,
    Event,
};

use sp_core::ecdsa::Signature;

#[module]
pub trait Staking: System {}

#[derive(Clone, Debug, Eq, PartialEq, Event, Decode)]
pub struct ClaimSigRequestedEvent<S: Staking> {
    /// The AccountId of the validator wanting to claim
    pub who: <S as System>::AccountId,

    pub eth_address: [u8; 20],

    pub nonce: u64,

    pub amount: u128,
}

#[derive(Clone, Debug, Eq, PartialEq, Event, Decode)]
pub struct StakedEvent<S: Staking> {
    pub who: <S as System>::AccountId,

    pub stake_added: u128,

    pub total_stake: u128,
}

#[derive(Clone, Debug, Eq, PartialEq, Event, Decode)]
pub struct ClaimedEvent<S: Staking> {
    pub who: <S as System>::AccountId,

    pub amount: u128,
}

#[derive(Clone, Debug, Eq, PartialEq, Event, Decode)]
pub struct StakeRefundEvent<S: Staking> {
    pub who: <S as System>::AccountId,

    pub amount: u128,

    pub eth_address: [u8; 20],
}

#[derive(Clone, Debug, Eq, PartialEq, Event, Decode)]
pub struct ClaimSignatureIssuedEvent<S: Staking> {
    pub who: <S as System>::AccountId,

    pub amount: u128,

    pub nonce: u64,

    pub eth_address: [u8; 20],

    pub signature: Signature,
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::witness::sc::runtime::StateChainRuntime;
    use codec::Encode;
    use pallet_cf_staking::{Config, Event};
    use state_chain_runtime::{Runtime as SCRuntime, RuntimeApiImpl};

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
        };

        assert_eq!(decoded_event, expecting);
    }
}
