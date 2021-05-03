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

#[module]
pub trait Staking: System {}

#[derive(Clone, Debug, Eq, PartialEq, Event, Decode, Serialize)]
pub struct ClaimSigRequestedEvent<S: Staking> {
    /// The AccountId of the validator wanting to claim
    pub who: <S as System>::AccountId,

    pub eth_address: [u8; 20],

    pub nonce: u64,

    pub amount: u128,
}

#[derive(Clone, Debug, Eq, PartialEq, Event, Decode, Serialize)]
pub struct ClaimedEvent<S: Staking> {
    pub who: <S as System>::AccountId,
    pub amount: u128,
    pub nonce: u32,
    pub address: String,
    pub signature: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::witness::sc::runtime::StateChainRuntime;
    use codec::Encode;
    use pallet_cf_staking::{Config, Event};
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

        // println!("Encoded event: {:#?}", encoded_claim_sig_requested);
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
}
