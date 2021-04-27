// Implements support for the staking module

use std::marker::PhantomData;

use chainflip_common::types::addresses::{Address, EthereumAddress};
use codec::{Codec, Decode, Encode};
use substrate_subxt::{
    module,
    sp_runtime::{app_crypto::RuntimePublic, traits::Member},
    system::System,
    Event,
};

#[module]
pub trait Staking: System {}

// Apparently should be an event type here
#[derive(Clone, Debug, Eq, PartialEq, Event, Decode)]
pub struct ClaimSigRequested<S: Staking> {
    /// The AccountId of the validator wanting to claim
    pub who: <S as System>::AccountId,

    pub amount: u128,

    pub nonce: u32,

    pub eth_account: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq, Event, Decode)]
pub struct Claim<S: Staking> {
    pub who: <S as System>::AccountId,
    pub amount: u128,
    pub nonce: u32,
    pub address: String,
    pub signature: String,
}
