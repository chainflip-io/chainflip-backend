use std::{
	collections::{btree_map, BTreeMap},
	fmt::Display,
	time::Instant,
};

use async_trait::async_trait;
use cf_primitives::{AuthorityCount, CeremonyId};
use tracing::warn;

use super::ceremony_stage::{CeremonyCommon, CeremonyStage, ProcessMessageResult, StageResult};
use crate::{
	client::{ceremony_manager::CeremonyTrait, MultisigMessage},
	p2p::{OutgoingMultisigStageMessages, ProtocolVersion, CURRENT_PROTOCOL_VERSION},
};
use utilities::metrics::CeremonyMetrics;

pub use super::broadcast_verification::verify_broadcasts_non_blocking;

/// Used by individual stages to distinguish between
/// a public message that should be broadcast to everyone
/// an secret messages that should be delivered to different
/// parties in private
pub enum DataToSend<T> {
	Broadcast(T),
	Private(BTreeMap<AuthorityCount, T>),
}

/// Abstracts away computations performed during every "broadcast" stage
/// of a ceremony
#[async_trait]
pub trait BroadcastStageProcessor<C: CeremonyTrait>: Display {
	/// The specific variant of D shared between parties
	/// during this stage
	type Message: Clone + Into<C::Data> + TryFrom<C::Data, Error = C::Data> + Send;

	/// Unique stage name used for logging and testing.
	const NAME: C::CeremonyStageName;

	/// Init the stage, returning the data to broadcast
	fn init(&mut self) -> DataToSend<Self::Message>;

	/// Determines how the data for this stage (of type `Self::Message`)
	/// should be processed once it either received it from all other parties
	/// or the stage timed out (None is used for missing messages)
	async fn process(
		self,
		messages: BTreeMap<AuthorityCount, Option<Self::Message>>,
	) -> StageResult<C>;
}

/// Responsible for broadcasting/collecting of stage data,
/// delegating the actual processing to `StageProcessor`
pub struct BroadcastStage<C: CeremonyTrait, Stage>
where
	Stage: BroadcastStageProcessor<C>,
{
	common: CeremonyCommon,
	/// Messages collected so far
	messages: BTreeMap<AuthorityCount, Stage::Message>,
	/// Determines the actual computations before/after
	/// the data is collected
	processor: Stage,
	stage_started: Option<Instant>,
}

impl<C: CeremonyTrait, Stage> BroadcastStage<C, Stage>
where
	Stage: BroadcastStageProcessor<C>,
{
	pub fn new(processor: Stage, common: CeremonyCommon) -> Self {
		BroadcastStage { common, messages: BTreeMap::new(), processor, stage_started: None }
	}
}

impl<C: CeremonyTrait, Stage> Display for BroadcastStage<C, Stage>
where
	Stage: BroadcastStageProcessor<C>,
	BroadcastStage<C, Stage>: CeremonyStage<C>,
{
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "BroadcastStage({})", &self.get_stage_name())
	}
}

fn serialize_for_version<C: CeremonyTrait>(
	ceremony_id: CeremonyId,
	data: C::Data,
	version: ProtocolVersion,
) -> Vec<u8> {
	let message = MultisigMessage { ceremony_id, data: data.into() };
	match version {
		1 => bincode::serialize(&message).unwrap(),
		_ => panic!("Unsupported protocol version"),
	}
}

#[async_trait]
impl<C: CeremonyTrait, Stage> CeremonyStage<C> for BroadcastStage<C, Stage>
where
	Stage: BroadcastStageProcessor<C> + Send,
{
	fn init(&mut self, metrics: &mut CeremonyMetrics) -> ProcessMessageResult {
		let common = &self.common;
		self.stage_started = Some(Instant::now());
		let idx_to_id = |idx: &AuthorityCount| common.validator_mapping.get_id(*idx).clone();

		let (own_message, outgoing_messages) = match self.processor.init() {
			DataToSend::Broadcast(stage_data) => {
				let ceremony_data: C::Data = stage_data.clone().into();
				(
					stage_data,
					OutgoingMultisigStageMessages::Broadcast(
						common
							.all_idxs
							.iter()
							.filter(|idx| **idx != common.own_idx)
							.map(idx_to_id)
							.collect(),
						serialize_for_version::<C>(
							common.ceremony_id,
							ceremony_data,
							CURRENT_PROTOCOL_VERSION,
						),
					),
				)
			},
			DataToSend::Private(mut messages) => (
				messages.remove(&common.own_idx).expect("Must include message to self"),
				OutgoingMultisigStageMessages::Private(
					messages
						.into_iter()
						.map(|(idx, stage_data)| {
							let ceremony_data: C::Data = stage_data.into();
							(
								idx_to_id(&idx),
								serialize_for_version::<C>(
									common.ceremony_id,
									ceremony_data,
									CURRENT_PROTOCOL_VERSION,
								),
							)
						})
						.collect(),
				),
			),
		};

		self.common
			.outgoing_p2p_message_sender
			.send(outgoing_messages)
			.expect("Could not send p2p message.");

		// Save our own share
		self.process_message(common.own_idx, own_message.into(), metrics)
	}

	fn process_message(
		&mut self,
		signer_idx: AuthorityCount,
		m: C::Data,
		metrics: &mut CeremonyMetrics,
	) -> ProcessMessageResult {
		metrics.processed_messages.inc();
		let m: Stage::Message = match m.try_into() {
			Ok(m) => m,
			Err(incorrect_type) => {
				metrics
					.bad_message
					.inc(&[&format!("incorrect_type ({})", self.get_stage_name())]);
				warn!(
					from_id = self.common.validator_mapping.get_id(signer_idx).to_string(),
					"Ignoring unexpected message {incorrect_type} while in stage {self}",
				);
				return ProcessMessageResult::NotReady
			},
		};

		if !self.common.all_idxs.contains(&signer_idx) {
			metrics
				.bad_message
				.inc(&[&format!("message_from_non_participant ({})", self.get_stage_name())]);
			warn!(
				from_id = self.common.validator_mapping.get_id(signer_idx).to_string(),
				"Ignoring a message from non-participant for stage {self}",
			);
			return ProcessMessageResult::NotReady
		}

		match self.messages.entry(signer_idx) {
			btree_map::Entry::Occupied(_) => {
				metrics
					.bad_message
					.inc(&[&format!("redundant_message ({})", self.get_stage_name())]);
				warn!(
					from_id = self.common.validator_mapping.get_id(signer_idx).to_string(),
					"Ignoring a redundant message for stage {self}",
				);
				ProcessMessageResult::NotReady
			},
			btree_map::Entry::Vacant(entry) => {
				entry.insert(m);

				if self.messages.len() == self.common.all_idxs.len() {
					ProcessMessageResult::Ready
				} else {
					ProcessMessageResult::NotReady
				}
			},
		}
	}

	async fn finalize(mut self: Box<Self>, metrics: &mut CeremonyMetrics) -> StageResult<C> {
		// Because we might want to finalize the stage before
		// all data has been received (e.g. due to a timeout),
		// we insert None for any missing data
		let stage_name = self.get_stage_name().to_string();
		if let Some(start_instant) = self.stage_started {
			metrics
				.stage_duration
				.observe(&[&stage_name, "receiving"], start_instant.elapsed());
		}

		let process_msg_instant = Instant::now();
		let mut received_messages = std::mem::take(&mut self.messages);

		// Turns values T into Option<T>, inserting `None` where
		// data hasn't been received for `idx`
		let messages: BTreeMap<_, _> = self
			.common
			.all_idxs
			.iter()
			.map(|idx| (*idx, received_messages.remove(idx)))
			.collect();

		let result = self.processor.process(messages).await;
		metrics
			.stage_duration
			.observe(&[&stage_name, "processing"], process_msg_instant.elapsed());
		result
	}

	fn awaited_parties(&self) -> std::collections::BTreeSet<AuthorityCount> {
		let mut awaited = std::collections::BTreeSet::new();

		for idx in &self.common.all_idxs {
			if !self.messages.contains_key(idx) {
				awaited.insert(*idx);
			}
		}

		awaited
	}

	fn get_stage_name(&self) -> C::CeremonyStageName {
		<Stage as BroadcastStageProcessor<C>>::NAME
	}

	fn ceremony_common(&self) -> &CeremonyCommon {
		&self.common
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		client::{
			ceremony_manager::KeygenCeremony, helpers::get_dummy_hash_comm, keygen::KeygenData,
		},
		eth::EvmCryptoScheme,
	};
	use rand::{rngs::StdRng, SeedableRng};

	#[test]
	/// If the structure or serialization of `MultisigMessage` changes, a new protocol version is
	/// needed.
	fn multisig_message_serialization_is_backwards_compatibility() {
		let rng = &mut StdRng::from_seed([0_u8; 32]);
		let data = KeygenData::HashComm1(get_dummy_hash_comm(rng));
		let serialized_data = serialize_for_version::<KeygenCeremony<EvmCryptoScheme>>(
			1,
			data,
			CURRENT_PROTOCOL_VERSION,
		);

		// Compare the serialized data with previously generated data using protocol version 1
		assert_eq!(hex::encode(serialized_data), "010000000000000000000000010000004200000000000000307839626634396136613037353566393533383131666365313235663236383364353034323963336262343965303734313437653030383961353265616531353566");
	}
}
