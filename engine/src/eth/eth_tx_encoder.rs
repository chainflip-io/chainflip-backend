use crate::{mq::{IMQClient, Subject, pin_message_stream}, sc_observer::{runtime::StateChainRuntime, staking::ClaimSignatureIssuedEvent}, types::chain::Chain};
use super::stake_manager::stake_manager::StakeManager;

use anyhow::Result;
use futures::StreamExt;
use serde::{Serialize, Deserialize};
use web3::{ethabi::Token, types::Address};

const TX_CONFIRMATIONS: usize = 6;

#[derive(Serialize, Deserialize)]
pub struct TxDetails {
    contract_address: Address,
    data: Vec<u8>,
}

struct RegisterClaimEncoder<M: IMQClient> {
    mq_client: M,
    stake_manager: StakeManager,
}

impl<M: IMQClient> RegisterClaimEncoder<M> {
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
                Ok(evt) => {
                    match self.build_and_send_tx(evt).await {
                        Ok(_) => {
                            log::debug!("{} built and send to broadcaster.", stringify!(ClaimSignatureIssuedEvent));
                        },
                        Err(err) => {
                            log::error!("Could not process {}: {}", stringify!(ClaimSignatureIssuedEvent), err);
                        },
                    }
                },
                Err(e) => {
                    log::error!("Unable to process claim request: {:?}.", e)
                },
            }
        }).await;
        
        Ok(())
    }

    async fn build_and_send_tx(&self, event: ClaimSignatureIssuedEvent<StateChainRuntime>) -> Result<()> {
        let params = [
            Token::Tuple(vec![ // SigData
                Token::Uint(todo!("msgHash needs to be emitted from state chain")), // msgHash
                Token::Uint(event.nonce.into()),// nonce
                Token::Uint(todo!("Signature is currently a 512-bit hash, should be 256 bits")) // sig
            ]),
            Token::FixedBytes(AsRef::<[u8; 32]>::as_ref(&event.who).to_vec()), // nodeId
            Token::Uint(event.amount.into()), // amount
            Token::Address(event.eth_address.into()), // staker
            Token::Uint(event.expiry.as_secs().into()) // expiryTime
        ];

        let tx_data = self.stake_manager.register_claim().encode_input(&params[..])?;

        let tx_details = TxDetails {
            contract_address: self.stake_manager.deployed_address,
            data: tx_data.into(),
        };

        self.mq_client.publish(Subject::Broadcast(Chain::ETH), &tx_details).await?;
    }
}
