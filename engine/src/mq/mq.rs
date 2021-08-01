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
    ) -> Result<Box<dyn Stream<Item = Result<M>>>>;

    // / Close the connection to the MQ
    async fn close(&self) -> Result<()>;
}

/// Used to pin a stream within a single scope.
pub fn pin_message_stream<M>(stream: Box<dyn Stream<Item = M>>) -> Pin<Box<dyn Stream<Item = M>>> {
    stream.into()
}

/// Subjects that can be published / subscribed to
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Subject {
    Witness(Chain),
    Quote(Chain),
    Batch(Chain),
    Broadcast(Chain),

    // broadcaster pushes tx hashes here after being broadcast
    BroadcastSuccess(Chain),
    /// Stake events coming from the Stake manager contract
    StakeManager,
    /// events coming from the key manager contract
    KeyManager,

    // Auction pallet events
    AuctionStarted,
    AuctionConfirmed,
    AuctionCompleted,
    AuctionAborted,
    AuctionRangeChanged,
    AwaitingBidders,

    // Validator pallet events
    ForceRotationRequested,
    EpochDurationChanged,
    NewEpoch,

    // Staking pallet events
    ClaimSigRequested,
    Staked,
    ClaimSettled,
    StakeRefund,
    ClaimSignatureIssued,
    AccountRetired,
    AccountActivated,

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
            Subject::Witness(chain) => {
                format!("witness.{}", chain)
            }
            Subject::Quote(chain) => {
                format!("quote.{}", chain)
            }
            Subject::Batch(chain) => {
                format!("batch.{}", chain)
            }
            Subject::Broadcast(chain) => {
                format!("broadcast.{}", chain)
            }
            Subject::BroadcastSuccess(chain) => {
                format!("broadcast_success.{}", chain)
            }
            Subject::StakeManager => {
                format!("stake_manager")
            }
            Subject::KeyManager => {
                format!("key_manager")
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
            Subject::AuctionStarted => {
                format!("auction.auction_started")
            }
            Subject::AuctionConfirmed => {
                format!("auction.auction_confirmed")
            }
            Subject::AuctionCompleted => {
                format!("auction.auction_completed")
            }
            Subject::AuctionAborted => {
                format!("auction.auction_aborted")
            }
            Subject::AuctionRangeChanged => {
                format!("auction.auction_range_changed")
            }
            Subject::AwaitingBidders => {
                format!("auction.awaiting_bidders")
            }
            // === Validator events ===
            Subject::ForceRotationRequested => {
                format!("validator.force_rotation_requested")
            }
            Subject::EpochDurationChanged => {
                format!("validator.epoch_duration_changed")
            }
            Subject::NewEpoch => {
                format!("validator.new_epoch")
            }
            // === Staking events ===
            Subject::ClaimSigRequested => {
                format!("staking.claim_sig_requested")
            }
            Subject::Staked => {
                format!("staking.staked")
            }
            Subject::ClaimSettled => {
                format!("staking.claim_settled")
            }
            Subject::StakeRefund => {
                format!("staking.stake_refund")
            }
            Subject::ClaimSignatureIssued => {
                format!("staking.claim_signature_issued")
            }
            Subject::AccountRetired => {
                format!("staking.account_retired")
            }
            Subject::AccountActivated => {
                format!("staking.account_activated")
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn channel_to_subject_name() {
        let witness_subject = Subject::Witness(Chain::BTC);
        assert_eq!(witness_subject.to_subject_name(), "witness.BTC");

        let quote_subject = Subject::Quote(Chain::ETH);
        assert_eq!(quote_subject.to_subject_name(), "quote.ETH");

        let batch_subject = Subject::Batch(Chain::OXEN);
        assert_eq!(batch_subject.to_subject_name(), "batch.OXEN");

        let broadcast_subject = Subject::Broadcast(Chain::BTC);
        assert_eq!(broadcast_subject.to_subject_name(), "broadcast.BTC");

        let stake_manager_subject = Subject::StakeManager;
        assert_eq!(stake_manager_subject.to_subject_name(), "stake_manager");
    }
}
