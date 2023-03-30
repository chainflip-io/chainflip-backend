#[cfg(test)]
mod tests;

use anyhow::{anyhow, bail, Context, Result};
use futures::FutureExt;
use std::{
	collections::{hash_map, BTreeSet, HashMap},
	fmt::{Debug, Display},
	marker::PhantomData,
	sync::Arc,
};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tracing::{debug, info, info_span, trace, warn, Instrument};

use crate::{
	constants::CEREMONY_ID_WINDOW,
	multisig::{
		client,
		client::{
			common::{KeygenFailureReason, SigningFailureReason},
			keygen::generate_key_data,
			CeremonyRequestDetails,
		},
		crypto::{generate_single_party_signature, CryptoScheme, Rng},
	},
	p2p::{OutgoingMultisigStageMessages, VersionedCeremonyMessage},
	task_scope::{task_scope, Scope, ScopedJoinHandle},
};
use cf_primitives::{AuthorityCount, CeremonyId};
use state_chain_runtime::AccountId;

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
	keygen::{HashCommitments1, HashContext, KeygenData},
	legacy::MultisigMessageV1,
	signing::SigningData,
	CeremonyRequest, MultisigData, MultisigMessage,
};

pub type CeremonyOutcome<C> = Result<
	<C as CeremonyTrait>::Output,
	(BTreeSet<AccountId>, <C as CeremonyTrait>::FailureReason),
>;

pub type CeremonyResultSender<Ceremony> = oneshot::Sender<CeremonyOutcome<Ceremony>>;
pub type CeremonyResultReceiver<Ceremony> = oneshot::Receiver<CeremonyOutcome<Ceremony>>;

/// Ceremony trait combines type parameters that are often used together
pub trait CeremonyTrait: 'static {
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
	type Crypto = C;
	type Data = SigningData<<C as CryptoScheme>::Point>;
	type Request = CeremonyRequest<C>;
	type Output = Vec<<C as CryptoScheme>::Signature>;
	type FailureReason = SigningFailureReason;
	type CeremonyStageName = SigningStageName;
}

/// Responsible for mapping ceremonies to the corresponding states and
/// generating signer indexes based on the list of parties
pub struct CeremonyManager<C: CryptoScheme> {
	my_account_id: AccountId,
	outgoing_p2p_message_sender: UnboundedSender<OutgoingMultisigStageMessages>,
	signing_states: CeremonyStates<SigningCeremony<C>>,
	keygen_states: CeremonyStates<KeygenCeremony<C>>,
	latest_ceremony_id: CeremonyId,
}

// A CeremonyStage for either keygen or signing
pub type DynStage<C> = Box<dyn CeremonyStage<C> + Send + Sync>;

// A ceremony request that has passed initial checks and setup its initial stage
pub struct PreparedRequest<C: CeremonyTrait> {
	pub initial_stage: DynStage<C>,
}

// Initial checks and setup before sending the request to the `CeremonyRunner`
pub fn prepare_signing_request<Crypto: CryptoScheme>(
	ceremony_id: CeremonyId,
	own_account_id: &AccountId,
	signers: BTreeSet<AccountId>,
	key_info: KeygenResultInfo<Crypto>,
	payloads: Vec<Crypto::SigningPayload>,
	outgoing_p2p_message_sender: &UnboundedSender<OutgoingMultisigStageMessages>,
	rng: Rng,
) -> Result<PreparedRequest<SigningCeremony<Crypto>>, SigningFailureReason> {
	// Check that we have enough signers
	let minimum_signers_needed = key_info.params.threshold + 1;
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
		match map_ceremony_parties(own_account_id, &signers, &key_info.validator_mapping) {
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
			validator_mapping: key_info.validator_mapping,
			own_idx,
			all_idxs: signer_idxs,
			rng,
		};

		let processor = AwaitCommitments1::<Crypto>::new(
			common.clone(),
			SigningStateCommonInfo { payloads, key: key_info.key },
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
	resharing_context: Option<ResharingContext<Crypto>>,
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
		};

		let processor = HashCommitments1::new(
			common.clone(),
			generate_keygen_context(ceremony_id, participants),
			resharing_context,
		);

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
		.map_err(|_| "invalid participants")?;

	Ok((our_idx, signer_idxs))
}

pub fn deserialize_from_v1<C: CryptoScheme>(payload: &[u8]) -> Result<MultisigMessage<C::Point>> {
	let message: MultisigMessageV1<C::Point> = bincode::deserialize(payload)
		.map_err(|e| anyhow!("Failed to deserialize message (version: 1): {:?}", e))?;
	Ok(MultisigMessage { ceremony_id: message.ceremony_id, data: message.data.into() })
}

pub fn deserialize_for_version<C: CryptoScheme>(
	message: VersionedCeremonyMessage,
) -> Result<MultisigMessage<C::Point>> {
	match message.version {
		1 => {
			// NOTE: version 1 did not expect signing over multiple payloads,
			// so we need to parse them using the old format and transform to the new
			// format:
			deserialize_from_v1::<C>(&message.payload)
		},
		2 => bincode::deserialize::<'_, MultisigMessage<C::Point>>(&message.payload).map_err(|e| {
			anyhow!("Failed to deserialize message (version: {}): {:?}", message.version, e)
		}),
		_ => Err(anyhow!("Unsupported message version: {}", message.version)),
	}
}

impl<C: CryptoScheme> CeremonyManager<C> {
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

	async fn on_request(&mut self, request: CeremonyRequest<C>, scope: &Scope<'_, anyhow::Error>) {
		// Always update the latest ceremony id, even if we are not participating
		self.update_latest_ceremony_id(request.ceremony_id);

		match request.details {
			Some(CeremonyRequestDetails::Keygen(details)) => self.on_keygen_request(
				request.ceremony_id,
				details.participants,
				details.rng,
				details.result_sender,
				scope,
			),
			Some(CeremonyRequestDetails::Sign(details)) => {
				self.on_request_to_sign(
					request.ceremony_id,
					details.participants,
					details.payload,
					details.keygen_result_info,
					details.rng,
					details.result_sender,
					scope,
				);
			},
			None => {
				// Because unauthorised ceremonies don't timeout, We must check the id of ceremonies
				// that we are not participating in and cleanup any unauthorised ceremonies that may
				// have been created by a bad p2p message.
				if self.signing_states.cleanup_unauthorised_ceremony(&request.ceremony_id) {
					SigningFailureReason::NotParticipatingInUnauthorisedCeremony
						.log(&BTreeSet::default());
				}
				if self.keygen_states.cleanup_unauthorised_ceremony(&request.ceremony_id) {
					KeygenFailureReason::NotParticipatingInUnauthorisedCeremony
						.log(&BTreeSet::default());
				}
			},
		}
	}

	pub async fn run(
		mut self,
		mut ceremony_request_receiver: UnboundedReceiver<CeremonyRequest<C>>,
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
							match deserialize_for_version::<C>(data) {
								Ok(message) => self.process_p2p_message(sender_id, message, scope),
								Err(_) => {
									warn!("Failed to deserialize message from: {sender_id}");
								},
							}
						}
						Some((id, outcome)) = self.signing_states.outcome_receiver.recv() => {
							self.signing_states.finalize_authorised_ceremony(id, outcome);
						}
						Some((id, outcome)) = self.keygen_states.outcome_receiver.recv() => {
							self.keygen_states.finalize_authorised_ceremony(id, outcome);
						}
					}
				}
			}
			.instrument(info_span!("MultisigClient", chain = C::NAME))
			.boxed()
		})
		.await
	}

	/// Process a keygen request
	fn on_keygen_request(
		&mut self,
		ceremony_id: CeremonyId,
		participants: BTreeSet<AccountId>,
		rng: Rng,
		result_sender: CeremonyResultSender<KeygenCeremony<C>>,
		scope: &Scope<'_, anyhow::Error>,
	) {
		assert!(!participants.is_empty(), "Keygen request has no participants");

		let span = info_span!("Keygen Ceremony", ceremony_id = ceremony_id);
		let _entered = span.enter();

		debug!("Processing a keygen request");

		if participants.len() == 1 {
			let _result = result_sender.send(Ok(self.single_party_keygen(rng)));
			return
		}

		let request = match prepare_keygen_request(
			ceremony_id,
			&self.my_account_id,
			participants,
			&self.outgoing_p2p_message_sender,
			// For now, we don't expect re-sharing requests
			None,
			rng,
		) {
			Ok(request) => request,
			Err(failed_outcome) => {
				let _res = result_sender.send(CeremonyOutcome::<KeygenCeremony<C>>::Err((
					BTreeSet::new(),
					failed_outcome,
				)));

				// Remove a possible unauthorised ceremony
				self.keygen_states.cleanup_unauthorised_ceremony(&ceremony_id);
				return
			},
		};

		let ceremony_handle =
			self.keygen_states.get_state_or_create_unauthorized(ceremony_id, scope);

		ceremony_handle
			.on_request(request, result_sender)
			.with_context(|| format!("Invalid keygen request with ceremony id {ceremony_id}"))
			.unwrap();
	}

	/// Process a request to sign
	fn on_request_to_sign(
		&mut self,
		ceremony_id: CeremonyId,
		signers: BTreeSet<AccountId>,
		payloads: Vec<C::SigningPayload>,
		key_info: KeygenResultInfo<C>,
		rng: Rng,
		result_sender: CeremonyResultSender<SigningCeremony<C>>,
		scope: &Scope<'_, anyhow::Error>,
	) {
		assert!(!signers.is_empty(), "Request to sign has no signers");

		let span = info_span!("Signing Ceremony", ceremony_id = ceremony_id);
		let _entered = span.enter();

		debug!("Processing a request to sign");

		if signers.len() == 1 {
			let _result =
				result_sender.send(Ok(self.single_party_signing(payloads, key_info, rng)));
			return
		}

		let request = match prepare_signing_request(
			ceremony_id,
			&self.my_account_id,
			signers,
			key_info,
			payloads,
			&self.outgoing_p2p_message_sender,
			rng,
		) {
			Ok(request) => request,
			Err(failed_outcome) => {
				let _res = result_sender.send(CeremonyOutcome::<SigningCeremony<C>>::Err((
					BTreeSet::new(),
					failed_outcome,
				)));

				// Remove a possible unauthorised ceremony
				self.signing_states.cleanup_unauthorised_ceremony(&ceremony_id);
				return
			},
		};

		// We have the key and have received a request to sign
		let ceremony_handle =
			self.signing_states.get_state_or_create_unauthorized(ceremony_id, scope);

		ceremony_handle
			.on_request(request, result_sender)
			.with_context(|| format!("Invalid sign request with ceremony id {ceremony_id}"))
			.unwrap();
	}

	/// Process message from another validator
	fn process_p2p_message(
		&mut self,
		sender_id: AccountId,
		message: MultisigMessage<C::Point>,
		scope: &Scope<'_, anyhow::Error>,
	) {
		match message {
			MultisigMessage { ceremony_id, data: MultisigData::Keygen(data) } => {
				let span = info_span!("Keygen Ceremony", ceremony_id = ceremony_id);
				let _entered = span.enter();

				self.keygen_states.process_data(
					sender_id,
					ceremony_id,
					data,
					self.latest_ceremony_id,
					scope,
				)
			},
			MultisigMessage { ceremony_id, data: MultisigData::Signing(data) } => {
				let span = info_span!("Signing Ceremony", ceremony_id = ceremony_id);
				let _entered = span.enter();

				self.signing_states.process_data(
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

	fn single_party_keygen(&self, mut rng: Rng) -> KeygenResultInfo<C> {
		info!("Performing solo keygen");

		let (_key_id, key_data) =
			generate_key_data::<C>(BTreeSet::from_iter([self.my_account_id.clone()]), &mut rng);
		key_data[&self.my_account_id].clone()
	}

	fn single_party_signing(
		&self,
		payloads: Vec<C::SigningPayload>,
		keygen_result_info: KeygenResultInfo<C>,
		mut rng: Rng,
	) -> Vec<C::Signature> {
		info!("Performing solo signing");

		let key = &keygen_result_info.key.key_share;

		payloads
			.iter()
			.map(|payload| generate_single_party_signature::<C>(&key.x_i, payload, &mut rng))
			.collect()
	}
}

/// Create unique deterministic context used for generating a ZKP to prevent replay attacks
fn generate_keygen_context(ceremony_id: CeremonyId, signers: BTreeSet<AccountId>) -> HashContext {
	use sha2::{Digest, Sha256};

	let mut hasher = Sha256::new();

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
	fn process_data(
		&mut self,
		sender_id: AccountId,
		ceremony_id: CeremonyId,
		data: Ceremony::Data,
		latest_ceremony_id: CeremonyId,
		scope: &Scope<'_, anyhow::Error>,
	) {
		debug!("Received data {data} from [{sender_id}]");

		// Get the existing ceremony or create an unauthorised one (with ceremony id tracking check)
		let ceremony_handle = match self.ceremony_handles.entry(ceremony_id) {
			hash_map::Entry::Vacant(entry) => {
				// Only a ceremony id that is within the ceremony id window can create unauthorised
				// ceremonies
				if ceremony_id > latest_ceremony_id + CEREMONY_ID_WINDOW {
					warn!("Ignoring data: unexpected future ceremony id {}", ceremony_id);
					return
				} else if ceremony_id < latest_ceremony_id {
					trace!("Ignoring data: old ceremony id {}", ceremony_id);
					return
				} else {
					entry.insert(CeremonyHandle::spawn(
						ceremony_id,
						self.outcome_sender.clone(),
						scope,
					))
				}
			},
			hash_map::Entry::Occupied(entry) => entry.into_mut(),
		};

		// NOTE: There is a short delay between dropping the ceremony runner (and any channels
		// associated with it) and dropping the corresponding ceremony handle, which makes it
		// possible for the following `send` to fail
		if ceremony_handle.message_sender.send((sender_id, data)).is_err() {
			debug!("Ignoring data: ceremony runner has been dropped");
		}
	}

	/// Returns the state for the given ceremony id if it exists,
	/// otherwise creates a new unauthorized one
	fn get_state_or_create_unauthorized(
		&mut self,
		ceremony_id: CeremonyId,
		scope: &Scope<'_, anyhow::Error>,
	) -> &mut CeremonyHandle<Ceremony> {
		self.ceremony_handles.entry(ceremony_id).or_insert_with(|| {
			CeremonyHandle::spawn(ceremony_id, self.outcome_sender.clone(), scope)
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
	fn spawn(
		ceremony_id: CeremonyId,
		outcome_sender: UnboundedSender<(CeremonyId, CeremonyOutcome<Ceremony>)>,
		scope: &Scope<'_, anyhow::Error>,
	) -> Self {
		let (message_sender, message_receiver) = mpsc::unbounded_channel();
		let (request_sender, request_receiver) = oneshot::channel();

		let task_handle = scope.spawn_with_handle(CeremonyRunner::<Ceremony>::run(
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

#[cfg(test)]
mod key_id_agg_key_match {
	use cf_chains::ChainCrypto;
	use rand_legacy::SeedableRng;

	use crate::multisig::{bitcoin::BtcSigning, eth::EthSigning, polkadot::PolkadotSigning};

	use super::*;

	fn test_agg_key_key_id_match<CScheme, CC>()
	where
		CScheme: CryptoScheme,
		CC: ChainCrypto,
		CC::AggKey: From<CScheme::AggKey>,
	{
		let rng = crate::multisig::crypto::Rng::from_seed([0u8; 32]);
		let agg_key = CeremonyManager::<CScheme>::new(
			[4u8; 32].into(),
			tokio::sync::mpsc::unbounded_channel().0,
			0,
		)
		.single_party_keygen(rng)
		.key
		.agg_key();

		let public_key_bytes: Vec<u8> = agg_key.clone().into();

		assert_eq!(
			CC::agg_key_to_key_id(CC::AggKey::from(agg_key), 9).public_key_bytes,
			public_key_bytes
		);
	}

	#[test]
	fn test() {
		test_agg_key_key_id_match::<BtcSigning, cf_chains::Bitcoin>();
		test_agg_key_key_id_match::<EthSigning, cf_chains::Ethereum>();
		test_agg_key_key_id_match::<PolkadotSigning, cf_chains::Polkadot>();
	}
}
