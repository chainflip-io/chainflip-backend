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

//! Voting in every `pallet-cf-elections` instance through one extrinsic: the payload the
//! engine's voters fill in, and the runtime's implementation of `ElectionInstancesVoting`.

use crate::Runtime;
use cf_primitives::ForeignChain;
use cf_traits::elections::{ElectionInstance, ElectionInstancesVoting, VoterContext};
use codec::{Decode, DecodeWithMemTracking, Encode};
use frame_support::{
	instances::{Instance1, Instance3, Instance4, Instance5, Instance6, Instance7, Instance8},
	pallet_prelude::DispatchError,
	weights::Weight,
};
use scale_info::TypeInfo;
use sp_std::prelude::*;

/// Implemented per elections instance so that a caller holding one instance's votes - the
/// engine's per-instance voter - can package them for [`Call::submit_elections_votes`] knowing
/// only its own instance.
///
/// Keyed on the instance marker rather than on the votes: `AuthorityVotes<Runtime, I>` is built
/// from associated-type projections through `I`, and coherence does not normalise projections
/// when checking impl overlap, so `From<AuthorityVotes<Runtime, I>>` impls are rejected as
/// possibly-overlapping however concretely `I` is spelled. `Instance1` in an impl header has
/// nothing to normalise.
pub trait BatchedInstance: Sized + 'static
where
	Runtime: pallet_cf_elections::Config<Self>,
{
	/// This instance's votes, as a batch carrying only them.
	fn votes(
		votes: pallet_cf_elections::AuthorityVotes<Runtime, Self>,
	) -> AllElectionInstancesVotes;
}

/// The set of elections instances a validator votes in, as one unit.
pub struct AllElectionInstances;

/// The one place the set of elections instances is enumerated: adding a chain means adding a
/// line here. The batch type carrying their votes, and everything that walks the instances -
/// packaging one instance's votes, voting them all in, and the weight of doing so - is generated
/// from that list, so none of it can be left behind.
///
/// Spelled with the concrete `InstanceN` types rather than the `EthereumInstance` aliases -
/// those aliases are themselves projections (`<Ethereum as PalletInstanceAlias>::Instance`),
/// which coherence cannot tell apart either.
macro_rules! election_instances {
	($( $field:ident => $instance:ty = $election_instance:expr ),+ $(,)?) => {

		/// Votes for every `pallet-cf-elections` instance, carried by a single
		/// `Environment::submit_elections_votes` extrinsic.
		///
		/// Each field is optional so a validator can target any subset of instances - in
		/// practice it sends whichever ones produced votes this block. Fields follow the
		/// instance list, so their order - and with it the encoding - is that list's.
		#[derive(
			Clone, Debug, Default, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo,
		)]
		pub struct AllElectionInstancesVotes {
			$(
				pub $field: Option<Box<pallet_cf_elections::AuthorityVotes<Runtime, $instance>>>,
			)+
		}

		// Implementing `BatchedInstance`: filling one slot, for a caller that knows only its
		// own instance.
		$(
			impl BatchedInstance for $instance {
				fn votes(
					votes: pallet_cf_elections::AuthorityVotes<Runtime, Self>,
				) -> AllElectionInstancesVotes {
					AllElectionInstancesVotes {
						$field: Some(Box::new(votes)),
						..Default::default()
					}
				}
			}
		)+

		// Implementing the batch own API: gathering several instances into one batch, for
		// the engine's vote batcher.
		impl AllElectionInstancesVotes {
			/// How many instances this batch carries votes for.
			pub fn instances(&self) -> usize {
				[$( self.$field.is_some() ),+].into_iter().filter(|carried| *carried).count()
			}

			/// Merge `other` in, or hand it straight back if any instance it carries already
			/// has votes here - a batch has room for one set of votes per instance.
			///
			/// Handing `other` back rather than overwriting is what makes it impossible to
			/// drop votes: a caller gathering several instances either merged them, or still
			/// holds them and must put them in another batch.
			pub fn try_merge(&mut self, other: Self) -> Result<(), Self> {
				if true $( && !(self.$field.is_some() && other.$field.is_some()) )+ {
					$( if other.$field.is_some() { self.$field = other.$field; } )+
					Ok(())
				} else {
					Err(other)
				}
			}
		}

		// Implementing `ElectionInstancesVoting`: unpacking a batch on the receiving end
		impl ElectionInstancesVoting<Runtime> for AllElectionInstances {
			type Votes = AllElectionInstancesVotes;

			fn authorise_voter_weight() -> Weight {
				// Instance-agnostic, so any instance's weights serve.
				pallet_cf_elections::Pallet::<Runtime, ()>::authorise_voter_weight()
			}

			fn vote_all_weight(votes: &Self::Votes) -> Weight {
				Weight::zero()
					$( .saturating_add(
						votes.$field.as_ref().map_or(Weight::zero(), |votes| {
							pallet_cf_elections::Pallet::<Runtime, $instance>::do_vote_weight(
								votes.len() as u32,
							)
						})
					) )+
			}

			fn vote_all(
				context: &VoterContext<Runtime>,
				votes: Self::Votes,
			) -> sp_std::vec::Vec<(ElectionInstance, DispatchError)> {
				let mut failures = sp_std::vec::Vec::new();

				// Each instance gets its own storage layer, so one rejecting its votes neither
				// aborts the others nor rolls back what they already wrote. `do_vote` is a plain
				// function, so unlike a dispatchable it gets no such layer automatically.
				$(
					if let Some(votes) = votes.$field {
						if let Err(error) = frame_support::storage::with_storage_layer(|| {
							pallet_cf_elections::Pallet::<Runtime, $instance>::do_vote(
								context,
								*votes,
							)
						}) {
							failures.push(($election_instance, error));
						}
					}
				)+

				failures
			}
		}
	};
}

election_instances! {
	generic => () = ElectionInstance::Generic,
	ethereum => Instance1 = ElectionInstance::Chain(ForeignChain::Ethereum),
	bitcoin => Instance3 = ElectionInstance::Chain(ForeignChain::Bitcoin),
	arbitrum => Instance4 = ElectionInstance::Chain(ForeignChain::Arbitrum),
	solana => Instance5 = ElectionInstance::Chain(ForeignChain::Solana),
	assethub => Instance6 = ElectionInstance::Chain(ForeignChain::Assethub),
	tron => Instance7 = ElectionInstance::Chain(ForeignChain::Tron),
	bsc => Instance8 = ElectionInstance::Chain(ForeignChain::Bsc),
}
