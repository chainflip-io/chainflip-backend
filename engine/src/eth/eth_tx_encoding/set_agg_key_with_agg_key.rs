use std::collections::HashMap;

use crate::{
    eth::key_manager::KeyManager,
    mq::{pin_message_stream, IMQClient, Subject},
    p2p::ValidatorId,
    settings,
    signing::{KeyId, MessageHash, MultisigInstruction, SigningInfo},
    state_chain::{auction::AuctionConfirmedEvent, runtime::StateChainRuntime},
    types::chain::Chain,
};

use anyhow::Result;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use sp_core::Hasher;
use sp_runtime::traits::Keccak256;
use web3::{
    ethabi::{Token, Uint},
    types::Address,
};

/// Helper function, constructs and runs the [SetAggKeyWithAggKeyEncoder] asynchronously.
pub async fn start<M: IMQClient + Clone>(
    settings: &settings::Settings,
    mq_client: M,
) -> Result<()> {
    let encoder =
        SetAggKeyWithAggKeyEncoder::new(settings.eth.key_manager_eth_address.as_ref(), mq_client)?;

    encoder.run().await
}

/// Details of a transaction to be broadcast to ethereum.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct TxDetails {
    pub contract_address: Address,
    pub data: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
pub struct FakeNewAggKey(u64, String);

#[derive(Serialize, Deserialize)]
pub struct FakeNewAggKeySigningComplete {
    hash: MessageHash,
    sig: Vec<u8>,
}

/// Reads [AuctionConfirmedEvent]s off the message queue and encodes the function call to the stake manager.
#[derive(Clone)]
struct SetAggKeyWithAggKeyEncoder<M: IMQClient> {
    mq_client: M,
    key_manager: KeyManager,
    // maps the MessageHash which gets sent to the signer with the data that the MessageHash is a hash of
    messages: HashMap<MessageHash, Vec<u8>>,
    // On genesis, where do these validators come from, to allow for the first key update
    validators: HashMap<KeyId, Vec<ValidatorId>>,
    curr_signing_key_id: Option<u64>,
    next_key_id: Option<u64>,
}

impl<M: IMQClient + Clone> SetAggKeyWithAggKeyEncoder<M> {
    fn new(key_manager_address: &str, mq_client: M) -> Result<Self> {
        let key_manager = KeyManager::load(key_manager_address)?;

        Ok(Self {
            mq_client,
            key_manager,
            messages: HashMap::new(),
            validators: HashMap::new(),
            curr_signing_key_id: None,
            next_key_id: None,
        })
    }

    async fn run(self) -> Result<()> {
        // from here we are getting signed message hashses
        let subscription = self
            .mq_client
            .subscribe::<FakeNewAggKeySigningComplete>(Subject::FakeNewAggKeySigningComplete)
            .await?;

        let subscription = pin_message_stream(subscription);

        subscription
            .for_each_concurrent(None, |msg| async {
                // in here we need to:
                // 1. Get the data from the message hash that was signed (using the `messages` field)
                // 2. Call build_tx with the required info
                // 3. Send it on its way to the eth broadcaster
                match msg {
                    Ok(ref msg) => {
                        let signed_data_for_msg = self.messages.get(&msg.hash).expect("should have been stored when asked to sign");
                        match self.build_tx(msg) {
                            Ok(ref tx_details) => {
                                self.mq_client
                                    .publish(Subject::Broadcast(Chain::ETH), tx_details)
                                    .await
                                    .unwrap_or_else(|err| {
                                        log::error!("Could not process");
                                    });
                            }
                            Err(err) => {
                                log::error!("failed to build")
                            }
                    }
                }
                Err(e) => {
                    log::error!("Unable to process claim request: {:?}.", e);
                }
            }).await;

        log::error!("ARGAADFADFLAKJSLKFJAS;DKFAS;JF");
        Ok(())
    }

    fn build_tx(&self, event: &FakeNewAggKeySigningComplete) -> Result<TxDetails> {
        let params = [
            Token::Tuple(vec![
                // SigData
                Token::Uint(todo!()), // msgHash
                Token::Uint(todo!()), // nonce
                Token::Uint(todo!()), // sig
            ]),
            Token::Tuple(vec![
                // Key
                Token::Uint(todo!()),    // pubkeyX
                Token::Uint(todo!()),    // pubkeyYparity
                Token::Address(todo!()), // nonceTimesGAddr
            ]),
        ];

        let tx_data = self
            .key_manager
            .set_agg_key_with_agg_key()
            .encode_input(&params[..])?;

        Ok(TxDetails {
            contract_address: self.key_manager.deployed_address,
            data: tx_data.into(),
        })
    }

    // temporary process (will be handled by SC eventually) that creating the payload to be signed by the multisig process
    async fn run_tx_constructor(&mut self) {
        // get events from the mq that need to be constructed
        let new_agg_key_created_stream = self
            .mq_client
            .subscribe::<FakeNewAggKey>(Subject::FakeNewAggKey)
            .await
            .unwrap();

        let mut new_agg_key_created_stream = pin_message_stream(new_agg_key_created_stream);

        // Cannot use for_each_concurrent because we need to move a &mut self, and then push updates
        // to that ref
        while let Some(event) = new_agg_key_created_stream.next().await {
            let event = event.expect("Should be an event");
            let encoded_fn_params = self
                .build_encoded_fn_params(&event)
                .expect("should be a valid encoded params");

            let hash = Keccak256::hash(&encoded_fn_params[..]);
            let message_hash = MessageHash(hash.as_bytes().to_vec());

            // store the hash and the encoded_fn_params
            self.messages
                .entry(message_hash.clone())
                .or_insert(encoded_fn_params);

            // TODO: Use the correct KeyId and vector of validators here.
            // how do we get the subset of signers from the OLD key, in order to use? Need to be collected from somewhere
            let signing_info = SigningInfo::new(KeyId(1), vec![]);
            let signing_instruction = MultisigInstruction::Sign(message_hash, signing_info);

            self.mq_client
                .publish(Subject::MultisigInstruction, &signing_instruction)
                .await
                .expect("Should publish to MQ");
        }
    }

    // Temporary a CFE method, this will be moved to the state chain
    fn build_encoded_fn_params(&self, event: &FakeNewAggKey) -> Result<Vec<u8>> {
        let zero = [0u8; 32];

        // TODO: Work out how to use an sequential nonce
        let nonce = rand::random::<u64>();

        let params = [
            Token::Tuple(vec![
                // SigData
                Token::Uint(zero.into()), // msgHash
                // TODO: Acutally get the nonce
                Token::Uint(nonce.into()), // nonce
                Token::Uint(zero.into()),  // sig
            ]),
            Token::Tuple(vec![
                // Key
                Token::Uint(todo!("Get this from the signing module")), // pubkeyX
                Token::Uint(todo!("Get this from the signing module")), // pubkeyYparity
                Token::Address(self.key_manager.deployed_address.into()), // nonceTimesGAddr
            ]),
        ];

        let tx_data = self
            .key_manager
            .set_agg_key_with_agg_key()
            .encode_input(&params[..])?;

        return Ok(tx_data);
    }
}

#[cfg(test)]
mod test_eth_tx_encoder {
    use super::*;
    use hex;

    use crate::mq::mq_mock::MQMock;

    #[ignore = "Not fully implemented"]
    #[test]
    fn test_tx_build() {
        let fake_address = hex::encode([12u8; 20]);
        let mq = MQMock::new();

        let encoder = SetAggKeyWithAggKeyEncoder::new(&fake_address[..], mq.get_client())
            .expect("Unable to intialise encoder");

        let event = AuctionConfirmedEvent::<StateChainRuntime> { auction_index: 1 };

        let _ = encoder
            .build_tx(&event)
            .expect("Unable to encode tx details");
    }

    #[test]
    fn test_tx_build_base() {
        let fake_address = hex::encode([12u8; 20]);
        let mq = MQMock::new();

        let encoder = SetAggKeyWithAggKeyEncoder::new(&fake_address[..], mq.get_client())
            .expect("Unable to intialise encoder");

        let event = FakeNewAggKey(
            23,
            "this is a new secret key, don't tell anyone about it".to_string(),
        );

        let _ = encoder
            .build_base_tx(&event)
            .expect("Unable to encode tx details");
    }
}
