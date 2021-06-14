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

    use frame_support::sp_runtime::AccountId32;
    use sp_runtime::MultiSigner;
    use state_chain_runtime::Origin;
    use substrate_subxt::extrinsic::{self, SignedPayload};
    use substrate_subxt::{DefaultNodeRuntime, EventSubscription};
    // use frame_system::pallet_prelude::OriginFor;
    // use state_chain_runtime::OriginFor;

    use super::*;

    use crate::sc_observer::staking::{MinExtCallExt, StakedCallExt, WitnessStakedCallExt};
    use crate::settings;
    use crate::settings::StateChain;

    use substrate_subxt::balances::{TransferCall, TransferCallExt, TransferEvent};
    use substrate_subxt::system::SetCodeCallExt;
    use substrate_subxt::Signer;

    // use substrate_subxt::extrinsic;

    // #[tokio::test]
    // async fn create_raw_payload_test() {

    //     let alice = AccountKeyring::Alice.to_account_id();
    //     let account_nonce = self.account(alice).await.unwrap().nonce;
    //     let version = self.runtime_version.spec_version;
    //     let genesis_hash = self.genesis_hash;
    //     let call = self
    //         .metadata()
    //         .module_with_calls(&call.module)
    //         .and_then(|module| module.call(&call.function, call.args))?;
    //     let extra: extrinsic::DefaultExtra<T> =
    //         extrinsic::DefaultExtra::new(version, account_nonce, genesis_hash);
    //     let raw_payload = SignedPayload::new(call, extra.extra())?;
    //     Ok(raw_payload.encode())
    // }

    #[tokio::test]
    async fn create_raw_payload_test() {
        let settings = settings::test_utils::new_test_settings().unwrap();
        let subxt_client = create_subxt_client(settings.state_chain).await.unwrap();

        // println!("Here's the metadata: {:#?}", metadata);
    }

    // #[tokio::test]
    // async fn broadcast_raw_data() {
    //     let settings = settings::test_utils::new_test_settings().unwrap();
    //     let subxt_client = create_subxt_client(settings.state_chain).await.unwrap();
    //     let data = "0x2bb96f6ff718b272b42f7ded0f6d30b979bfefcb368fd5663f645b766b61aefb";
    //     let expected_data_after_signed = "0xa1018400d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d011a20d5f1685e79632c3a9acf843ef918353f4177e04aea95189dc156d819262a1a67c0a108be8fb971787088c40b2ae1bb0f6fbcd8e9aa01a553249c6ae53c816502080d00";
    //     let signer = PairSigner::new(AccountKeyring::Alice.pair());

    //     let genesis_hash = "xc786272a77bc0d76c9836073b0917cd34b0e012ab83538";

    //     let runtime_version = "0x4073746174652d636861696e2d6e6f64654073746174652d636861696e2d6e6f646501000000640000000100000024df6acb689907609b0300000037e397fc7c91f5e40100000040fe3ad401f8959a04000000d2bc9897eed08f1502000000f78b278be53f454c02000000dd718d5cc53262d401000000ab3c0572291feb8b01000000ed99c5acb25eedf502000000bc9d89904f5b923f0100000001000000";

    //     let call = TransferCall {
    //         _runtime: PhantomData,
    //     };

    //     let signed_xt = extrinsic::create_signed(
    //         runtime_version.into(),
    //         genesis_hash.into(),
    //         0.into(),
    //         call.encode(),
    //         &signer,
    //     )
    //     .await;

    //     // let signed = signer.sign(call);

    //     // let result = subxt_client.submit(&signer).await;

    //     println!("signed data = {:#?}", &signed_data);
    // }

    #[tokio::test]
    async fn test_transfer_example() {
        let signer = PairSigner::new(AccountKeyring::Alice.pair());
        let dest = AccountKeyring::Alice.to_account_id().into();

        let client = ClientBuilder::<StateChainRuntime>::new()
            .skip_type_sizes_check()
            .build()
            .await
            .unwrap();
        let result = client.transfer(&signer, &dest, 10_000).await;

        println!("The result is: {:#?}", result);

        // println!("The result is: {:#?}", result);

        // if let Some(event) = result.transfer()? {
        //     println!("Balance transfer success: value: {:?}", event.amount);
        // } else {
        //     println!("Failed to find Balances::Transfer Event");
        // }
        // Ok(())
    }

    #[tokio::test]
    async fn submit_xt_test() {
        let settings = settings::test_utils::new_test_settings().unwrap();
        let subxt_client = create_subxt_client(settings.state_chain).await.unwrap();
        // let ecdsa_signer = sp_runtime::app_crypto::ecdsa::Pair::default();
        let signer = PairSigner::new(AccountKeyring::Alice.pair());

        let alice = AccountKeyring::Alice.to_account_id();

        // let bob_multi = sp_runtime::MultiAddress::Address32(bob.into());

        let eth_address: [u8; 20] = [
            00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 02, 01,
        ];

        // let result = subxt_client.transfer(&signer, &bob_multi, 12).await;

        // println!("result: {:#?}", result);

        let result = subxt_client
            .set_code(&signer, &[0u8, 0u8, 0u8, 0u8])
            .await
            .unwrap();

        // let result = subxt_client.min_ext(&signer).await;

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
