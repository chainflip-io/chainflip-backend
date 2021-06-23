// first we need to connect to the state chain

// We need to use the keys of the state chain (need the filepath to these)

// we need to be able to read from the message queue

// We need to be able to submit extrinsics (signed and unsigned to the state chain)

// Start with submitting an extrinsic of the easiest kind

use futures::pin_mut;
use sp_core::Pair;
use sp_keyring::AccountKeyring;

use sp_runtime::AccountId32;
use substrate_subxt::{Client, ClientBuilder, PairSigner};
use tokio_stream::StreamExt;

use std::{fs, str::FromStr};
use substrate_subxt::Signer;

use super::{helpers::create_subxt_client, runtime::StateChainRuntime};
use crate::{
    eth::stake_manager::stake_manager::StakeManagerEvent,
    mq::{
        nats_client::{NatsMQClient, NatsMQClientFactory},
        pin_message_stream, IMQClient, IMQClientFactory, Subject,
    },
    settings::Settings,
};

use crate::state_chain::staking::{WitnessClaimedCallExt, WitnessStakedCallExt};

use anyhow::Result;

use codec::Encode;

pub struct SCBroadcaster<MQ>
where
    MQ: IMQClient + Send + Sync,
{
    mq_client: MQ,
    sc_client: Client<StateChainRuntime>,
    // do we want to load in the keys here? how can we ensure signing with the correct
    // stuff
    // signer: PairSigner<Runtime??, sp_core::sr25519::Pair>,
}

impl<MQ> SCBroadcaster<MQ>
where
    MQ: IMQClient + Send + Sync,
{
    pub async fn new(settings: Settings, mq_client: MQ) -> Self {
        let sc_client = create_subxt_client(settings.state_chain.clone())
            .await
            .unwrap();

        let key = fs::read_to_string(settings.state_chain.signing_key_path)
            .expect("Can't read in signing key");

        println!("The key is: {:#?}", key);

        SCBroadcaster {
            mq_client,
            sc_client,
            // signer: alice_signer,
        }
    }

    pub async fn run(&self) -> Result<()> {
        let stream = self
            .mq_client
            .subscribe::<StakeManagerEvent>(Subject::StakeManager)
            .await?;

        let mut stream = pin_message_stream(stream);

        // TOOD: Loop through the events here, pushing each as they come
        let event = stream.next().await;
        println!("Get next event: {:#?}", event);

        let event = event.unwrap().unwrap();

        self.submit_event(event).await?;

        let err_msg = "State Chain Broadcaster has stopped running!";
        log::error!("{}", err_msg);
        Err(anyhow::Error::msg(err_msg))
    }

    /// Submit an event to the state chain, return the tx_hash
    async fn submit_event(&self, event: StakeManagerEvent) -> Result<()> {
        let alice_signer = PairSigner::new(AccountKeyring::Alice.pair());

        match event {
            StakeManagerEvent::Staked(node_id, amount, tx_hash) => {
                log::trace!("Sending witness_staked to state chain");
                let result = self
                    .sc_client
                    .witness_staked(&alice_signer, node_id, amount, tx_hash);
                println!("Result of witness_staked xt is: {:#?}", result.await);
            }
            StakeManagerEvent::ClaimExecuted(node_id, amount, tx_hash) => {
                log::trace!("Sending claim_executed to the state chain");
                // claim executed
                let result =
                    self.sc_client
                        .witness_claimed(&alice_signer, node_id, amount, tx_hash);
                println!("The result of witness_claimed xt is: {:#?}", result.await);
            }
            _ => {
                log::warn!("Staking event not supported for SC broadcaster");
            }
        };
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    // use frame_system::pallet_prelude::OriginFor;
    // use state_chain_runtime::OriginFor;

    use super::*;

    use crate::settings;
    use crate::settings::StateChain;
    use crate::state_chain::validator::ForceRotationCallExt;

    const TX_HASH: [u8; 32] = [
        00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 02, 01, 02, 01, 02,
        01, 02, 01, 02, 01, 02, 01, 02, 01,
    ];

    #[tokio::test]
    async fn can_create_sc_broadcaster() {
        let settings = settings::test_utils::new_test_settings().unwrap();

        let mq_factory = NatsMQClientFactory::new(&settings.message_queue);

        let mq_client = mq_factory
            .create()
            .await
            .expect("Could not create MQ client");

        let sc_broadcaster = SCBroadcaster::new(settings, *mq_client).await;
    }

    // TODO: Use the SC broadcaster struct instead
    #[tokio::test]
    #[ignore = "depends on running state chain"]
    async fn submit_xt_test() {
        let settings = settings::test_utils::new_test_settings().unwrap();
        let subxt_client = create_subxt_client(settings.state_chain).await.unwrap();

        let alice = AccountKeyring::Alice.pair();
        let signer = PairSigner::new(alice);

        let eth_address: [u8; 20] = [
            00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 02, 01,
        ];

        let tx_hash: [u8; 32] = [
            00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 02, 01, 01, 01,
            01, 01, 01, 01, 01, 01, 01, 01, 01, 01,
        ];

        let result = subxt_client
            .witness_staked(
                &signer,
                AccountKeyring::Alice.to_account_id(),
                10000000u128,
                tx_hash,
            )
            .await;

        println!("Result is: {:#?}", result);

        assert!(result.is_ok());
    }

    #[tokio::test]
    #[ignore = "depends on running state chain"]
    async fn sc_broadcaster_submit_event() {
        let settings = settings::test_utils::new_test_settings().unwrap();

        let mq_factory = NatsMQClientFactory::new(&settings.message_queue);

        let mq_client = mq_factory
            .create()
            .await
            .expect("Could not create MQ client");

        let sc_broadcaster = SCBroadcaster::new(settings, *mq_client).await;

        let staked_node_id =
            AccountId32::from_str("5C4hrfjw9DjXZTzV3MwzrrAr9P1MJhSrvWGWqi1eSuziKFgU").unwrap();
        let staked_event = StakeManagerEvent::Staked(staked_node_id, 100, TX_HASH);

        let result = sc_broadcaster.submit_event(staked_event).await;

        println!("Result: ")
    }
}
