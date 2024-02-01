#[cfg(test)]
mod tests;

use std::{
	collections::{btree_map, BTreeMap, BTreeSet},
	pin::Pin,
	time::{Duration, Instant},
};

use anyhow::Result;
use cf_primitives::{AuthorityCount, CeremonyId};
use futures::future::{BoxFuture, FutureExt};
use tokio::sync::{
	mpsc::{UnboundedReceiver, UnboundedSender},
	oneshot,
};
use tracing::{debug, warn, Instrument};
use utilities::{format_iterator, metrics::CeremonyMetrics};

use crate::{
	client::{
		ceremony_id_string,
		common::{ProcessMessageResult, StageResult},
	},
	ChainSigning,
};
use state_chain_runtime::{constants::common::MAX_STAGE_DURATION_SECONDS, AccountId};

use super::{
	ceremony_manager::{CeremonyOutcome, CeremonyTrait, DynStage, PreparedRequest},
	common::PreProcessStageDataCheck,
};

const MAX_STAGE_DURATION: Duration = Duration::from_secs(MAX_STAGE_DURATION_SECONDS as u64);
const INCORRECT_NUMBER_ELEMENTS: &str = "incorrect_number_of_elements";

type OptionalCeremonyReturn<C> = Option<
	Result<
		<C as CeremonyTrait>::Output,
		(BTreeSet<AccountId>, <C as CeremonyTrait>::FailureReason),
	>,
>;

pub struct CeremonyRunner<Ceremony, Chain>
where
	Ceremony: CeremonyTrait,
	Chain: ChainSigning<CryptoScheme = Ceremony::Crypto>,
{
	// `None` means that the ceremony is not yet authorised (but may start delaying messages)
	stage: Option<DynStage<Ceremony>>,
	// Note that because we use a map here, the number of messages
	// that can be delayed from any one party is limited to one per stage.
	delayed_messages: BTreeMap<AccountId, Ceremony::Data>,
	/// This will fire on stage timeout
	timeout_handle: Pin<Box<tokio::time::Sleep>>,
	outcome_sender: UnboundedSender<(CeremonyId, CeremonyOutcome<Ceremony>)>,
	_phantom: std::marker::PhantomData<Chain>,
	metrics: CeremonyMetrics,
}

impl<Ceremony, Chain> CeremonyRunner<Ceremony, Chain>
where
	Ceremony: CeremonyTrait,
	Chain: ChainSigning<CryptoScheme = Ceremony::Crypto>,
{
	/// Listen for requests until the ceremony is finished
	/// Returns the id of the ceremony to make it easier to identify
	/// which ceremony is finished when many are running
	pub async fn run(
		ceremony_id: CeremonyId,
		mut message_receiver: UnboundedReceiver<(AccountId, Ceremony::Data)>,
		request_receiver: oneshot::Receiver<PreparedRequest<Ceremony>>,
		outcome_sender: UnboundedSender<(CeremonyId, CeremonyOutcome<Ceremony>)>,
	) -> Result<()> {
		let span = tracing::info_span!(
			"CeremonyRunner",
			ceremony_id = ceremony_id_string::<Chain>(ceremony_id)
		);

		// We always create unauthorised first, it can get promoted to
		// an authorised one with a ceremony request
		let mut runner = Self::new_unauthorised(outcome_sender);
		let mut ceremony_start: Option<Instant> = None;
		// Fuse the oneshot future so it will not get called twice
		let mut request_receiver = request_receiver.fuse();

		let outcome = loop {
			tokio::select! {
				Some((sender_id, message)) = message_receiver.recv() => {

					if let Some(result) = runner.process_or_delay_message(sender_id, message).instrument(span.clone()).await {
						break result;
					}

				}
				request = &mut request_receiver => {

					let PreparedRequest { initial_stage } = request.expect("Ceremony request channel was dropped unexpectedly");
					ceremony_start = Some(Instant::now());
					if let Some(result) = runner.on_ceremony_request(initial_stage).instrument(span.clone()).await {
						break result;
					}

				}
				// Only timeout if the ceremony is authorised
				() = runner.timeout_handle.as_mut(), if runner.stage.is_some() => {
					if let Some(result) = runner.on_timeout().instrument(span.clone()).await {
						break result;
					}
				}
			}
		};
		if let Some(start_instant) = ceremony_start {
			let duration = start_instant.elapsed();
			runner.metrics.ceremony_duration.observe(duration);
			span.in_scope(|| {
				tracing::info!("Ceremony took {}ms to complete", duration.as_millis())
			});
		}
		let _result = runner.outcome_sender.send((ceremony_id, outcome));
		Ok(())
	}

	/// Create ceremony state without a ceremony request (which is expected to arrive
	/// shortly). Until such request is received, we can start delaying messages, but
	/// cannot make any progress otherwise
	fn new_unauthorised(
		outcome_sender: UnboundedSender<(CeremonyId, CeremonyOutcome<Ceremony>)>,
	) -> Self {
		CeremonyRunner {
			stage: None,
			delayed_messages: Default::default(),
			// Unauthorised ceremonies cannot timeout, so just set the timeout to 0 for now.
			timeout_handle: Box::pin(tokio::time::sleep(tokio::time::Duration::ZERO)),
			outcome_sender,
			_phantom: Default::default(),
			metrics: CeremonyMetrics::new(Chain::NAME, Ceremony::CEREMONY_TYPE),
		}
	}

	/// Process ceremony request from the State Chain,
	/// initializing the ceremony into an authorised stage 1 state.
	pub async fn on_ceremony_request(
		&mut self,
		mut initial_stage: DynStage<Ceremony>,
	) -> OptionalCeremonyReturn<Ceremony> {
		let single_party_result = initial_stage.init(&mut self.metrics);

		// This function is only ever called from a oneshot channel,
		// so it should never get called twice.
		// Therefore we can assume the inner is not initialized yet.
		assert!(self.stage.replace(initial_stage).is_none());

		// Unlike other state transitions, we don't take into account
		// any time left in the prior stage when receiving a ceremony request because
		// we don't want other parties to be able to control when our stages time out.
		self.timeout_handle = Box::pin(tokio::time::sleep(MAX_STAGE_DURATION));

		if let ProcessMessageResult::Ready = single_party_result {
			self.finalize_current_stage().await
		} else {
			self.process_delayed().await
		}
	}

	fn finalize_current_stage(&mut self) -> BoxFuture<OptionalCeremonyReturn<Ceremony>> {
		async {
			// Ideally, we would pass the authorised state as a parameter
			// as it is always present (i.e. not `None`) when this function
			// is called, but the borrow checker won't let allow this.

			let stage = self
				.stage
				.take()
				.expect("Ceremony must be authorised to finalize any of its stages");
			let stage_name = stage.get_stage_name().to_string();
			let validator_mapping = stage.ceremony_common().validator_mapping.clone();

			match stage.finalize(&mut self.metrics).await {
				StageResult::NextStage(mut next_stage) => {
					debug!("Ceremony transitions to {}", next_stage.get_stage_name());
					self.metrics.stage_completing.inc(&[&stage_name]);

					let single_party_result = next_stage.init(&mut self.metrics);

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

					if let ProcessMessageResult::Ready = single_party_result {
						self.finalize_current_stage().await
					} else {
						self.process_delayed().await
					}
				},
				StageResult::Error(bad_validators, reason) => {
					self.metrics.stage_failing.inc(&[&stage_name, &format!("{:?}", reason)]);
					Some(Err((validator_mapping.get_ids(bad_validators), reason)))
				},
				StageResult::Done(result) => {
					debug!("Ceremony reached the final stage!");
					self.metrics.stage_completing.inc(&[&stage_name]);

					Some(Ok(result))
				},
			}
		}
		.boxed()
	}

	/// Process message from a peer, returning ceremony outcome if
	/// the ceremony stage machine cannot progress any further.
	/// Note: this is only public because of tests.
	pub async fn process_or_delay_message(
		&mut self,
		sender_id: AccountId,
		data: Ceremony::Data,
	) -> OptionalCeremonyReturn<Ceremony> {
		match &mut self.stage {
			None => {
				if !data.should_delay_unauthorised() {
					self.metrics.bad_message.inc(&["non_initial_stage"]);
					debug!(
						from_id = sender_id.to_string(),
						"Ignoring data for unauthorised ceremony: non-initial stage data"
					);
					return None
				}

				if !data.is_initial_stage_data_size_valid::<Chain>() {
					self.metrics.bad_message.inc(&[INCORRECT_NUMBER_ELEMENTS]);
					debug!(
						from_id = sender_id.to_string(),
						"Ignoring data for unauthorised ceremony: incorrect number of elements"
					);
					return None
				}

				self.add_delayed(sender_id, data);
			},
			Some(stage) => {
				// Check that the sender is a possible participant in the ceremony
				let sender_idx = match stage.ceremony_common().validator_mapping.get_idx(&sender_id)
				{
					Some(idx) => idx,
					None => {
						self.metrics.bad_message.inc(&["not_valid_participant"]);
						debug!("Ignoring data: sender {sender_id} is not a valid participant",);
						return None
					},
				};

				// Check that the number of elements in the data is what we expect
				if !data.is_data_size_valid::<Chain>(
					stage.ceremony_common().all_idxs.len() as AuthorityCount,
					stage.ceremony_common().number_of_signing_payloads,
				) {
					self.metrics.bad_message.inc(&[INCORRECT_NUMBER_ELEMENTS]);
					debug!(
						from_id = sender_id.to_string(),
						"Ignoring data: incorrect number of elements"
					);
					return None
				}

				// Check if we should delay this message for the next stage to use
				if Ceremony::Data::should_delay(stage.get_stage_name(), &data) {
					self.add_delayed(sender_id, data);
					return None
				}

				if let ProcessMessageResult::Ready =
					stage.process_message(sender_idx, data, &mut self.metrics)
				{
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

			if !messages.is_empty() {
				debug!(
					from_ids = format_iterator(messages.keys()).to_string(),
					"Processing {} delayed messages",
					messages.len(),
				);
			}
			for (id, m) in messages {
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
		let party_and_stage = match &self.stage {
			Some(stage) => format!("party [{id}] during stage {}", stage.get_stage_name()),
			None => format!("party [{id}] for an unauthorised ceremony"),
		};
		let total_delayed = self.delayed_messages.len() + 1;

		match self.delayed_messages.entry(id) {
			btree_map::Entry::Occupied(_) => {
				self.metrics.bad_message.inc(&["redundant_delayed_msg"]);
				warn!("Ignoring a redundant delayed message from {party_and_stage}");
			},
			btree_map::Entry::Vacant(entry) => {
				debug!("Delaying message {m} from {party_and_stage}. (Total: {total_delayed})");
				entry.insert(m);
			},
		}
	}

	async fn on_timeout(&mut self) -> OptionalCeremonyReturn<Ceremony> {
		if let Some(stage) = &self.stage {
			// We can't simply abort here as we don't know whether other
			// participants are going to do the same (e.g. if a malicious
			// node targeted us by communicating with everyone but us, it
			// would look to the rest of the network like we are the culprit).
			// Instead, we delegate the responsibility to the concrete stage
			// implementation to try to recover or agree on who to report.

			let missing_messages_from_accounts =
				stage.ceremony_common().validator_mapping.get_ids(stage.awaited_parties());

			warn!(
					missing_ids = format_iterator(missing_messages_from_accounts.clone()).to_string(),
					"Ceremony stage {} timed out before all messages collected ({} missing), trying to finalize current stage anyway.",
					stage.get_stage_name(),
					missing_messages_from_accounts.len()
				);
			let stage_name = stage.get_stage_name().to_string();
			self.metrics
				.missing_messages
				.set(&[&stage_name], missing_messages_from_accounts.len());
			self.finalize_current_stage().await
		} else {
			panic!("Unauthorised ceremonies cannot timeout");
		}
	}
}

#[cfg(test)]
impl<Ceremony, Chain> CeremonyRunner<Ceremony, Chain>
where
	Ceremony: CeremonyTrait,
	Chain: ChainSigning<CryptoScheme = Ceremony::Crypto>,
{
	/// This is to allow calling a private method from tests
	pub fn new_unauthorised_for_test() -> Self {
		Self::new_unauthorised(tokio::sync::mpsc::unbounded_channel().0)
	}

	fn get_awaited_parties_count(&self) -> Option<AuthorityCount> {
		self.stage.as_ref().map(|stage| stage.awaited_parties().len() as AuthorityCount)
	}

	pub async fn force_timeout(&mut self) -> OptionalCeremonyReturn<Ceremony> {
		self.on_timeout().await
	}
}
