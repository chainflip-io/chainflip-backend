// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeSet, sync::Arc};

use crate::{
	client::{ceremony_manager::CeremonyTrait, utils::PartyIdxMapping},
	crypto::Rng,
	p2p::OutgoingMultisigStageMessages,
	ChainSigning,
};
use async_trait::async_trait;
use cf_primitives::{AuthorityCount, CeremonyId};
use cf_utilities::metrics::CeremonyMetrics;
use tokio::sync::mpsc::UnboundedSender;

/// Outcome of a given ceremony stage
pub enum StageResult<C: CeremonyTrait> {
	/// Ceremony proceeds to the next stage
	NextStage(Box<dyn CeremonyStage<C> + Send + Sync>),
	/// Ceremony aborted (contains parties to report)
	Error(BTreeSet<AuthorityCount>, C::FailureReason),
	/// Ceremony finished and successful
	Done(C::Output),
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
#[async_trait]
pub trait CeremonyStage<C: CeremonyTrait> {
	/// Perform initial computation for this stage (and initiate communication with other parties)
	fn init(&mut self, metrics: &mut CeremonyMetrics) -> ProcessMessageResult;

	/// Process message from signer at index `signer_idx`. Precondition: the signer is a valid
	/// holder of the key and selected to participate in this ceremony (TODO: also check that
	/// we haven't processed a message from them?)
	fn process_message(
		&mut self,
		signer_idx: AuthorityCount,
		m: C::Data,
		metrics: &mut CeremonyMetrics,
	) -> ProcessMessageResult;

	/// Verify data for this stage after it is received from all other parties,
	/// either abort or proceed to the next stage based on the result
	async fn finalize(self: Box<Self>, metrics: &mut CeremonyMetrics) -> StageResult<C>;

	/// Parties we haven't heard from for the current stage
	fn awaited_parties(&self) -> BTreeSet<AuthorityCount>;

	fn get_stage_name(&self) -> C::CeremonyStageName;

	fn ceremony_common(&self) -> &CeremonyCommon;
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
	pub number_of_signing_payloads: Option<usize>,
}

impl CeremonyCommon {
	pub fn is_idx_valid(&self, idx: AuthorityCount) -> bool {
		self.all_idxs.contains(&idx)
	}
}

pub trait PreProcessStageDataCheck<CeremonyStageName> {
	/// Check that the number of elements in the data is correct
	fn is_data_size_valid<Chain: ChainSigning>(
		&self,
		num_of_parties: AuthorityCount,
		num_of_payloads: Option<usize>,
	) -> bool;

	/// Check that the number of elements in the data is within expected bounds.
	/// This is needed because we may not know how many parties are going to participate yet.
	fn is_initial_stage_data_size_valid<Chain: ChainSigning>(&self) -> bool;

	/// Returns true if this message should be delayed if the ceremony is still unauthorised.
	/// This is needed because a message may arrive before the ceremony request.
	fn should_delay_unauthorised(&self) -> bool;

	/// Returns true if this message should be delayed for the given stage
	fn should_delay(stage_name: CeremonyStageName, message: &Self) -> bool;
}
