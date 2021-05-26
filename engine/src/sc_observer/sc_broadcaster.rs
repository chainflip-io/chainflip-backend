// first we need to connect to the state chain

// We need to use the keys of the state chain (need the filepath to these)

// we need to be able to read from the message queue

// We need to be able to submit extrinsics (signed and unsigned to the state chain)

// Start with submitting an extrinsic of the easiest kind

use sp_keyring::AccountKeyring;

use substrate_subxt::{Client, ClientBuilder, PairSigner};

use super::{helpers::create_subxt_client, runtime::StateChainRuntime};
use crate::{
    mq::{nats_client::NatsMQClient, IMQClient},
    settings::Settings,
};

use codec::Encode;

/// Broadcasts events to the state chain by submitting 'extrinsics'
pub struct SCBroadcaster<M: IMQClient + Send + Sync> {
    mq_client: M,
    sc_client: Client<StateChainRuntime>,
}

impl<M: IMQClient + Send + Sync> SCBroadcaster<M> {
    pub async fn new(settings: Settings) -> Self {
        let sc_client = create_subxt_client(settings.state_chain).await.unwrap();

        let mq_client = *M::connect(settings.message_queue).await.unwrap();

        SCBroadcaster {
            mq_client,
            sc_client,
        }
    }
}

#[cfg(test)]
mod tests {

    use std::marker::PhantomData;

    use frame_support::sp_runtime::AccountId32;
    use state_chain_runtime::Origin;
    // use frame_system::pallet_prelude::OriginFor;
    // use state_chain_runtime::OriginFor;

    use super::*;

    use crate::sc_observer::staking::{StakedCallExt, WitnessStakedCallExt};
    use crate::settings;
    use crate::settings::StateChain;

    use substrate_subxt::system::AccountStoreExt;

    #[tokio::test]
    async fn submit_xt_test() {
        let settings = settings::test_utils::new_test_settings().unwrap();
        let subxt_client = create_subxt_client(settings.state_chain).await.unwrap();

        // let signer = PairSigner::new(AccountKeyring::Alice.pair());

        let alice = AccountKeyring::Alice.to_account_id();

        let eth_address: [u8; 20] = [
            00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 02, 01,
        ];

        let alice_account = subxt_client.account(&alice, None).await.unwrap();

        // let result = subxt_client
        //     .witness_staked(&signer, alice, 100u128, eth_address)
        //     .await;

        // TODO: Ensure this is actually correct.
        // let result = subxt_client
        //     .staked(
        //         &signer,
        //         account_id_32.clone(),
        //         account_id_32,
        //         100u128,
        //         eth_address,
        //     )
        //     .await;

        // println!("Here's the result: {:#?}", result);
    }

    // #[test]
    // fn test_new_broadcaster() {
    //     // let settings = {

    //     // }
    //     // let broadcaster = SCBroadcaster::new();

    //     // didn't panic, yay!
    // }
}
