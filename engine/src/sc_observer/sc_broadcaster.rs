// first we need to connect to the state chain

// We need to use the keys of the state chain (need the filepath to these)

// we need to be able to read from the message queue

// We need to be able to submit extrinsics (signed and unsigned to the state chain)

// Start with submitting an extrinsic of the easiest kind

use futures::pin_mut;
use sp_core::Pair;
use sp_keyring::AccountKeyring;

use substrate_subxt::{Client, ClientBuilder, PairSigner};
use tokio_stream::StreamExt;

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

use crate::sc_observer::staking::WitnessStakedCallExt;

use anyhow::Result;

use codec::Encode;

/// TODO: make this generic again
/// Broadcasts events to the state chain by submitting 'extrinsics'
// pub struct SCBroadcaster<M: IMQClient + Send + Sync> {
//     mq_client: M,
//     sc_client: Client<StateChainRuntime>,
// }

pub struct SCBroadcaster {
    mq_client: NatsMQClient,
    sc_client: Client<StateChainRuntime>,
    // do we want to load in the keys here? how can we ensure signing with the correct
    // stuff
    // signer: PairSigner<Runtime??, sp_core::sr25519::Pair>,
}

impl SCBroadcaster {
    pub async fn new(settings: Settings) -> Self {
        let sc_client = create_subxt_client(settings.state_chain).await.unwrap();

        // TODO: Use the factory better here now
        // let mq_client = *M::connect(settings.message_queue).await.unwrap();

        // TODO: Read in the keys from a file

        let mq_client_factory = NatsMQClientFactory::new(&settings.message_queue);
        let mq_client = *mq_client_factory.create().await.unwrap();

        SCBroadcaster {
            mq_client,
            sc_client,
            // signer: alice_signer,
        }
    }

    pub async fn run(&self) -> Result<()> {
        // read in Stakemanager events'

        // let alice_signer = PairSigner::new(AccountKeyring::Alice.pair());

        let stream = self
            .mq_client
            .subscribe::<StakeManagerEvent>(Subject::StakeManager)
            .await?;

        let mut stream = pin_message_stream(stream);

        let event = stream.next().await;
        println!("Get next event: {:#?}", event);

        let event = event.unwrap().unwrap();

        let tx_hash = "0x0000000000000000000000000000000000000000000000000000000000000000";
        // match event {
        //     StakeManagerEvent::Staked(node_id, amount) => {
        //         log::trace!("Sending witness staked to state chain");
        //         self.sc_client
        //             .witness_staked(&alice_signer, node_id, amount, tx_hash);
        //     }
        //     _ => {
        //         log::warn!("Staking event not supported for SC broadcaster");
        //     }
        // }

        let err_msg = "State Chain Broadcaster has stopped running!";
        log::error!("{}", err_msg);
        Err(anyhow::Error::msg(err_msg))
    }
}

#[cfg(test)]
mod tests {

    use std::marker::PhantomData;
    use std::str::FromStr;

    use frame_support::sp_runtime::AccountId32;
    use sp_core::{sr25519, Pair, Public, H256};
    use sp_runtime::traits::{IdentifyAccount, Verify};
    use sp_runtime::{MultiSignature, MultiSigner};
    use state_chain_runtime::{AccountId, Origin};
    use substrate_subxt::extrinsic::{self, CheckGenesis, SignedPayload};
    use substrate_subxt::{DefaultNodeRuntime, EventSubscription, SignedExtension};
    // use frame_system::pallet_prelude::OriginFor;
    // use state_chain_runtime::OriginFor;

    use super::*;

    use crate::sc_observer::validator::ForceRotationCallExt;
    use crate::settings;
    use crate::settings::StateChain;

    use hex_literal::hex;
    use substrate_subxt::sudo::SudoCallExt;
    use substrate_subxt::system::SetCodeCallExt;
    use substrate_subxt::Signer;

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
    #[ignore = "needs stuff"]
    async fn sc_broadcaster_subscribe() {}
}
