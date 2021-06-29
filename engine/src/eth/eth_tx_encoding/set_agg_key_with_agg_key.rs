use crate::{
    eth::key_manager::KeyManager,
    mq::{pin_message_stream, IMQClient, Subject},
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

/// Reads [AuctionConfirmedEvent]s off the message queue and encodes the function call to the stake manager.
#[derive(Clone)]
struct SetAggKeyWithAggKeyEncoder<M: IMQClient> {
    mq_client: M,
    key_manager: KeyManager,
}

impl<M: IMQClient + Clone> SetAggKeyWithAggKeyEncoder<M> {
    fn new(key_manager_address: &str, mq_client: M) -> Result<Self> {
        let key_manager = KeyManager::load(key_manager_address)?;

        Ok(Self {
            mq_client,
            key_manager,
        })
    }

    async fn run(self) -> Result<()> {
        let subscription = self
            .mq_client
            .subscribe::<AuctionConfirmedEvent<StateChainRuntime>>(Subject::SetAggKey)
            .await?;

        let subscription = pin_message_stream(subscription);

        subscription
            .for_each_concurrent(None, |msg| async {
                match msg {
                    Ok(ref evt) => match self.build_tx(evt) {
                        Ok(ref tx_details) => {
                            self.mq_client
                                .publish(Subject::Broadcast(Chain::ETH), tx_details)
                                .await
                                .unwrap_or_else(|err| {
                                    log::error!(
                                        "Could not process {}: {}",
                                        stringify!(AuctionConfirmedEvent),
                                        err
                                    );
                                });
                        }
                        Err(err) => {
                            log::error!(
                                "Failed to build {} for {:?}: {:?}",
                                stringify!(TxDetails),
                                evt,
                                err
                            );
                        }
                    },
                    Err(e) => {
                        log::error!("Unable to process claim request: {:?}.", e);
                    }
                }
            })
            .await;

        log::info!("{} has stopped.", stringify!(SetAggKeyWithAggKeyEncoder));
        Ok(())
    }

    fn build_tx(&self, event: &AuctionConfirmedEvent<StateChainRuntime>) -> Result<TxDetails> {
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
    async fn run_tx_constructor(&self) {
        // get events from the mq that need to be constructed
        let new_agg_key_created_stream = self
            .mq_client
            .subscribe::<FakeNewAggKey>(Subject::FakeNewAggKey)
            .await
            .unwrap();

        let new_agg_key_created_stream = pin_message_stream(new_agg_key_created_stream);

        new_agg_key_created_stream
            .for_each_concurrent(None, |event| async {
                let event = event.expect("Should be an event");
                let empty_tx = self.build_base_tx(&event).expect("should be a valid tx");

                let hash = Keccak256::hash(&empty_tx[..]);
                let message_hash = MessageHash(hash.as_bytes().to_vec());

                // TODO: Use the correct KeyId and vector of validators here.
                // Question, why do we even care who the validators are, should this be encapsulated by
                // the signing module?
                let signing_info = SigningInfo::new(KeyId(1), vec![]);
                let signing_instruction = MultisigInstruction::Sign(message_hash, signing_info);

                self.mq_client
                    .publish(Subject::MultisigInstruction, &signing_instruction)
                    .await
                    .expect("Should publish to MQ");
            })
            .await;
    }

    // Temporary a CFE method, this will be moved to the state chain
    fn build_base_tx(&self, event: &FakeNewAggKey) -> Result<Vec<u8>> {
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
