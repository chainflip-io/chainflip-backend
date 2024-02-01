#[cfg(test)]
mod tests;

use anyhow::{anyhow, bail, Context, Result};
use futures::FutureExt;
use serde::Serialize;
use std::{
	collections::{BTreeSet, HashMap},
	fmt::{Debug, Display},
	marker::PhantomData,
	sync::Arc,
};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tracing::{debug, info_span, trace, warn, Instrument};

use crate::{
	client,
	client::{
		ceremony_id_string,
		common::{KeygenFailureReason, SigningFailureReason},
		signing::PayloadAndKey,
		CeremonyRequestDetails,
	},
	crypto::{CryptoScheme, Rng},
	p2p::{OutgoingMultisigStageMessages, VersionedCeremonyMessage},
	ChainSigning,
};
use cf_primitives::{AuthorityCount, CeremonyId};
use state_chain_runtime::AccountId;
use utilities::{
	metrics::{AUTHORIZED_CEREMONIES, CEREMONY_BAD_MSG, UNAUTHORIZED_CEREMONIES},
	task_scope::{task_scope, Scope, ScopedJoinHandle},
};

use client::{ceremony_runner::CeremonyRunner, utils::PartyIdxMapping};

use tokio::sync::oneshot;

use client::common::{
	broadcast::BroadcastStage, CeremonyCommon, CeremonyFailureReason, KeygenResultInfo,
};

use super::{
	common::{
		CeremonyStage, KeygenStageName, PreProcessStageDataCheck, ResharingContext,
		SigningStageName,
	},
	keygen::{HashCommitments1, HashContext, KeygenData, PubkeySharesStage0},
	signing::SigningData,
	CeremonyRequest, MultisigData, MultisigMessage,
};

pub type CeremonyOutcome<C> = Result<
	<C as CeremonyTrait>::Output,
	(BTreeSet<AccountId>, <C as CeremonyTrait>::FailureReason),
>;

pub type CeremonyResultSender<Ceremony> = oneshot::Sender<CeremonyOutcome<Ceremony>>;
pub type CeremonyResultReceiver<Ceremony> = oneshot::Receiver<CeremonyOutcome<Ceremony>>;

const KEYGEN_LABEL: &str = "keygen";
const SIGNING_LABEL: &str = "signing";

/// Ceremony trait combines type parameters that are often used together
pub trait CeremonyTrait: 'static {
	const CEREMONY_TYPE: &'static str;
	// Determines which curve and signing method to use
	type Crypto: CryptoScheme;
	// The type of data that will be used in p2p for this ceremony type
	type Data: Debug
		+ Display
		+ PreProcessStageDataCheck<Self::CeremonyStageName>
		+ TryFrom<
			MultisigData<<Self::Crypto as CryptoScheme>::Point>,
			Error = MultisigData<<Self::Crypto as CryptoScheme>::Point>,
		> + Into<MultisigData<<Self::Crypto as CryptoScheme>::Point>>
		+ Send
		+ Ord
		+ Serialize
		+ 'static;
	type Request: Send + 'static;
	/// The product of a successful ceremony result
	type Output: Debug + Send + 'static;
	type FailureReason: CeremonyFailureReason + Send + Ord + Debug;
	type CeremonyStageName: Debug + Display + Ord + Send;
}

pub struct KeygenCeremony<C> {
	_phantom: PhantomData<C>,
}

impl<C: CryptoScheme> CeremonyTrait for KeygenCeremony<C> {
	const CEREMONY_TYPE: &'static str = KEYGEN_LABEL;
	type Crypto = C;
	type Data = KeygenData<<C as CryptoScheme>::Point>;
	type Request = CeremonyRequest<C>;
	type Output = KeygenResultInfo<C>;
	type FailureReason = KeygenFailureReason;
	type CeremonyStageName = KeygenStageName;
}

pub struct SigningCeremony<C> {
	_phantom: PhantomData<C>,
}

impl<C: CryptoScheme> CeremonyTrait for SigningCeremony<C> {
	const CEREMONY_TYPE: &'static str = SIGNING_LABEL;
	type Crypto = C;
	type Data = SigningData<<C as CryptoScheme>::Point>;
	type Request = CeremonyRequest<C>;
	type Output = Vec<<C as CryptoScheme>::Signature>;
	type FailureReason = SigningFailureReason;
	type CeremonyStageName = SigningStageName;
}

/// Responsible for mapping ceremonies to the corresponding states and
/// generating signer indexes based on the list of parties
pub struct CeremonyManager<Chain: ChainSigning> {
	my_account_id: AccountId,
	outgoing_p2p_message_sender: UnboundedSender<OutgoingMultisigStageMessages>,
	signing_states: CeremonyStates<SigningCeremony<Chain::CryptoScheme>>,
	keygen_states: CeremonyStates<KeygenCeremony<Chain::CryptoScheme>>,
	latest_ceremony_id: CeremonyId,
}

// A CeremonyStage for either keygen or signing
pub type DynStage<C> = Box<dyn CeremonyStage<C> + Send + Sync>;

// A ceremony request that has passed initial checks and setup its initial stage
pub struct PreparedRequest<C: CeremonyTrait> {
	pub initial_stage: DynStage<C>,
}

// Checks if all keys have the same parameters (including validator indices mapping), which
// should be the case if they have been generated for the same set of validators
fn are_key_parameters_same<'a, Crypto: CryptoScheme>(
	keys: impl IntoIterator<Item = &'a KeygenResultInfo<Crypto>>,
) -> bool {
	let mut keys_iter = keys.into_iter();
	let first = keys_iter.next().expect("must have at least one key");

	keys_iter
		.all(|key| key.params == first.params && key.validator_mapping == first.validator_mapping)
}

// Initial checks and setup before sending the request to the `CeremonyRunner`
pub fn prepare_signing_request<Crypto: CryptoScheme>(
	ceremony_id: CeremonyId,
	own_account_id: &AccountId,
	signers: BTreeSet<AccountId>,
	signing_info: Vec<(KeygenResultInfo<Crypto>, Crypto::SigningPayload)>,
	outgoing_p2p_message_sender: &UnboundedSender<OutgoingMultisigStageMessages>,
	rng: Rng,
) -> Result<PreparedRequest<SigningCeremony<Crypto>>, SigningFailureReason> {
	// Sanity check: all keys must have the same parameters
	if !are_key_parameters_same(signing_info.iter().map(|(info, _)| info)) {
		return Err(SigningFailureReason::DeveloperError(
			"keys have different parameters".to_string(),
		))
	}

	let validator_mapping = signing_info[0].0.validator_mapping.clone();

	// Check that we have enough signers
	let minimum_signers_needed = signing_info[0].0.params.threshold + 1;
	let signers_len: AuthorityCount = signers.len().try_into().expect("too many signers");
	if signers_len < minimum_signers_needed {
		debug!(
			"Request to sign invalid: not enough signers ({}/{minimum_signers_needed})",
			signers.len(),
		);

		return Err(SigningFailureReason::NotEnoughSigners)
	}

	// Generate signer indexes
	let (own_idx, signer_idxs) =
		match map_ceremony_parties(own_account_id, &signers, &validator_mapping) {
			Ok(result) => result,
			Err(reason) => {
				debug!("Request to sign invalid: {reason}");
				return Err(SigningFailureReason::InvalidParticipants)
			},
		};

	// Prepare initial ceremony stage
	let initial_stage = {
		use super::signing::{AwaitCommitments1, SigningStateCommonInfo};

		let common = CeremonyCommon {
			ceremony_id,
			outgoing_p2p_message_sender: outgoing_p2p_message_sender.clone(),
			validator_mapping,
			own_idx,
			all_idxs: signer_idxs,
			rng,
			number_of_signing_payloads: Some(signing_info.len()),
		};

		let processor = AwaitCommitments1::<Crypto>::new(
			common.clone(),
			SigningStateCommonInfo {
				payloads_and_keys: signing_info
					.into_iter()
					.map(|(key_info, payload)| PayloadAndKey { payload, key: key_info.key })
					.collect(),
			},
		);

		Box::new(BroadcastStage::new(processor, common))
	};

	Ok(PreparedRequest { initial_stage })
}

pub fn prepare_key_handover_request<Crypto: CryptoScheme>(
	ceremony_id: CeremonyId,
	own_account_id: &AccountId,
	participants: BTreeSet<AccountId>,
	outgoing_p2p_message_sender: &UnboundedSender<OutgoingMultisigStageMessages>,
	resharing_context: ResharingContext<Crypto>,
	rng: Rng,
) -> Result<PreparedRequest<KeygenCeremony<Crypto>>, KeygenFailureReason> {
	let validator_mapping = Arc::new(PartyIdxMapping::from_participants(participants.clone()));

	let (our_idx, signer_idxs) =
		match map_ceremony_parties(own_account_id, &participants, &validator_mapping) {
			Ok(res) => res,
			Err(reason) => {
				debug!("Key Handover request invalid: {reason}");

				return Err(KeygenFailureReason::InvalidParticipants)
			},
		};

	let initial_stage = {
		let common = CeremonyCommon {
			ceremony_id,
			outgoing_p2p_message_sender: outgoing_p2p_message_sender.clone(),
			validator_mapping,
			own_idx: our_idx,
			all_idxs: signer_idxs,
			rng,
			number_of_signing_payloads: None,
		};

		let processor = PubkeySharesStage0::new(
			common.clone(),
			generate_keygen_context(ceremony_id, participants),
			resharing_context,
		);

		Box::new(BroadcastStage::new(processor, common))
	};

	Ok(PreparedRequest { initial_stage })
}

// Initial checks and setup before sending the request to the `CeremonyRunner`
pub fn prepare_keygen_request<Crypto: CryptoScheme>(
	ceremony_id: CeremonyId,
	own_account_id: &AccountId,
	participants: BTreeSet<AccountId>,
	outgoing_p2p_message_sender: &UnboundedSender<OutgoingMultisigStageMessages>,
	rng: Rng,
) -> Result<PreparedRequest<KeygenCeremony<Crypto>>, KeygenFailureReason> {
	let validator_mapping = Arc::new(PartyIdxMapping::from_participants(participants.clone()));

	let (our_idx, signer_idxs) =
		match map_ceremony_parties(own_account_id, &participants, &validator_mapping) {
			Ok(res) => res,
			Err(reason) => {
				debug!("Keygen request invalid: {reason}");

				return Err(KeygenFailureReason::InvalidParticipants)
			},
		};

	let initial_stage = {
		let common = CeremonyCommon {
			ceremony_id,
			outgoing_p2p_message_sender: outgoing_p2p_message_sender.clone(),
			validator_mapping,
			own_idx: our_idx,
			all_idxs: signer_idxs,
			rng,
			number_of_signing_payloads: None,
		};

		let keygen_common = client::keygen::KeygenCommon::new(
			common.clone(),
			generate_keygen_context(ceremony_id, participants),
			None,
		);

		let processor = HashCommitments1::new(keygen_common);

		Box::new(BroadcastStage::new(processor, common))
	};

	Ok(PreparedRequest { initial_stage })
}

fn map_ceremony_parties(
	own_account_id: &AccountId,
	participants: &BTreeSet<AccountId>,
	validator_mapping: &PartyIdxMapping,
) -> Result<(AuthorityCount, BTreeSet<AuthorityCount>), &'static str> {
	assert!(participants.contains(own_account_id), "we are not among participants");

	// It should be impossible to fail here because of the check above,
	// but I don't like unwrapping (would be better if we
	// could combine this with the check above)
	let our_idx = validator_mapping.get_idx(own_account_id).ok_or("could not derive our idx")?;

	// Check that signer ids are known for this key
	let signer_idxs = validator_mapping
		.get_all_idxs(participants)
		.map_err(|()| "Failed to map ceremony parties: invalid participants")?;

	Ok((our_idx, signer_idxs))
}

pub fn deserialize_for_version<C: CryptoScheme>(
	message: VersionedCeremonyMessage,
) -> Result<MultisigMessage<C::Point>> {
	match message.version {
		1 => bincode::deserialize::<'_, MultisigMessage<C::Point>>(&message.payload).map_err(|e| {
			anyhow!("Failed to deserialize message (version: {}): {:?}", message.version, e)
		}),
		_ => Err(anyhow!("Unsupported message version: {}", message.version)),
	}
}

impl<Chain: ChainSigning> CeremonyManager<Chain> {
	pub fn new(
		my_account_id: AccountId,
		outgoing_p2p_message_sender: UnboundedSender<OutgoingMultisigStageMessages>,
		latest_ceremony_id: CeremonyId,
	) -> Self {
		CeremonyManager {
			my_account_id,
			outgoing_p2p_message_sender,
			signing_states: CeremonyStates::new(),
			keygen_states: CeremonyStates::new(),
			latest_ceremony_id,
		}
	}

	async fn on_request(
		&mut self,
		request: CeremonyRequest<Chain::CryptoScheme>,
		scope: &Scope<'_, anyhow::Error>,
	) {
		// Always update the latest ceremony id, even if we are not participating
		self.update_latest_ceremony_id(request.ceremony_id);

		match request.details {
			Some(CeremonyRequestDetails::Keygen(details)) => {
				if let Some(resharing_context) = details.resharing_context {
					self.on_key_handover_request(
						request.ceremony_id,
						details.participants,
						details.rng,
						details.result_sender,
						resharing_context,
						scope,
					)
				} else {
					self.on_keygen_request(
						request.ceremony_id,
						details.participants,
						details.rng,
						details.result_sender,
						scope,
					)
				}
				UNAUTHORIZED_CEREMONIES.set(
					&[Chain::NAME, KEYGEN_LABEL],
					self.keygen_states.count_unauthorised_ceremonies(),
				);
				AUTHORIZED_CEREMONIES.set(
					&[Chain::NAME, KEYGEN_LABEL],
					self.keygen_states.count_authorised_ceremonies(),
				);
			},
			Some(CeremonyRequestDetails::Sign(details)) => {
				self.on_request_to_sign(
					request.ceremony_id,
					details.participants,
					details.signing_info,
					details.rng,
					details.result_sender,
					scope,
				);
				UNAUTHORIZED_CEREMONIES.set(
					&[Chain::NAME, SIGNING_LABEL],
					self.signing_states.count_unauthorised_ceremonies(),
				);
				AUTHORIZED_CEREMONIES.set(
					&[Chain::NAME, SIGNING_LABEL],
					self.signing_states.count_authorised_ceremonies(),
				);
			},
			None => {
				// Because unauthorised ceremonies don't timeout, We must check the id of ceremonies
				// that we are not participating in and cleanup any unauthorised ceremonies that may
				// have been created by a bad p2p message.
				if self.signing_states.cleanup_unauthorised_ceremony(&request.ceremony_id) {
					SigningFailureReason::NotParticipatingInUnauthorisedCeremony
						.log(&BTreeSet::default());
					UNAUTHORIZED_CEREMONIES.set(
						&[Chain::NAME, SIGNING_LABEL],
						self.signing_states.count_unauthorised_ceremonies(),
					);
				}
				if self.keygen_states.cleanup_unauthorised_ceremony(&request.ceremony_id) {
					KeygenFailureReason::NotParticipatingInUnauthorisedCeremony
						.log(&BTreeSet::default());
					UNAUTHORIZED_CEREMONIES.set(
						&[Chain::NAME, KEYGEN_LABEL],
						self.keygen_states.count_unauthorised_ceremonies(),
					);
				}
			},
		}
	}

	pub async fn run(
		mut self,
		mut ceremony_request_receiver: UnboundedReceiver<CeremonyRequest<Chain::CryptoScheme>>,
		mut incoming_p2p_message_receiver: UnboundedReceiver<(AccountId, VersionedCeremonyMessage)>,
	) -> Result<()> {
		task_scope(|scope| {
			async {
				loop {
					tokio::select! {
						Some(request) = ceremony_request_receiver.recv() => {
							self.on_request(request, scope).await;
						}
						Some((sender_id, data)) = incoming_p2p_message_receiver.recv() => {

							// At this point we know the messages to be for the
							// appropriate curve (as defined by `C`)
							match deserialize_for_version::<Chain::CryptoScheme>(data) {
								Ok(message) => self.process_p2p_message(sender_id, message, scope),
								Err(_) => {
									CEREMONY_BAD_MSG.inc(&[Chain::NAME, "deserialize_for_version"]);
									warn!("Failed to deserialize message from: {sender_id}");
								},
							}
						}
						Some((id, outcome)) = self.signing_states.outcome_receiver.recv() => {
							self.signing_states.finalize_authorised_ceremony(id, outcome);
							AUTHORIZED_CEREMONIES.set(&[Chain::NAME, SIGNING_LABEL], self.signing_states.count_authorised_ceremonies());
						}
						Some((id, outcome)) = self.keygen_states.outcome_receiver.recv() => {
							self.keygen_states.finalize_authorised_ceremony(id, outcome);
							AUTHORIZED_CEREMONIES.set(&[Chain::NAME, KEYGEN_LABEL], self.keygen_states.count_authorised_ceremonies());
						}
					}
				}
			}
			.instrument(info_span!("MultisigClient", chain = Chain::NAME))
			.boxed()
		})
		.await
	}

	fn on_key_handover_request(
		&mut self,
		ceremony_id: CeremonyId,
		participants: BTreeSet<AccountId>,
		rng: Rng,
		result_sender: CeremonyResultSender<KeygenCeremony<Chain::CryptoScheme>>,
		resharing_context: ResharingContext<Chain::CryptoScheme>,
		scope: &Scope<'_, anyhow::Error>,
	) {
		assert!(!participants.is_empty(), "Key handover request has no participants");

		let span = info_span!(
			"Key Handover Ceremony",
			ceremony_id = ceremony_id_string::<Chain>(ceremony_id)
		);
		let _entered = span.enter();

		debug!("Processing a key handover request");

		let request =
			match prepare_key_handover_request(
				ceremony_id,
				&self.my_account_id,
				participants,
				&self.outgoing_p2p_message_sender,
				resharing_context,
				rng,
			) {
				Ok(request) => request,
				Err(failed_outcome) => {
					let _res = result_sender.send(CeremonyOutcome::<
						KeygenCeremony<Chain::CryptoScheme>,
					>::Err((BTreeSet::new(), failed_outcome)));

					// Remove a possible unauthorised ceremony
					self.keygen_states.cleanup_unauthorised_ceremony(&ceremony_id);
					return
				},
			};

		let ceremony_handle =
			self.keygen_states.get_state_or_create_unauthorized::<Chain>(ceremony_id, scope);

		ceremony_handle
			.on_request(request, result_sender)
			.with_context(|| {
				format!(
					"Invalid key handover request with ceremony id {}",
					ceremony_id_string::<Chain>(ceremony_id)
				)
			})
			.unwrap();
	}

	/// Process a keygen request
	fn on_keygen_request(
		&mut self,
		ceremony_id: CeremonyId,
		participants: BTreeSet<AccountId>,
		rng: Rng,
		result_sender: CeremonyResultSender<KeygenCeremony<Chain::CryptoScheme>>,
		scope: &Scope<'_, anyhow::Error>,
	) {
		assert!(!participants.is_empty(), "Keygen request has no participants");

		let span =
			info_span!("Keygen Ceremony", ceremony_id = ceremony_id_string::<Chain>(ceremony_id));
		let _entered = span.enter();

		debug!("Processing a keygen request");

		let request =
			match prepare_keygen_request(
				ceremony_id,
				&self.my_account_id,
				participants,
				&self.outgoing_p2p_message_sender,
				rng,
			) {
				Ok(request) => request,
				Err(failed_outcome) => {
					let _res = result_sender.send(CeremonyOutcome::<
						KeygenCeremony<Chain::CryptoScheme>,
					>::Err((BTreeSet::new(), failed_outcome)));

					// Remove a possible unauthorised ceremony
					self.keygen_states.cleanup_unauthorised_ceremony(&ceremony_id);
					return
				},
			};

		let ceremony_handle =
			self.keygen_states.get_state_or_create_unauthorized::<Chain>(ceremony_id, scope);

		ceremony_handle
			.on_request(request, result_sender)
			.with_context(|| {
				format!(
					"Invalid keygen request with ceremony id {}",
					ceremony_id_string::<Chain>(ceremony_id)
				)
			})
			.unwrap();
	}

	/// Process a request to sign
	fn on_request_to_sign(
		&mut self,
		ceremony_id: CeremonyId,
		signers: BTreeSet<AccountId>,
		signing_info: Vec<(
			KeygenResultInfo<Chain::CryptoScheme>,
			<Chain::CryptoScheme as CryptoScheme>::SigningPayload,
		)>,
		rng: Rng,
		result_sender: CeremonyResultSender<SigningCeremony<Chain::CryptoScheme>>,
		scope: &Scope<'_, anyhow::Error>,
	) {
		assert!(!signers.is_empty(), "Request to sign has no signers");

		let span =
			info_span!("Signing Ceremony", ceremony_id = ceremony_id_string::<Chain>(ceremony_id));
		let _entered = span.enter();

		debug!("Processing a request to sign");

		let request = match prepare_signing_request(
			ceremony_id,
			&self.my_account_id,
			signers,
			signing_info,
			&self.outgoing_p2p_message_sender,
			rng,
		) {
			Ok(request) => request,
			Err(failed_outcome) => {
				let _res = result_sender.send(CeremonyOutcome::<
					SigningCeremony<Chain::CryptoScheme>,
				>::Err((BTreeSet::new(), failed_outcome)));

				// Remove a possible unauthorised ceremony
				self.signing_states.cleanup_unauthorised_ceremony(&ceremony_id);
				return
			},
		};

		// We have the key and have received a request to sign
		let ceremony_handle = self
			.signing_states
			.get_state_or_create_unauthorized::<Chain>(ceremony_id, scope);

		ceremony_handle
			.on_request(request, result_sender)
			.with_context(|| {
				format!(
					"Invalid sign request with ceremony id {}",
					ceremony_id_string::<Chain>(ceremony_id)
				)
			})
			.unwrap();
	}

	/// Process message from another validator
	fn process_p2p_message(
		&mut self,
		sender_id: AccountId,
		message: MultisigMessage<<Chain::CryptoScheme as CryptoScheme>::Point>,
		scope: &Scope<'_, anyhow::Error>,
	) {
		match message {
			MultisigMessage { ceremony_id, data: MultisigData::Keygen(data) } => {
				let span = info_span!(
					"Keygen Ceremony",
					ceremony_id = ceremony_id_string::<Chain>(ceremony_id)
				);
				let _entered = span.enter();

				self.keygen_states.process_data::<Chain>(
					sender_id,
					ceremony_id,
					data,
					self.latest_ceremony_id,
					scope,
				)
			},
			MultisigMessage { ceremony_id, data: MultisigData::Signing(data) } => {
				let span = info_span!(
					"Signing Ceremony",
					ceremony_id = ceremony_id_string::<Chain>(ceremony_id)
				);
				let _entered = span.enter();

				self.signing_states.process_data::<Chain>(
					sender_id,
					ceremony_id,
					data,
					self.latest_ceremony_id,
					scope,
				)
			},
		}
	}

	/// Override the latest ceremony id. Used to limit the spamming of unauthorised ceremonies.
	pub fn update_latest_ceremony_id(&mut self, ceremony_id: CeremonyId) {
		assert_eq!(self.latest_ceremony_id + 1, ceremony_id);
		self.latest_ceremony_id = ceremony_id;
	}
}

/// Create unique deterministic context used for generating a ZKP to prevent replay attacks
fn generate_keygen_context(ceremony_id: CeremonyId, signers: BTreeSet<AccountId>) -> HashContext {
	use blake2::{Blake2b, Digest};

	let mut hasher = Blake2b::<typenum::U32>::new();

	hasher.update(ceremony_id.to_be_bytes());

	// NOTE: it should be sufficient to use ceremony_id as context as
	// we never reuse the same id for different ceremonies, but lets
	// put the signers in to make the context hard to predict as well
	for id in signers {
		hasher.update(id);
	}

	HashContext(*hasher.finalize().as_ref())
}

struct CeremonyStates<Ceremony: CeremonyTrait> {
	// Collection of all ceremony handles used to send data to the ceremony tasks
	ceremony_handles: HashMap<CeremonyId, CeremonyHandle<Ceremony>>,
	// Given to each ceremony for it to send back the outcome
	outcome_sender: UnboundedSender<(CeremonyId, CeremonyOutcome<Ceremony>)>,
	/// All authorised ceremonies will send their outcome here
	outcome_receiver: UnboundedReceiver<(CeremonyId, CeremonyOutcome<Ceremony>)>,
}

impl<Ceremony: CeremonyTrait> CeremonyStates<Ceremony> {
	fn new() -> Self {
		let (outcome_sender, outcome_receiver) = mpsc::unbounded_channel();
		Self { ceremony_handles: HashMap::new(), outcome_sender, outcome_receiver }
	}

	/// Process ceremony data arriving from a peer,
	fn process_data<Chain: ChainSigning>(
		&mut self,
		sender_id: AccountId,
		ceremony_id: CeremonyId,
		data: Ceremony::Data,
		latest_ceremony_id: CeremonyId,
		scope: &Scope<'_, anyhow::Error>,
	) where
		Chain: ChainSigning<CryptoScheme = Ceremony::Crypto>,
	{
		debug!("Received data {data} from [{sender_id}]");

		// If no ceremony exists, create an unauthorised one (with ceremony id tracking
		if let std::collections::hash_map::Entry::Vacant(e) =
			self.ceremony_handles.entry(ceremony_id)
		{
			// Only a ceremony id that is within the ceremony id window can create unauthorised
			// ceremonies
			let ceremony_id_string = ceremony_id_string::<Chain>(ceremony_id);
			if ceremony_id > latest_ceremony_id + Chain::CEREMONY_ID_WINDOW {
				CEREMONY_BAD_MSG.inc(&[Chain::NAME, "unexpected_future_ceremony_id"]);
				warn!("Ignoring data: unexpected future ceremony id {ceremony_id_string}",);
				return
			} else if ceremony_id <= latest_ceremony_id {
				CEREMONY_BAD_MSG.inc(&[Chain::NAME, "old_ceremony_id"]);
				trace!("Ignoring data: old ceremony id {ceremony_id_string}",);
				return
			} else {
				e.insert(CeremonyHandle::spawn::<Chain>(
					ceremony_id,
					self.outcome_sender.clone(),
					scope,
				));
				let total = self.count_unauthorised_ceremonies();
				UNAUTHORIZED_CEREMONIES.set(&[Chain::NAME, Ceremony::CEREMONY_TYPE], total);
				trace!("Unauthorised ceremony created {ceremony_id_string} (Total: {total})",);
			}
		}

		let ceremony_handle =
			self.ceremony_handles.get(&ceremony_id).expect("Entry is inserted above");

		// NOTE: There is a short delay between dropping the ceremony runner (and any channels
		// associated with it) and dropping the corresponding ceremony handle, which makes it
		// possible for the following `send` to fail
		if ceremony_handle.message_sender.send((sender_id, data)).is_err() {
			debug!("Ignoring data: ceremony runner has been dropped");
		}
	}

	/// Returns the state for the given ceremony id if it exists,
	/// otherwise creates a new unauthorized one
	fn get_state_or_create_unauthorized<Chain: ChainSigning>(
		&mut self,
		ceremony_id: CeremonyId,
		scope: &Scope<'_, anyhow::Error>,
	) -> &mut CeremonyHandle<Ceremony>
	where
		Chain: ChainSigning<CryptoScheme = Ceremony::Crypto>,
	{
		self.ceremony_handles.entry(ceremony_id).or_insert_with(|| {
			CeremonyHandle::spawn::<Chain>(ceremony_id, self.outcome_sender.clone(), scope)
		})
	}

	/// Send the outcome of the ceremony and remove its state
	fn finalize_authorised_ceremony(
		&mut self,
		ceremony_id: CeremonyId,
		ceremony_outcome: CeremonyOutcome<Ceremony>,
	) {
		if let CeremonyRequestState::Authorised(result_sender) = self
			.ceremony_handles
			.remove(&ceremony_id)
			.expect("Should have handle")
			.request_state
		{
			let _result = result_sender.send(ceremony_outcome);
		} else {
			panic!("Expected authorised ceremony");
		}
	}

	/// Removing any state associated with the unauthorized ceremony and therefore abort its task
	fn cleanup_unauthorised_ceremony(&mut self, ceremony_id: &CeremonyId) -> bool {
		// Dropping the ceremony handle will cause any associated task to be aborted
		if let Some(ceremony_handle) = self.ceremony_handles.remove(ceremony_id) {
			assert!(
				matches!(ceremony_handle.request_state, CeremonyRequestState::Unauthorised(_)),
				"Expected an unauthorised ceremony"
			);
			true
		} else {
			false
		}
	}

	fn count_unauthorised_ceremonies(&self) -> usize {
		self.ceremony_handles
			.values()
			.filter(|handle| matches!(handle.request_state, CeremonyRequestState::Unauthorised(_)))
			.count()
	}

	fn count_authorised_ceremonies(&self) -> usize {
		self.ceremony_handles
			.values()
			.filter(|handle| matches!(handle.request_state, CeremonyRequestState::Authorised(_)))
			.count()
	}
}

// ==================

/// Contains the result sender and the channels used to send data to a running ceremony
struct CeremonyHandle<Ceremony: CeremonyTrait> {
	pub message_sender: UnboundedSender<(AccountId, Ceremony::Data)>,
	pub request_state: CeremonyRequestState<Ceremony>,
	// When the task handle is dropped, the task will be aborted.
	pub _task_handle: ScopedJoinHandle<()>,
}

/// Contains either the request sender or the result sender depending on the state of the ceremony
enum CeremonyRequestState<Ceremony: CeremonyTrait> {
	/// Initial state before we have received the request from the SC.
	/// Contains the oneshot channel used to relay the request to the ceremony task.
	Unauthorised(oneshot::Sender<PreparedRequest<Ceremony>>),
	/// State after receiving the request from the SC.
	/// Contains the result sender that is used to send the ceremony outcome.
	Authorised(CeremonyResultSender<Ceremony>),
}

impl<Ceremony: CeremonyTrait> CeremonyHandle<Ceremony> {
	fn spawn<Chain: ChainSigning>(
		ceremony_id: CeremonyId,
		outcome_sender: UnboundedSender<(CeremonyId, CeremonyOutcome<Ceremony>)>,
		scope: &Scope<'_, anyhow::Error>,
	) -> Self
	where
		Chain: ChainSigning<CryptoScheme = Ceremony::Crypto>,
	{
		let (message_sender, message_receiver) = mpsc::unbounded_channel();
		let (request_sender, request_receiver) = oneshot::channel();

		let task_handle = scope.spawn_with_handle(CeremonyRunner::<Ceremony, Chain>::run(
			ceremony_id,
			message_receiver,
			request_receiver,
			outcome_sender,
		));

		CeremonyHandle {
			message_sender,
			request_state: CeremonyRequestState::Unauthorised(request_sender),
			_task_handle: task_handle,
		}
	}

	fn on_request(
		&mut self,
		request: PreparedRequest<Ceremony>,
		result_sender: CeremonyResultSender<Ceremony>,
	) -> Result<()> {
		// Transition to an authorized state by consuming the
		// request sender and storing the result sender
		if let CeremonyRequestState::Unauthorised(request_sender) = std::mem::replace(
			&mut self.request_state,
			CeremonyRequestState::Authorised(result_sender),
		) {
			let _res = request_sender.send(request);
		} else {
			// Already in an authorised state, a request has already been sent to a ceremony with
			// this id
			bail!("Duplicate ceremony id");
		}

		Ok(())
	}
}
