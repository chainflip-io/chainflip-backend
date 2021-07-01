use std::{collections::HashMap, convert::TryInto};

use crate::{
    eth::key_manager::KeyManager,
    mq::{pin_message_stream, IMQClient, Subject},
    p2p::ValidatorId,
    settings,
    signing::{
        KeyId, KeygenOutcome, KeygenSuccess, MessageHash, MessageInfo, MultisigEvent,
        MultisigInstruction, SigningInfo,
    },
    types::chain::Chain,
};

use anyhow::Result;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use sp_core::Hasher;
use sp_runtime::traits::Keccak256;
use std::str::FromStr;
use web3::{ethabi::Token, types::Address};

use secp256k1::{PublicKey, Secp256k1, SecretKey, Signature};

/// Helper function, constructs and runs the [SetAggKeyWithAggKeyEncoder] asynchronously.
pub async fn start<M: IMQClient + Clone>(
    settings: &settings::Settings,
    mq_client: M,
) -> Result<()> {
    let mut encoder = SetAggKeyWithAggKeyEncoder::new(
        settings.eth.key_manager_eth_address.as_ref(),
        settings.signing.init_validators.clone(),
        mq_client,
    )?;

    let result = encoder.process_multi_sig_event_stream().await;

    Ok(())

    // let run_build_agg_key_fut = encoder.clone().run_build_and_emit_set_agg_key_txs();
    // let run_tx_constructor_fut = encoder.run_tx_constructor();

    // first fut is the only one that returns a Result, so just use that
    // futures::join!(run_build_agg_key_fut).0
}

/// Details of a transaction to be broadcast to ethereum.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct TxDetails {
    pub contract_address: Address,
    pub data: Vec<u8>,
}

// TODO: Use signing::MessageHash once it's updated to use [u8; 32]
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Hash, Eq)]
pub struct FakeMessageHash(pub [u8; 32]);

/// Reads [AuctionConfirmedEvent]s off the message queue and encodes the function call to the stake manager.
#[derive(Clone)]
struct SetAggKeyWithAggKeyEncoder<M: IMQClient> {
    mq_client: M,
    key_manager: KeyManager,
    // maps the MessageHash which gets sent to the signer with the data that the MessageHash is a hash of
    messages: HashMap<FakeMessageHash, ParamContainer>,
    // On genesis, where do these validators come from, to allow for the first key update
    validators: HashMap<KeyId, Vec<ValidatorId>>,
    curr_signing_key_id: Option<u64>,
    next_key_id: Option<u64>,
}

#[derive(Clone)]
struct ParamContainer {
    pub key_id: KeyId,
    pub nonce: [u8; 32],
    pub pubkey_x: [u8; 32],
    pub pubkey_y_parity: u8,
    pub nonce_times_g_addr: [u8; 20],
}

impl<M: IMQClient + Clone> SetAggKeyWithAggKeyEncoder<M> {
    fn new(
        key_manager_address: &str,
        init_validators: Vec<ValidatorId>,
        mq_client: M,
    ) -> Result<Self> {
        let key_manager = KeyManager::load(key_manager_address)?;

        let mut init_validators_hash_map = HashMap::new();
        init_validators_hash_map
            .entry(KeyId(0))
            .or_insert(init_validators);
        Ok(Self {
            mq_client,
            key_manager,
            messages: HashMap::new(),
            validators: init_validators_hash_map,
            curr_signing_key_id: Some(0),
            next_key_id: None,
        })
    }

    async fn process_multi_sig_event_stream(&mut self) {
        let multisig_event_stream = self
            .mq_client
            .subscribe::<MultisigEvent>(Subject::MultisigEvent)
            .await
            .unwrap();

        let mut multisig_event_stream = pin_message_stream(multisig_event_stream);

        while let Some(event) = multisig_event_stream.next().await {
            match event {
                Ok(event) => {
                    match event {
                        MultisigEvent::KeygenResult(key_outcome) => {
                            match key_outcome {
                                KeygenOutcome::Success(keygen_success) => {
                                    self.handle_keygen_success(keygen_success).await;
                                }
                                // TODO: Be more granular with log messages here
                                _ => {
                                    log::error!("Signing module returned error generating key")
                                }
                            }
                        }
                        MultisigEvent::MessageSigned(msg, sig) => {
                            self.handle_set_agg_key_message_signed(msg, sig).await;
                        }
                        _ => {
                            log::trace!("Discarding non keygen result or message signed event")
                        }
                    }
                }
                Err(e) => {
                    log::error!("Error reading event from multisig event stream");
                }
            }
        }
    }

    async fn handle_keygen_success(&mut self, keygen_success: KeygenSuccess) {
        // process the keygensuccess
        let (encoded_fn_params, param_container) = self
            .build_encoded_fn_params(&keygen_success)
            .expect("should be a valid encoded params");

        let hash = Keccak256::hash(&encoded_fn_params[..]);
        let message_hash = FakeMessageHash(hash.into());

        // store key: parameters, so we can fetch the parameters again, after the payload
        // has been signed by the signing module
        self.messages
            .entry(message_hash.clone())
            .or_insert(param_container);

        // TODO: Use the correct KeyId and vector of validators here.
        // how do we get the subset of signers from the OLD key, in order to use? Need to be collected from somewhere
        let signing_info = SigningInfo::new(KeyId(1), vec![]);
        // TODO: remove this cast when MessageHash wraps a [u8; 32]
        let message_hash = MessageHash(message_hash.0.to_vec());
        let signing_instruction = MultisigInstruction::Sign(message_hash, signing_info);

        self.mq_client
            .publish(Subject::MultisigInstruction, &signing_instruction)
            .await
            .expect("Should publish to MQ");
    }

    async fn handle_set_agg_key_message_signed(&self, msg: MessageInfo, sig: Signature) {
        // TODO: We can get the key id here from MessageInfo :)

        // 1. Get the data from the message hash that was signed (using the `messages` field)
        let msg: FakeMessageHash = FakeMessageHash(msg.hash.0.try_into().unwrap());
        let params = self
            .messages
            .get(&msg)
            .expect("should have been stored when asked to sign");
        // 2. Call build_tx with the required info
        match self.build_tx(&msg, &sig, params) {
            Ok(ref tx_details) => {
                // 3. Send it on its way to the eth broadcaster
                self.mq_client
                    .publish(Subject::Broadcast(Chain::ETH), tx_details)
                    .await
                    .unwrap_or_else(|err| {
                        log::error!("Could not process: {:#?}", err);
                    });
                // here we assume the key was update successfully
                // TODO update the state to reflect the update key
                // update curr key id
                // curr = next
                // next = None
                // update
            }
            Err(err) => {
                log::error!("Failed to build: {:#?}", err);
            }
        }
    }

    fn build_tx(
        &self,
        msg: &FakeMessageHash,
        sig: &Signature,
        params: &ParamContainer,
    ) -> Result<TxDetails> {
        let params = [
            Token::Tuple(vec![
                // SigData
                Token::Uint(msg.0.into()),        // msgHash
                Token::Uint(params.nonce.into()), // nonce
                Token::Uint(sig.into()),          // sig
            ]),
            Token::Tuple(vec![
                // Key
                Token::Uint(params.pubkey_x.into()), // pubkeyX
                Token::Uint(params.pubkey_y_parity.into()), // pubkeyYparity
                Token::Address(params.nonce_times_g_addr.into()), // nonceTimesGAddr
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

    fn generate_crypto_parts(
        &self,
        pubkey: secp256k1::PublicKey,
    ) -> ([u8; 32], u8, [u8; 32], [u8; 20]) {
        let s = secp256k1::Secp256k1::signing_only();

        // we don't need the secret, we have the public key

        // compressed form, means first byte is the y valence
        let pubkey_bytes: [u8; 33] = pubkey.serialize();
        let pubkey_y_parity_byte = pubkey_bytes[0];
        let pubkey_y_parity = if pubkey_y_parity_byte == 2 { 0u8 } else { 1u8 };

        let pubkey_x: [u8; 32] = pubkey_bytes[1..]
            .try_into()
            .expect("should be a valid pubkey");

        println!("pubkey y parity: {:?}", pubkey_y_parity);

        // does this have to be related to the "private key"? -> if it does, we can use the keys from the python
        // tests in tests/consts.py
        // nonce
        // I think we can generate this randomly??
        // I *think* this is the nonce. And we can generate a uint (256) randomly
        // after all, this is 64 / 2 (2 chars per byte) * 8 (bits per byte) = 256
        // TODO: generate this randomly (crypto-secure)
        let k_hex = "d51e13c68bf56155a83e50fd9bc840e2a1847fb9b49cd206a577ecd1cd15e285";
        let k = SecretKey::from_str(k_hex).unwrap();

        let s = Secp256k1::signing_only();
        let k_times_g = PublicKey::from_secret_key(&s, &k);

        // this is really just the nonce
        let k_times_g_pub: [u8; 64] = k_times_g.serialize_uncompressed()[1..]
            .try_into()
            .expect("Should be a valid pubkey");

        // not actually sure this is correct, but this makes it 256 bits and resembles k?
        let nonce: [u8; 32] = k_times_g.serialize()[1..]
            .try_into()
            .expect("should be valid pubkey");

        // calculate nonce times g addr
        let nonce_times_g_addr = Keccak256::hash(&k_times_g_pub).as_bytes().to_owned();
        // take the last 160bits (20 bytes)
        let from = nonce_times_g_addr.len() - 20;

        // is this just r ???? it seems that way
        // https://docs.decred.org/research/schnorr-signatures/
        // #[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
        // pub struct Signature {
        //     // This is `s` in other literature
        //     pub sigma: FE,
        //     // This is `r` in other literature
        //     pub v: GE,
        // }
        // how do we get the nonce theN???????
        let nonce_times_g_addr: [u8; 20] = nonce_times_g_addr[from..]
            .try_into()
            .expect("should only be 20 bytes long");

        return (pubkey_x, pubkey_y_parity, nonce, nonce_times_g_addr);
    }

    // Temporarily a CFE method, this will be moved to the state chain
    fn build_encoded_fn_params(
        &self,
        keygen_success: &KeygenSuccess,
    ) -> Result<(Vec<u8>, ParamContainer)> {
        let zero = [0u8; 32];

        let (pubkey_x, pubkey_y_parity, nonce, nonce_times_g_addr) =
            self.generate_crypto_parts(keygen_success.key);

        let param_container = ParamContainer {
            key_id: keygen_success.key_id,
            nonce,
            pubkey_x,
            pubkey_y_parity,
            nonce_times_g_addr,
        };

        let params = [
            Token::Tuple(vec![
                // SigData
                Token::Uint(zero.into()),  // msgHash
                Token::Uint(nonce.into()), // nonce
                Token::Uint(zero.into()),  // sig
            ]),
            Token::Tuple(vec![
                // Key
                Token::Uint(pubkey_x.into()),              // pubkeyX
                Token::Uint(pubkey_y_parity.into()),       // pubkeyYparity
                Token::Address(nonce_times_g_addr.into()), // nonceTimesGAddr
            ]),
        ];

        let tx_data = self
            .key_manager
            .set_agg_key_with_agg_key()
            .encode_input(&params[..])?;

        return Ok((tx_data, param_container));
    }
}

#[cfg(test)]
mod test_eth_tx_encoder {
    use super::*;
    use hex;

    use crate::mq::mq_mock::MQMock;

    // #[ignore = "Not fully implemented"]
    // #[test]
    // fn test_tx_build() {
    //     let fake_address = hex::encode([12u8; 20]);
    //     let settings = settings::test_utils::new_test_settings().unwrap();
    //     let mq = MQMock::new();

    //     let encoder = SetAggKeyWithAggKeyEncoder::new(
    //         &fake_address[..],
    //         settings.signing.init_validators,
    //         mq.get_client(),
    //     )
    //     .expect("Unable to intialise encoder");

    //     let event = FakeNewAggKeySigningComplete {
    //         hash: FakeMessageHash([0; 32]),
    //         sig: [0; 32],
    //     };

    //     let param_container = ParamContainer {
    //         key_id: KeyId(1),
    //         nonce: 3u64,
    //         pubkey_x: [0; 32],
    //         pubkey_y_parity: [0; 32],
    //         nonce_times_g_addr: [0; 20],
    //     };

    //     let _ = encoder
    //         .build_tx(&event, &param_container)
    //         .expect("Unable to encode tx details");
    // }

    // THIS CRYPTO COMES FROM crypto.py in Schnorr from the smart contracts repository
    #[test]
    fn secp256k1_sanity_check() {
        let s = secp256k1::Secp256k1::signing_only();

        let sk = secp256k1::SecretKey::from_str(
            "01010101010101010001020304050607ffff0000ffff00006363636363636363",
        )
        .unwrap();

        let pubkey_from_sk = PublicKey::from_secret_key(&s, &sk);

        // these keys should be derivable from each other.
        let pubkey = secp256k1::PublicKey::from_str(
            "0218845781f631c48f1c9709e23092067d06837f30aa0cd0544ac887fe91ddd166",
        )
        .unwrap();

        // for sanity
        assert_eq!(pubkey_from_sk, pubkey);
    }

    #[test]
    fn test_crypto_parts() {
        let fake_address = hex::encode([12u8; 20]);
        let settings = settings::test_utils::new_test_settings().unwrap();

        let mq = MQMock::new();
        let mq_c = mq.get_client();

        let encoder = SetAggKeyWithAggKeyEncoder::new(
            &fake_address[..],
            settings.signing.init_validators,
            mq_c,
        )
        .unwrap();

        let pubkey = secp256k1::PublicKey::from_str(
            "0218845781f631c48f1c9709e23092067d06837f30aa0cd0544ac887fe91ddd166",
        )
        .unwrap();

        encoder.generate_crypto_parts(pubkey);
    }

    // #[test]
    // fn test_build_encodings() {
    //     let fake_address = hex::encode([12u8; 20]);
    //     let settings = settings::test_utils::new_test_settings().unwrap();
    //     let mq = MQMock::new();

    //     let encoder = SetAggKeyWithAggKeyEncoder::new(
    //         &fake_address[..],
    //         settings.signing.init_validators,
    //         mq.get_client(),
    //     )
    //     .expect("Unable to intialise encoder");

    //     let event = FakeNewAggKey {
    //         pubkey_x: [0; 32],
    //         pubkey_y_parity: [0; 32],
    //         nonce_times_g_addr: [0; 20],
    //     };

    //     let _ = encoder
    //         .build_encoded_fn_params(&event)
    //         .expect("Unable to encode tx details");
    // }
}
