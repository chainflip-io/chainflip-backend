#[cfg(test)]
mod tests;

use std::{
	collections::{BTreeMap, BTreeSet},
	pin::Pin,
	time::Duration,
};

use anyhow::Result;
use cf_primitives::{AuthorityCount, CeremonyId};
use futures::future::{BoxFuture, FutureExt};
use tokio::sync::{
	mpsc::{UnboundedReceiver, UnboundedSender},
	oneshot,
};

use crate::{
	common::format_iterator,
	logging::CEREMONY_ID_KEY,
	multisig::client::common::{ProcessMessageResult, StageResult},
};
use state_chain_runtime::{constants::common::MAX_STAGE_DURATION_SECONDS, AccountId};

use super::{
	ceremony_manager::{CeremonyOutcome, CeremonyTrait, DynStage, PreparedRequest},
	common::{CeremonyFailureReason, PreProcessStageDataCheck},
};

const MAX_STAGE_DURATION: Duration = Duration::from_secs(MAX_STAGE_DURATION_SECONDS as u64);

type OptionalCeremonyReturn<Ceremony> = Option<
	Result<
		<Ceremony as CeremonyTrait>::Output,
		(
			BTreeSet<AccountId>,
			CeremonyFailureReason<
				<Ceremony as CeremonyTrait>::FailureReason,
				<Ceremony as CeremonyTrait>::CeremonyStageName,
			>,
		),
	>,
>;

pub struct CeremonyRunner<Ceremony: CeremonyTrait> {
	stage: Option<DynStage<Ceremony>>,
	// Note that because we use a map here, the number of messages
	// that can be delayed from any one party is limited to one per stage.
	delayed_messages: BTreeMap<AccountId, Ceremony::Data>,
	/// This will fire on stage timeout
	timeout_handle: Pin<Box<tokio::time::Sleep>>,
	outcome_sender: UnboundedSender<(CeremonyId, CeremonyOutcome<Ceremony>)>,
	logger: slog::Logger,
}

impl<Ceremony: CeremonyTrait> CeremonyRunner<Ceremony> {
	/// Listen for requests until the ceremony is finished
	/// Returns the id of the ceremony to make it easier to identify
	/// which ceremony is finished when many are running
	pub async fn run(
		ceremony_id: CeremonyId,
		mut message_receiver: UnboundedReceiver<(AccountId, Ceremony::Data)>,
		request_receiver: oneshot::Receiver<PreparedRequest<Ceremony>>,
		outcome_sender: UnboundedSender<(CeremonyId, CeremonyOutcome<Ceremony>)>,
		logger: slog::Logger,
	) -> Result<()> {
		// We always create unauthorised first, it can get promoted to
		// an authorised one with a ceremony request
		let mut runner = Self::new_unauthorised(ceremony_id, outcome_sender, &logger);

		// Fuse the oneshot future so it will not get called twice
		let mut request_receiver = request_receiver.fuse();

		let outcome = loop {
			tokio::select! {
				Some((sender_id, message)) = message_receiver.recv() => {

					if let Some(result) = runner.process_or_delay_message(sender_id, message).await {
						break result;
					}

				}
				request = &mut request_receiver => {

					let PreparedRequest { initial_stage } = request.expect("Ceremony request channel was dropped unexpectedly");

					if let Some(result) = runner.on_ceremony_request(initial_stage).await {
						break result;
					}

				}
				() = runner.timeout_handle.as_mut() => {

					// Only timeout if the ceremony is authorised
					if runner.stage.is_some() {
						if let Some(result) = runner.on_timeout().await {
							break result;
						}
					}

				}
			}
		};

		let _result = runner.outcome_sender.send((ceremony_id, outcome));
		Ok(())
	}

	/// Create ceremony state without a ceremony request (which is expected to arrive
	/// shortly). Until such request is received, we can start delaying messages, but
	/// cannot make any progress otherwise
	fn new_unauthorised(
		ceremony_id: CeremonyId,
		outcome_sender: UnboundedSender<(CeremonyId, CeremonyOutcome<Ceremony>)>,
		logger: &slog::Logger,
	) -> Self {
		CeremonyRunner {
			stage: None,
			delayed_messages: Default::default(),
			// Unauthorised ceremonies cannot timeout, so just set the timeout to 0 for now.
			timeout_handle: Box::pin(tokio::time::sleep(tokio::time::Duration::ZERO)),
			outcome_sender,
			logger: logger.new(slog::o!(CEREMONY_ID_KEY => ceremony_id)),
		}
	}

	/// Process ceremony request from the State Chain,
	/// initializing the ceremony into an authorised stage 1 state.
	pub async fn on_ceremony_request(
		&mut self,
		mut initial_stage: DynStage<Ceremony>,
	) -> OptionalCeremonyReturn<Ceremony> {
		initial_stage.init();

		// This function is only ever called from a oneshot channel,
		// so it should never get called twice.
		// Therefore we can assume the inner is not initialized yet.
		assert!(self.stage.replace(initial_stage).is_none());

		// Unlike other state transitions, we don't take into account
		// any time left in the prior stage when receiving a ceremony request because
		// we don't want other parties to be able to control when our stages time out.
		self.timeout_handle = Box::pin(tokio::time::sleep(MAX_STAGE_DURATION));

		self.process_delayed().await
	}

	async fn finalize_current_stage(&mut self) -> OptionalCeremonyReturn<Ceremony> {
		// Ideally, we would pass the authorised state as a parameter
		// as it is always present (i.e. not `None`) when this function
		// is called, but the borrow checker won't let allow this.

		let stage = self
			.stage
			.take()
			.expect("Ceremony must be authorised to finalize any of its stages");

		let validator_mapping = stage.ceremony_common().validator_mapping.clone();

		match stage.finalize().await {
			StageResult::NextStage(mut next_stage) => {
				slog::debug!(
					self.logger,
					"Ceremony transitions to {}",
					next_stage.get_stage_name()
				);

				next_stage.init();

				self.stage = Some(next_stage);

				// Instead of resetting the expiration time, we simply extend
				// it (any remaining time carries over to the next stage).
				// Doing it otherwise would allow other parties to influence
				// the time at which the stages in individual nodes time out
				// (by sending their data at specific times) thus making some
				// attacks possible.
				{
					let current_deadline = self.timeout_handle.as_ref().deadline();
					self.timeout_handle.as_mut().reset(current_deadline + MAX_STAGE_DURATION);
				}

				self.process_delayed().await
			},
			StageResult::Error(bad_validators, reason) =>
				Some(Err((validator_mapping.get_ids(bad_validators), reason))),
			StageResult::Done(result) => {
				slog::debug!(self.logger, "Ceremony reached the final stage!");

				Some(Ok(result))
			},
		}
	}

	/// Process message from a peer, returning ceremony outcome if
	/// the ceremony stage machine cannot progress any further
	pub async fn process_or_delay_message(
		&mut self,
		sender_id: AccountId,
		data: Ceremony::Data,
	) -> OptionalCeremonyReturn<Ceremony> {
		match &mut self.stage {
			None => {
				if !data.is_first_stage() {
					slog::debug!(
						self.logger,
						"Ignoring data: non-initial stage data for unauthorised ceremony";
						"from_id" => sender_id.to_string(),
					);
					return None
				}

				// We do not need to check data_size_is_valid here because stage 1 messages are
				// always the correct size.

				self.add_delayed(sender_id, data);
			},
			Some(stage) => {
				// Check that the sender is a possible participant in the ceremony
				let sender_idx = match stage.ceremony_common().validator_mapping.get_idx(&sender_id)
				{
					Some(idx) => idx,
					None => {
						slog::debug!(
							self.logger,
							"Ignoring data: sender {} is not a valid participant",
							sender_id
						);
						return None
					},
				};

				// Check that the number of elements in the data is what we expect
				if !data
					.data_size_is_valid(stage.ceremony_common().all_idxs.len() as AuthorityCount)
				{
					slog::debug!(
						self.logger,
						"Ignoring data: incorrect number of elements";
						"from_id" => sender_id.to_string(),
					);
					return None
				}

				// Check if we should delay this message for the next stage to use
				if Ceremony::Data::should_delay(stage.get_stage_name(), &data) {
					self.add_delayed(sender_id, data);
					return None
				}

				if let ProcessMessageResult::Ready = stage.process_message(sender_idx, data) {
					return self.finalize_current_stage().await
				}
			},
		}

		None
	}

	/// Process previously delayed messages (which arrived one stage too early)
	// NOTE: Need this boxed to help with async recursion
	fn process_delayed(&mut self) -> BoxFuture<OptionalCeremonyReturn<Ceremony>> {
		async {
			let messages = std::mem::take(&mut self.delayed_messages);

			for (id, m) in messages {
				slog::debug!(self.logger, "Processing delayed message {} from party [{}]", m, id,);

				if let Some(result) = self.process_or_delay_message(id, m).await {
					return Some(result)
				}
			}

			None
		}
		.boxed()
	}

	/// Delay message to be processed in the next stage
	fn add_delayed(&mut self, id: AccountId, m: Ceremony::Data) {
		match &self.stage {
			Some(stage) => {
				slog::debug!(
					self.logger,
					"Delaying message {} from party [{}] during stage: {}",
					m,
					id,
					stage.get_stage_name()
				);
			},
			None => {
				slog::debug!(
					self.logger,
					"Delaying message {} from party [{}] for unauthorised ceremony",
					m,
					id
				)
			},
		}

		self.delayed_messages.insert(id, m);

		slog::debug!(self.logger, "Total delayed: {}", self.delayed_messages.len());
	}

	async fn on_timeout(&mut self) -> OptionalCeremonyReturn<Ceremony> {
		if let Some(stage) = &self.stage {
			// We can't simply abort here as we don't know whether other
			// participants are going to do the same (e.g. if a malicious
			// node targeted us by communicating with everyone but us, it
			// would look to the rest of the network like we are the culprit).
			// Instead, we delegate the responsibility to the concrete stage
			// implementation to try to recover or agree on who to report.

			slog::warn!(
				self.logger,
				"Ceremony stage timed out before all messages collected; trying to finalize current stage anyway"
			);

			// Log the account ids of the missing messages
			let missing_messages_from_accounts =
				stage.ceremony_common().validator_mapping.get_ids(stage.awaited_parties());
			slog::debug!(
				self.logger,
				"Stage `{}` is missing messages from {} parties",
				stage.get_stage_name(),
				missing_messages_from_accounts.len();
				"missing_ids" => format_iterator(missing_messages_from_accounts).to_string()
			);

			self.finalize_current_stage().await
		} else {
			panic!("Unauthorised ceremonies cannot timeout");
		}
	}
}

#[cfg(test)]
impl<Ceremony: CeremonyTrait> CeremonyRunner<Ceremony> {
	/// This is to allow calling a private method from tests
	pub fn new_unauthorised_for_test(ceremony_id: CeremonyId, logger: &slog::Logger) -> Self {
		Self::new_unauthorised(ceremony_id, tokio::sync::mpsc::unbounded_channel().0, logger)
	}

	pub fn new_authorised(
		ceremony_id: CeremonyId,
		stage: DynStage<Ceremony>,
		logger: slog::Logger,
	) -> Self {
		CeremonyRunner {
			stage: Some(stage),
			delayed_messages: Default::default(),
			timeout_handle: Box::pin(tokio::time::sleep(MAX_STAGE_DURATION)),
			outcome_sender: tokio::sync::mpsc::unbounded_channel().0,
			logger: logger.new(slog::o!(CEREMONY_ID_KEY => ceremony_id)),
		}
	}

	fn get_awaited_parties_count(&self) -> Option<AuthorityCount> {
		self.stage.as_ref().map(|stage| stage.awaited_parties().len() as AuthorityCount)
	}

	pub async fn force_timeout(&mut self) -> OptionalCeremonyReturn<Ceremony> {
		self.on_timeout().await
	}
}
