use std::pin::Pin;

use crate::types::chain::Chain;
use anyhow::Result;
use async_trait::async_trait;
use futures::Stream;
use serde::{de::DeserializeOwned, Serialize};

/// Interface for a message queue
#[async_trait]
pub trait IMQClient {
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
    ) -> Result<Pin<Box<dyn Stream<Item = Result<M>>>>>;

    // / Close the connection to the MQ
    async fn close(&self) -> Result<()>;
}

/// Subjects that can be published / subscribed to
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Subject {
    Broadcast(Chain),

    // broadcaster pushes tx hashes here after being broadcast
    BroadcastSuccess(Chain),

    // Auction pallet events
    AuctionCompleted,

    P2PIncoming,
    P2POutgoing,
    MultisigInstruction,
    // both signing and keygen events come from here
    MultisigEvent,
    /// Published by the signing module to notify SC about
    /// the outcome of a keygen ceremony
    KeygenResult,
}

/// Convert an object to a to a subject string (currently Nats compatible)
pub trait SubjectName {
    fn to_subject_name(&self) -> String;
}

impl SubjectName for Subject {
    fn to_subject_name(&self) -> String {
        match &self {
            Subject::Broadcast(chain) => {
                format!("broadcast.{}", chain)
            }
            Subject::BroadcastSuccess(chain) => {
                format!("broadcast_success.{}", chain)
            }
            // === Signing ===
            Subject::P2PIncoming => {
                format!("p2p_incoming")
            }
            Subject::P2POutgoing => {
                format!("p2p_outgoing")
            }
            Subject::MultisigInstruction => {
                format!("multisig_instruction")
            }
            Subject::MultisigEvent => {
                format!("multisig_event")
            }
            Subject::KeygenResult => {
                format!("keygen_result")
            }
            // === Auction events ===
            Subject::AuctionCompleted => {
                format!("auction.auction_completed")
            }
        }
    }
}
