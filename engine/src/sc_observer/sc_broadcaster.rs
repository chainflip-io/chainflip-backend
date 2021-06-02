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

    use crate::sc_observer::staking::{MinExtCallExt, StakedCallExt, WitnessStakedCallExt};
    use crate::settings;
    use crate::settings::StateChain;

    use substrate_subxt::system::AccountStoreExt;

    #[tokio::test]
    async fn submit_xt_test() {
        let settings = settings::test_utils::new_test_settings().unwrap();
        let subxt_client = create_subxt_client(settings.state_chain).await.unwrap();

        let signer = PairSigner::new(AccountKeyring::Alice.pair());

        let alice = AccountKeyring::Alice.to_account_id();

        let eth_address: [u8; 20] = [
            00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 02, 01,
        ];

        let result = subxt_client.account(&alice, None).await;

        println!("result: {:#?}", result);

        // let result = subxt_client
        //     .set_code(&signer, &[1u8, 2u8, 3u8, 4u8])
        //     .await
        //     .unwrap();

        let result = subxt_client.min_ext(&signer).await;

        println!("Result: {:#?}", result);

        // let result = subxt_client
        //     .witness_staked(&signer, alice, 123u128, eth_address)
        //     .await;

        // TODO: Ensure this is actually correct.
        // let result = subxt_client
        //     .staked(&signer, &alice, 100u128, &eth_address)
        //     .await;

        // println!("result: {:#?}", result);

        // result.unwrap();

        // println!("Here's the result: {:#?}", result);
    }
}

// Error I was getting when submitting a staked event from the frontend:
// Unable to decode storage system.account: entry 0:: createType(AccountInfo):: {"nonce":"Index","consumers":"RefCount","providers":"RefCount","sufficients":"RefCount","data":"AccountData"}:: Decoded input doesn't match input, received 0x0000000001000000010000000000000000000010000000000000000000000000…0000000000000000000000000000000000000000000000000000000000000000 (76 bytes), created 0x0000000001000000010000000000000000000010000000000000000000000000…0000000000000000000000000000000000000000000000000000000000000000 (80 bytes)
