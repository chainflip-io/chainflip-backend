use dyn_clone::DynClone;

use super::P2PSender;

/// Outcome of a given ceremony stage
pub enum StageResult<M, Result> {
    /// Ceremony proceeds to the next stage
    NextStage(Box<dyn CeremonyStage<Message = M, Result = Result>>),
    /// Ceremony aborted (contains parties to report)
    Error(Vec<usize>),
    /// Ceremony finished and successful
    Done(Result),
}

/// The result of processing a message for a stage from a single party
/// (currently used to indicate whether we are ready to proceed to the
/// next stage)
pub enum ProcessMessageResult {
    /// Some progress was made
    Progress,
    /// No further messages are expected
    CollectedAll,
    /// No progress was made
    Ignored,
}

/// Defines actions that any given stage of a ceremony should be able to perform
pub trait CeremonyStage: DynClone + std::fmt::Display {
    // Message type to be processed by a particular stage
    type Message;
    // Result to return if the ceremony is successful
    type Result;

    /// Perform initial computation for this stage (and initiate communication with other parties)
    fn init(&mut self);

    /// Process message from signer at index `signer_idx`. Precodintion: the signer is a valid
    /// holder of the key and selected to participate in this ceremony (TODO: also check that
    /// we haven't processed a message from them?)
    fn process_message(&mut self, signer_idx: usize, m: Self::Message) -> ProcessMessageResult;

    /// This is how individual stages signal messages that should be processed in the next stage
    fn should_delay(&self, m: &Self::Message) -> bool;

    /// Verify data for this stage after it is received from all other parties,
    /// either abort or proceed to the next stage based on the result
    fn finalize(self: Box<Self>) -> StageResult<Self::Message, Self::Result>;

    /// Parties we haven't heard from for the current stage
    fn awaited_parties(&self) -> Vec<usize>;
}

/// Data useful during any stage of a ceremony
#[derive(Clone)]
pub struct CeremonyCommon {
    /// Our own signer index
    pub own_idx: usize,
    /// Indexes of parties participating in the ceremony
    pub all_idxs: Vec<usize>,
    /// Sending end of the channel used for p2p communication
    pub p2p_sender: P2PSender,
    pub logger: slog::Logger,
}
