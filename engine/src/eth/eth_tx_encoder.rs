use super::stake_manager::stake_manager::StakeManager;
use crate::{
    mq::{pin_message_stream, IMQClient, Subject},
    sc_observer::{runtime::StateChainRuntime, staking::ClaimSignatureIssuedEvent},
    settings,
    types::chain::Chain,
};

use anyhow::Result;
use futures::StreamExt;
use serde::{Serialize, Deserialize};
use web3::{ethabi::Token, types::Address};

/// Helper function, constructs and runs the [RegisterClaimEncoder] asynchronously.
pub async fn start<M: IMQClient + Clone>(settings: &settings::Settings, mq_client: M) -> Result<()> {
    let encoder = RegisterClaimEncoder::new(
        settings.eth.stake_manager_eth_address.as_ref(), 
        mq_client)?;

    encoder.run().await
}

/// Details of a transaction to be broadcast to ethereum. 
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct TxDetails {
    pub contract_address: Address,
    pub data: Vec<u8>,
}

/// Reads [ClaimSignatureIssuedEvent]s off the message queue and encodes the function call to the stake manager.
#[derive(Clone)]
struct RegisterClaimEncoder<M: IMQClient> {
    mq_client: M,
    stake_manager: StakeManager,
}

impl<M: IMQClient + Clone> RegisterClaimEncoder<M> {
    fn new(stake_manager_address: &str, mq_client: M) -> Result<Self> {
        let stake_manager = StakeManager::load(stake_manager_address)?;
        
        Ok(Self {
            mq_client,
            stake_manager,
        })
    }

    async fn run(self) -> Result<()> {
        let subscription = self
            .mq_client
            .subscribe::<ClaimSignatureIssuedEvent<StateChainRuntime>>(Subject::StateChainClaim)
            .await?;
        
        let subscription = pin_message_stream(subscription);

        subscription.for_each_concurrent(None, |msg| async {
            match msg {
                Ok(ref evt) => {
                    match self.build_tx(evt) {
                        Ok(ref tx_details) => {
                            self.mq_client.publish(Subject::Broadcast(Chain::ETH), tx_details).await
                                .unwrap_or_else(|err| {
                                    log::error!("Could not process {}: {}", stringify!(ClaimSignatureIssuedEvent), err);
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

        log::info!("{} has stopped.", stringify!(RegisterClaimEncoder));
        Ok(())
    }

    fn build_tx(&self, event: &ClaimSignatureIssuedEvent<StateChainRuntime>) -> Result<TxDetails> {
        let params = [
            Token::Tuple(vec![ // SigData
                Token::Uint(event.msg_hash), // msgHash
                Token::Uint(event.nonce.into()),// nonce
                Token::Uint(event.signature) // sig
            ]),
            Token::FixedBytes(AsRef::<[u8; 32]>::as_ref(&event.who).to_vec()), // nodeId
            Token::Uint(event.amount.into()), // amount
            Token::Address(event.eth_address.into()), // staker
            Token::Uint(event.expiry.as_secs().into()) // expiryTime
        ];

        let tx_data = self.stake_manager.register_claim().encode_input(&params[..])?;

        Ok(TxDetails {
            contract_address: self.stake_manager.deployed_address,
            data: tx_data.into(),
        })
    }
}

#[cfg(test)]
mod test_eth_tx_encoder {
    use super::*;
    use async_trait::async_trait;
    use frame_support::sp_runtime::AccountId32;
    use hex;
    use std::time::Duration;
    use web3::ethabi::ethereum_types::U256;

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
        let encoder = RegisterClaimEncoder::new(
            &fake_address[..],
            MockMqClient).expect("Unable to intialise encoder");
        
        let event = ClaimSignatureIssuedEvent::<StateChainRuntime> {
            msg_hash: U256::zero(),
            nonce: 1,
            signature: U256::zero(),
            who: AccountId32::new([2u8; 32]),
            amount: 1u128,
            eth_address: [1u8; 20],
            expiry: Duration::from_secs(0),
            _phantom: Default::default(),
        };
        
        let _ = encoder.build_tx(&event).expect("Unable to encode tx details");
    }
}
