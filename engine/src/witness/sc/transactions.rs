// Implements support for the transactions module

use codec::{Codec, Decode, Encode};
use substrate_subxt::{
    module,
    sp_runtime::{app_crypto::RuntimePublic, traits::Member},
    system::System,
    Event,
};

#[module]
pub trait Transactions: System {}

// Apparently should be an event type here
#[derive(Clone, Debug, Eq, PartialEq, Event, Decode)]
pub struct DataAddedEvent<T: Transactions> {
    pub who: <T as System>::AccountId,

    pub data: Vec<u8>,
}

// #[cfg(test)]
// mod tests {
//     use super::*;

//     use substrate_subxt::PairSigner;

//     use crate::witness::sc::runtime::StateChainRuntime;

//     use sp_keyring::AccountKeyring;

//     use substrate_subxt::{subxt_test}

//     #[tokio::test]
//     async fn basic_add_data() {
//         let alice = PairSigner::<StateChainRuntime, _>::new(AccountKeyring::Alice.pair());

//     }
// }
