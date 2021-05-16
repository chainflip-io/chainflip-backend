use std::{fmt, pin::Pin};

use anyhow::Result;
use async_trait::async_trait;
use chainflip_common::types::coin::Coin;
use futures::Stream;
use serde::{de::DeserializeOwned, Serialize};

/// Contains various general message queue options
#[derive(Debug, Clone)]
pub struct Options {
    pub url: String,
}

/// Interface for a message queue
#[async_trait]
pub trait IMQClient {
    /// Open a connection to the message queue
    async fn connect(opts: Options) -> Result<Box<Self>>;

    /// Publish something to a particular subject
    async fn publish<M: 'static + Serialize + Sync>(
        &self,
        subject: Subject,
        message: &'_ M,
    ) -> Result<()>;

    /// Subscribe to a subject
    async fn subscribe<M: DeserializeOwned>(
        &self,
        subject: Subject,
    ) -> Result<Box<dyn Stream<Item = Result<M>>>>;

    // / Close the connection to the MQ
    async fn close(&self) -> Result<()>;
}

/// Used to pin a stream within a single scope.
pub fn pin_message_stream<M>(stream: Box<dyn Stream<Item = M>>) -> Pin<Box<dyn Stream<Item = M>>> {
    stream.into()
}
/// Subjects that can be published / subscribed to
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Subject {
    Witness(Coin),
    Quote(Coin),
    Batch(Coin),
    Broadcast(Coin),
    /// Stake events coming from the Stake manager contract
    StakeManager,
    /// Stake events coming from the State chain
    StateChainStake,
    /// Claim events coming from the State chain
    StateChainClaim,
    /// Claim issued event from the state chain
    StateChainClaimIssued,
    Rotate,
    P2PIncoming,
    P2POutgoing,
}

// TODO: Make this a separate trait, not `fmt::Display` - https://github.com/chainflip-io/chainflip-backend/issues/63
// Used to create the subject that the MQ publishes to
impl fmt::Display for Subject {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self {
            Subject::Witness(coin) => {
                write!(f, "witness.{}", coin.to_string())
            }
            Subject::Quote(coin) => {
                write!(f, "quote.{}", coin.to_string())
            }
            Subject::Batch(coin) => {
                write!(f, "batch.{}", coin.to_string())
            }
            Subject::Broadcast(coin) => {
                write!(f, "broadcast.{}", coin.to_string())
            }
            Subject::StakeManager => {
                write!(f, "stake_manager")
            }
            Subject::StateChainClaim => {
                write!(f, "state_chain_claim")
            }
            Subject::Rotate => {
                write!(f, "rotate")
            }
            Subject::StateChainStake => {
                write!(f, "state_chain_stake")
            }
            Subject::StateChainClaimIssued => {
                write!(f, "state_chain_claim_issued")
            }
            Subject::P2PIncoming => {
                write!(f, "p2p_incoming")
            }
            Subject::P2POutgoing => {
                write!(f, "p2p_outgoing")
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn channel_to_string() {
        let witness_subject = Subject::Witness(Coin::BTC);
        assert_eq!(witness_subject.to_string(), "witness.BTC");

        let quote_subject = Subject::Quote(Coin::ETH);
        assert_eq!(quote_subject.to_string(), "quote.ETH");

        let batch_subject = Subject::Batch(Coin::OXEN);
        assert_eq!(batch_subject.to_string(), "batch.OXEN");

        let broadcast_subject = Subject::Broadcast(Coin::BTC);
        assert_eq!(broadcast_subject.to_string(), "broadcast.BTC");

        let stake_manager_subject = Subject::StakeManager;
        assert_eq!(stake_manager_subject.to_string(), "stake_manager");

        let sc_stake_subject = Subject::StateChainStake;
        assert_eq!(sc_stake_subject.to_string(), "state_chain_stake");

        let sc_claim_subject = Subject::StateChainClaim;
        assert_eq!(sc_claim_subject.to_string(), "state_chain_claim");
    }
}
