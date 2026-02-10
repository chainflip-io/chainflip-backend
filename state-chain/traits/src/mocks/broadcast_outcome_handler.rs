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

use super::{MockPallet, MockPalletStorage};
use crate::BroadcastOutcomeHandler;
use cf_chains::Chain;
use cf_primitives::BroadcastId;
use codec::{Decode, Encode};
use scale_info::TypeInfo;
use sp_std::{marker::PhantomData, vec::Vec};

pub struct MockBroadcastOutcomeHandler<C>(PhantomData<C>);

impl<C> MockPallet for MockBroadcastOutcomeHandler<C> {
	const PREFIX: &'static [u8] = b"MockBroadcastOutcomeHandler";
}

const OUTCOMES: &[u8] = b"OUTCOMES";

#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub enum MockBroadcastOutcome<ChainBlockNumber> {
	Success { broadcast_id: BroadcastId, witness_block: ChainBlockNumber },
	Aborted { broadcast_id: BroadcastId },
	Expired { broadcast_id: BroadcastId },
}

impl<C: Chain> MockBroadcastOutcomeHandler<C> {
	fn push_outcome(outcome: MockBroadcastOutcome<C::ChainBlockNumber>) {
		Self::mutate_value(OUTCOMES, |outcomes: &mut Option<Vec<_>>| {
			outcomes.get_or_insert_default().push(outcome);
		});
	}

	pub fn get_outcomes() -> Vec<MockBroadcastOutcome<C::ChainBlockNumber>> {
		Self::get_value(OUTCOMES).unwrap_or_default()
	}

	pub fn take_outcomes() -> Vec<MockBroadcastOutcome<C::ChainBlockNumber>> {
		Self::take_value(OUTCOMES).unwrap_or_default()
	}
}

impl<C: Chain> BroadcastOutcomeHandler<C> for MockBroadcastOutcomeHandler<C> {
	fn on_broadcast_success(broadcast_id: BroadcastId, witness_block: C::ChainBlockNumber) {
		Self::push_outcome(MockBroadcastOutcome::Success { broadcast_id, witness_block });
	}

	fn on_broadcast_aborted(broadcast_id: BroadcastId) {
		Self::push_outcome(MockBroadcastOutcome::Aborted { broadcast_id });
	}

	fn on_broadcast_expired(broadcast_id: BroadcastId) {
		Self::push_outcome(MockBroadcastOutcome::Expired { broadcast_id });
	}
}
