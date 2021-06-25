use crate::{
    eth::key_manager::KeyManager,
    mq::{pin_message_stream, IMQClient, Subject},
    sc_observer::{runtime::StateChainRuntime, validator::AuctionConfirmedEvent},
    settings,
    types::chain::Chain,
};

use anyhow::Result;
use futures::StreamExt;
use serde::{Serialize, Deserialize};
use web3::{ethabi::Token, types::Address};

/// Helper function, constructs and runs the [SetAggKeyWithAggKeyEncoder] asynchronously.
pub async fn start<M: IMQClient + Clone>(settings: &settings::Settings, mq_client: M) -> Result<()> {
    let encoder = SetAggKeyWithAggKeyEncoder::new(
        settings.eth.key_manager_eth_address.as_ref(), 
        mq_client)?;

    encoder.run().await
}

/// Details of a transaction to be broadcast to ethereum. 
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct TxDetails {
    pub contract_address: Address,
    pub data: Vec<u8>,
}

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
            .subscribe::<AuctionConfirmedEvent<StateChainRuntime>>(Subject::Rotate)
            .await?;
        
        let subscription = pin_message_stream(subscription);

        subscription.for_each_concurrent(None, |msg| async {
            match msg {
                Ok(ref evt) => {
                    match self.build_tx(evt) {
                        Ok(ref tx_details) => {
                            self.mq_client.publish(Subject::Broadcast(Chain::ETH), tx_details).await
                                .unwrap_or_else(|err| {
                                    log::error!("Could not process {}: {}", stringify!(AuctionConfirmedEvent), err);
                                });
                        },
                        Err(err) => {
                            log::error!("Failed to build {} for {:?}: {:?}", stringify!(TxDetails), evt, err);
                        },
                    }
                },
                Err(e) => {
                    log::error!("Unable to process claim request: {:?}.", e);
                },
            }
        }).await;

        log::info!("{} has stopped.", stringify!(SetAggKeyWithAggKeyEncoder));
        Ok(())
    }

    fn build_tx(&self, event: &AuctionConfirmedEvent<StateChainRuntime>) -> Result<TxDetails> {
        let params = [
            Token::Tuple(vec![ // SigData
                Token::Uint(todo!()), // msgHash
                Token::Uint(todo!()),// nonce
                Token::Uint(todo!()) // sig
            ]),
            Token::Tuple(vec![ // Key
                Token::Uint(todo!()), // msgHash
                Token::Uint(todo!()),// nonce
                Token::Address(todo!()) // sig
            ]),
        ];

        let tx_data = self.key_manager.set_agg_key_with_agg_key().encode_input(&params[..])?;

        Ok(TxDetails {
            contract_address: self.key_manager.deployed_address,
            data: tx_data.into(),
        })
    }
}

#[cfg(test)]
mod test_eth_tx_encoder {
    use super::*;
    use async_trait::async_trait;
    use hex;

    #[derive(Clone)]
    struct MockMqClient;

    #[async_trait]
    impl IMQClient for MockMqClient {
        async fn publish<M: 'static + Serialize + Sync>( &self, _subject: Subject, _message: &'_ M) -> Result<()> {
            unimplemented!()
        }

        async fn subscribe<M: frame_support::sp_runtime::DeserializeOwned>( &self, _subject: Subject, ) 
        -> Result<Box<dyn futures::Stream<Item = Result<M>>>> {
            unimplemented!()
        }

        async fn close(&self) -> Result<()> {
            unimplemented!()
        }
    }

    #[test]
    fn test_tx_build() {
        let fake_address = hex::encode([12u8; 20]);
        let encoder = SetAggKeyWithAggKeyEncoder::new(
            &fake_address[..],
            MockMqClient).expect("Unable to intialise encoder");
        
        let event = AuctionConfirmedEvent::<StateChainRuntime> {
            epoch_index: 1,
        };
        
        let _ = encoder.build_tx(&event).expect("Unable to encode tx details");
    }
}
