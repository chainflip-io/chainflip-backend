use std::{collections::BTreeSet, sync::Arc};

use cf_traits::AuthorityCount;
use pallet_cf_vaults::CeremonyId;
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    multisig::{client::utils::PartyIdxMapping, crypto::Rng},
    multisig_p2p::OutgoingMultisigStageMessages,
};

use super::CeremonyFailureReason;

/// Outcome of a given ceremony stage
pub enum StageResult<M, Result> {
    /// Ceremony proceeds to the next stage
    NextStage(Box<dyn CeremonyStage<Message = M, Result = Result>>),
    /// Ceremony aborted (contains parties to report)
    Error(BTreeSet<AuthorityCount>, CeremonyFailureReason),
    /// Ceremony finished and successful
    Done(Result),
}

/// The result of processing a message for a stage from a single party
/// (currently used to indicate whether we are ready to proceed to the
/// next stage)
pub enum ProcessMessageResult {
    /// No further messages are expected for the current stage
    Ready,
    /// Should wait for more messages
    NotReady,
}

/// Defines actions that any given stage of a ceremony should be able to perform
pub trait CeremonyStage: std::fmt::Display {
    // Message type to be processed by a particular stage
    type Message;
    // Result to return if the ceremony is successful
    type Result;

    /// Perform initial computation for this stage (and initiate communication with other parties)
    fn init(&mut self);

    /// Process message from signer at index `signer_idx`. Precondition: the signer is a valid
    /// holder of the key and selected to participate in this ceremony (TODO: also check that
    /// we haven't processed a message from them?)
    fn process_message(
        &mut self,
        signer_idx: AuthorityCount,
        m: Self::Message,
    ) -> ProcessMessageResult;

    /// This is how individual stages signal messages that should be processed in the next stage
    fn should_delay(&self, m: &Self::Message) -> bool;

    /// Verify data for this stage after it is received from all other parties,
    /// either abort or proceed to the next stage based on the result
    fn finalize(self: Box<Self>) -> StageResult<Self::Message, Self::Result>;

    /// Parties we haven't heard from for the current stage
    fn awaited_parties(&self) -> BTreeSet<AuthorityCount>;
}

/// Data useful during any stage of a ceremony
#[derive(Clone)]
pub struct CeremonyCommon {
    pub ceremony_id: CeremonyId,
    /// Our own signer index
    pub own_idx: AuthorityCount,
    /// Indexes of parties participating in the ceremony
    pub all_idxs: BTreeSet<AuthorityCount>,
    pub outgoing_p2p_message_sender: UnboundedSender<OutgoingMultisigStageMessages>,
    pub validator_mapping: Arc<PartyIdxMapping>,
    pub rng: Rng,
    pub logger: slog::Logger,
}

impl CeremonyCommon {
    pub fn is_idx_valid(&self, idx: AuthorityCount) -> bool {
        self.all_idxs.contains(&idx)
    }
}
