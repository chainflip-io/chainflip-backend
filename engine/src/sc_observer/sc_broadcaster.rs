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

/// Broadcasts events to the state chain by submitting 'extrinsics'
pub struct SCBroadcaster<M: IMQClient + Send + Sync> {
    mq_client: M,
    sc_client: Client<StateChainRuntime>,
}

// impl<M: IMQClient + Send + Sync> SCBroadcaster<M> {
//     pub async fn new(settings: Settings) -> Self {
//         // TODO: Change this to be the keys from the state chain
//         let signer = PairSigner::new(AccountKeyring::Alice.pair());
//         let client = ClientBuilder::<StateChainRuntime>::new()
//             .build()
//             .await
//             .unwrap();

//         let sc_client = create_subxt_client(settings.state_chain).await.unwrap();

//         let mq_client = M::connect(settings.message_queue).await.unwrap();

//         SCBroadcaster {
//             mq_client,
//             sc_client,
//         }
//     }
// }

// #[cfg(test)]
// mod tests {

//     use state_chain_runtime::UncheckedExtrinsic;

//     use super::*;

//     // #[tokio::test]
//     // async fn submit_xt_test() {
//     //     let client = ClientBuilder::<StateChainRuntime>::new()
//     //         .build()
//     //         .await
//     //         .unwrap();

//     //     //         let extrinsic = UncheckedExtrinsic {
//     //     // "
//     //     //             function:
//     //     //         };

//     //     client.submit_extrinsic(extrinsic)
//     // }

//     #[test]
//     fn test_new_broadcaster() {
//         // let settings = {

//         // }
//         // let broadcaster = SCBroadcaster::new();

//         // didn't panic, yay!
//     }
// }
