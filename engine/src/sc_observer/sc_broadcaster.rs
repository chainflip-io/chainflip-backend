// first we need to connect to the state chain

// We need to use the keys of the state chain (need the filepath to these)

// we need to be able to read from the message queue

// We need to be able to submit extrinsics (signed and unsigned to the state chain)

// Start with submitting an extrinsic of the easiest kind

use sp_keyring::AccountKeyring;

use substrate_subxt::{Client, ClientBuilder, PairSigner};

use super::{helpers::create_subxt_client, runtime::StateChainRuntime};
use crate::{
    mq::{
        nats_client::{NatsMQClient, NatsMQClientFactory},
        IMQClient, IMQClientFactory,
    },
    settings::Settings,
};

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
}

impl SCBroadcaster {
    pub async fn new(settings: Settings) -> Self {
        let sc_client = create_subxt_client(settings.state_chain).await.unwrap();

        // TODO: Use the factory better here now
        // let mq_client = *M::connect(settings.message_queue).await.unwrap();

        let mq_client_factory = NatsMQClientFactory::new(&settings.message_queue);
        let mq_client = *mq_client_factory.create().await.unwrap();

        SCBroadcaster {
            mq_client,
            sc_client,
        }
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

    use crate::sc_observer::staking::{StakedCallExt, WitnessStakedCallExt};
    use crate::sc_observer::validator::ForceRotationCallExt;
    use crate::settings;
    use crate::settings::StateChain;

    use hex_literal::hex;
    use substrate_subxt::sudo::SudoCallExt;
    use substrate_subxt::system::SetCodeCallExt;
    use substrate_subxt::Signer;

    #[tokio::test]
    async fn create_raw_payload_test() {
        let settings = settings::test_utils::new_test_settings().unwrap();
        let subxt_client = create_subxt_client(settings.state_chain).await.unwrap();
    }

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
                eth_address,
                tx_hash,
            )
            .await;

        println!("Result is: {:#?}", result);

        assert!(result.is_ok());
    }
}

// Error I was getting when submitting a staked event from the frontend:
// Unable to decode storage system.account: entry 0:: createType(AccountInfo):: {"nonce":"Index","consumers":"RefCount","providers":"RefCount","sufficients":"RefCount","data":"AccountData"}:: Decoded input doesn't match input, received 0x0000000001000000010000000000000000000010000000000000000000000000…0000000000000000000000000000000000000000000000000000000000000000 (76 bytes), created 0x0000000001000000010000000000000000000010000000000000000000000000…0000000000000000000000000000000000000000000000000000000000000000 (80 bytes)

// Unable to decode storage system.account: entry 0:: createType(AccountInfo):: {"nonce":"Index","consumers":"RefCount","providers":"RefCount","data":"AccountData"}:: Decoded input doesn't match input, received 0x000000000100000001000000 (12 bytes), created 0x0000000001000000010000000000000000000000000000000000000000000000…0000000000000000000000000000000000000000000000000000000000000000 (76 bytes)

// pub fn get_from_seed<TPublic: Public>(seed: &str) -> <TPublic::Pair as Pair>::Public {
//     TPublic::Pair::from_string(&format!("//{}", seed), None)
//         .expect("static values are valid; qed")
//         .public()
// }

// type AccountPublic = <MultiSignature as Verify>::Signer;

// /// Generate an account ID from seed.
// pub fn get_account_id_from_seed<TPublic: Public>(seed: &str) -> AccountId
// where
//     AccountPublic: From<<TPublic::Pair as Pair>::Public>,
// {
//     AccountPublic::from(get_from_seed::<TPublic>(seed)).into_account()
// }
