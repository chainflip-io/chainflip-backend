use crate::{
	electoral_system::{
		AuthorityVoteOf, ConsensusVotes, ElectionReadAccess, ElectionWriteAccess, ElectoralSystem,
		ElectoralSystemTypes, ElectoralWriteAccess, PartialVoteOf, VoteOf, VotePropertiesOf,
	},
	vote_storage, CorruptStorageError, ElectionIdentifier,
};
use cf_chains::benchmarking_value::BenchmarkValue;
use cf_runtime_utilities::log_or_panic;
use cf_traits::IngressSink;
use cf_utilities::success_threshold_from_share_count;
use codec::{Decode, Encode, MaxEncodedLen};
use core::cmp::Ordering;
use frame_support::{
	pallet_prelude::{MaybeSerializeDeserialize, Member},
	sp_runtime::traits::Zero,
	storage::bounded_btree_map::BoundedBTreeMap,
	Parameter,
};
use scale_info::TypeInfo;
use sp_core::ConstU32;
use sp_std::{
	collections::{btree_map::BTreeMap, vec_deque::VecDeque},
	ops::{Add, Rem},
	vec,
	vec::Vec,
};

use serde::{Deserialize, Serialize};

pub const MAXIMUM_CHANNELS_PER_ELECTION: u32 = 50;

/// Represents the total ingressed amount over all time of a given asset at a particular
/// `block_number`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct ChannelTotalIngressed<BlockNumber, Amount> {
	pub block_number: BlockNumber,
	pub amount: Amount,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Encode, Decode, TypeInfo, MaxEncodedLen)]
pub struct OpenChannelDetails<Asset, BlockNumber> {
	pub asset: Asset,
	pub close_block: BlockNumber,
}

pub type ChannelTotalIngressedFor<Sink> =
	ChannelTotalIngressed<<Sink as IngressSink>::BlockNumber, <Sink as IngressSink>::Amount>;

pub type OpenChannelDetailsFor<Sink> =
	OpenChannelDetails<<Sink as IngressSink>::Asset, <Sink as IngressSink>::BlockNumber>;

#[derive(
	Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo, Deserialize, Serialize, Default,
)]
pub struct BackoffSettings<StateChainBlockNumber> {
	// After this number of state chain blocks, we will backoff request frequency. To
	// request every 10 mintutes / 100 blocks.
	pub backoff_after_blocks: StateChainBlockNumber,
	// The frequency of requests after the backoff period.
	pub backoff_frequency: StateChainBlockNumber,
}

#[cfg(feature = "runtime-benchmarks")]
impl BenchmarkValue for BackoffSettings<u32> {
	fn benchmark_value() -> Self {
		Self { backoff_after_blocks: 600, backoff_frequency: 100 }
	}
}

pub struct DeltaBasedIngress<Sink: IngressSink, Settings, ValidatorId, StateChainBlockNumber> {
	_phantom: core::marker::PhantomData<(Sink, Settings, ValidatorId, StateChainBlockNumber)>,
}
impl<Sink, Settings, ValidatorId, StateChainBlockNumber>
	DeltaBasedIngress<Sink, Settings, ValidatorId, StateChainBlockNumber>
where
	Sink: IngressSink<DepositDetails = ()> + 'static,
	Settings: Parameter + Member + MaybeSerializeDeserialize + Eq,
	<Sink as IngressSink>::Account: Ord,
	<Sink as IngressSink>::Amount: Default,
	ValidatorId: Member + Parameter + Ord + MaybeSerializeDeserialize,
	StateChainBlockNumber: Member + Parameter + Ord + MaybeSerializeDeserialize,
{
	pub fn open_channel<ElectoralAccess: ElectoralWriteAccess<ElectoralSystem = Self> + 'static>(
		election_identifiers: Vec<
			ElectionIdentifier<<Self as ElectoralSystemTypes>::ElectionIdentifierExtra>,
		>,
		channel: Sink::Account,
		asset: Sink::Asset,
		close_block: Sink::BlockNumber,
		current_state_chain_block_number: StateChainBlockNumber,
	) -> Result<(), CorruptStorageError> {
		let channel_details = (
			OpenChannelDetails { asset, close_block },
			ElectoralAccess::unsynchronised_state_map(&(channel.clone(), asset))?.unwrap_or(
				ChannelTotalIngressed { block_number: Zero::zero(), amount: Zero::zero() },
			),
		);
		if let Some(election_identifier) = election_identifiers.last() {
			let mut election_access = ElectoralAccess::election_mut(*election_identifier);
			let (mut channels, _last_channel_opened_at) = election_access.properties()?;
			if channels.len() < MAXIMUM_CHANNELS_PER_ELECTION as usize {
				channels.insert(channel, channel_details);
				election_access.refresh(
					election_identifier
						.extra()
						.checked_add(1)
						.ok_or_else(CorruptStorageError::new)?,
					(channels, current_state_chain_block_number),
				)?;
				return Ok(())
			}
		}

		ElectoralAccess::new_election(
			Default::default(), /* We use the lowest value, so we can refresh the elections the
			                     * maximum number of times */
			([(channel, channel_details)].into_iter().collect(), current_state_chain_block_number),
			Default::default(),
		)?;

		Ok(())
	}
}
impl<Sink, Settings, ValidatorId, StateChainBlockNumber> ElectoralSystemTypes
	for DeltaBasedIngress<Sink, Settings, ValidatorId, StateChainBlockNumber>
where
	Sink: IngressSink<DepositDetails = ()> + 'static,
	Settings: Parameter + Member + MaybeSerializeDeserialize + Eq,
	<Sink as IngressSink>::Account: Ord,
	<Sink as IngressSink>::Amount: Default,
	ValidatorId: Member + Parameter + Ord + MaybeSerializeDeserialize,
	StateChainBlockNumber: Member + Parameter + Ord + MaybeSerializeDeserialize,
{
	type ValidatorId = ValidatorId;
	type StateChainBlockNumber = StateChainBlockNumber;
	type ElectoralUnsynchronisedState = ();

	// Stores the total ingressed amounts for all channels that have already been dispatched i.e. we
	// told the `IngressEgress` pallet about, and for example, for swap deposit channels, has been
	// scheduled to be swapped.
	type ElectoralUnsynchronisedStateMapKey = (Sink::Account, Sink::Asset);
	type ElectoralUnsynchronisedStateMapValue = ChannelTotalIngressedFor<Sink>;

	type ElectoralUnsynchronisedSettings = ();
	type ElectoralSettings = (Settings, BackoffSettings<StateChainBlockNumber>);
	type ElectionIdentifierExtra = u32;

	// Stores the channels a given election is witnessing, and a recent total ingressed value.
	type ElectionProperties = (
		BTreeMap<Sink::Account, (OpenChannelDetailsFor<Sink>, ChannelTotalIngressedFor<Sink>)>,
		// Last Channel Opened At - We use this to determin when it is ok to backoff
		// request frequency.
		StateChainBlockNumber,
	);

	// Stores the any pending total ingressed values that are waiting for
	// the safety margin to pass.
	type ElectionState = BTreeMap<Sink::Account, ChannelTotalIngressedFor<Sink>>;
	type VoteStorage = vote_storage::individual::Individual<
		(),
		vote_storage::individual::identity::Identity<
			BoundedBTreeMap<
				Sink::Account,
				ChannelTotalIngressedFor<Sink>,
				ConstU32<MAXIMUM_CHANNELS_PER_ELECTION>,
			>,
		>,
	>;
	type Consensus = BTreeMap<Sink::Account, ChannelTotalIngressedFor<Sink>>;
	type OnFinalizeContext = Sink::BlockNumber;
	type OnFinalizeReturn = ();
}

impl<Sink, Settings, ValidatorId, StateChainBlockNumber> ElectoralSystem
	for DeltaBasedIngress<Sink, Settings, ValidatorId, StateChainBlockNumber>
where
	Sink: IngressSink<DepositDetails = ()> + 'static,
	Settings: Parameter + Member + MaybeSerializeDeserialize + Eq,
	<Sink as IngressSink>::Account: Ord,
	<Sink as IngressSink>::Amount: Default,
	ValidatorId: Member + Parameter + Ord + MaybeSerializeDeserialize,
	StateChainBlockNumber: Member
		+ Parameter
		+ Ord
		+ MaybeSerializeDeserialize
		+ Add<Output = StateChainBlockNumber>
		+ Rem<Output = StateChainBlockNumber>
		+ frame_support::sp_runtime::traits::Zero,
{
	fn is_vote_desired<ElectionAccess: ElectionReadAccess<ElectoralSystem = Self>>(
		election_access: &ElectionAccess,
		_current_vote: Option<(VotePropertiesOf<Self>, AuthorityVoteOf<Self>)>,
		current_state_chain_block_number: Self::StateChainBlockNumber,
	) -> Result<bool, CorruptStorageError> {
		let (_settings, backoff_settings) = election_access.settings()?;
		let (_channel_properties, last_channel_opened_at) = election_access.properties()?;

		// We want to vote if:
		// 1. We are still in the first few blocks (before the backoff_after_blocks period has
		//    elapsed)
		// 2. The backoff_after_blocks period has elapsed, but we are on a block that is a multiple
		//    of backoff_frequency
		Ok(!((current_state_chain_block_number.clone() >
			last_channel_opened_at + backoff_settings.backoff_after_blocks) &&
			(current_state_chain_block_number.clone() % backoff_settings.backoff_frequency !=
				Zero::zero())))
	}

	fn is_vote_needed(
		(_, _, _): (VotePropertiesOf<Self>, PartialVoteOf<Self>, AuthorityVoteOf<Self>),
		(_, proposed_vote): (PartialVoteOf<Self>, VoteOf<Self>),
	) -> bool {
		!proposed_vote.is_empty()
	}

	fn generate_vote_properties(
		_election_identifier: ElectionIdentifier<Self::ElectionIdentifierExtra>,
		_previous_vote: Option<(VotePropertiesOf<Self>, AuthorityVoteOf<Self>)>,
		_vote: &PartialVoteOf<Self>,
	) -> Result<VotePropertiesOf<Self>, CorruptStorageError> {
		Ok(())
	}

	fn on_finalize<ElectoralAccess: ElectoralWriteAccess<ElectoralSystem = Self> + 'static>(
		election_identifiers: Vec<ElectionIdentifier<Self::ElectionIdentifierExtra>>,
		chain_tracking: &Self::OnFinalizeContext,
	) -> Result<Self::OnFinalizeReturn, CorruptStorageError> {
		for election_identifier in election_identifiers {
			let (
				(old_channel_properties, last_channel_opened_at),
				pending_ingress_totals,
				option_consensus,
			) = {
				let election_access = ElectoralAccess::election_mut(election_identifier);
				(
					election_access.properties()?,
					election_access.state()?,
					election_access.check_consensus()?.has_consensus(),
				)
			};

			let mut new_properties = old_channel_properties.clone();
			let mut new_pending_ingress_totals =
				<Self as ElectoralSystemTypes>::ElectionState::default();
			for (account, (details, _)) in &old_channel_properties {
				// We currently split the ingressed amount into two parts:
				// 1. The consensus amount with a block number *earlier* than chain tracking. i.e.
				//    Chain tracking is *ahead*, deposit witnessing is lagging.
				// 2. The pending amount that with a block number *later* than chain tracking. i.e.
				//    Chain tracking is *lagging*, deposit witnessing is ahead.
				// The engines currently do not necessarily agree on a particular value at the point
				// of an election because of the inability to query for data at
				// a particular block height on Solana. Thus, there are two approaches:
				// 1. Wait until all the engines agree on a particular value, which is guaranteed to
				//    *eventually* occur, given deposits are on the Solana blockchain, which is a
				//    source of truth for the engines.
				// 2. Use chain tracking to determine a block height at which we can dispatch a
				//    deposit action *so far*.
				// We cannot use approach 1. because it creates an attack scenario where an attacker
				// can send the smallest unit of Solana in a stream to the victim's deposit channel,
				// delaying the victim's deposit until the attacker stops their stream.

				let (ready_total, future_total) = match (
					option_consensus.as_ref().and_then(|consensus| consensus.get(account)),
					pending_ingress_totals.get(account),
				) {
					(None, None) => (None, None),
					(Some(total), None) | (None, Some(total)) => {
						if total.block_number <= *chain_tracking {
							(Some(total), None)
						} else {
							(None, Some(total))
						}
					},
					(Some(new_consensus), Some(old_consensus)) => {
						if new_consensus.block_number <= old_consensus.block_number {
							// Not sure if this is possible, but can't exclude it either. Can
							// indicate a re-org or misbehaving rpcs. Ignore the previous
							// amount.
							if *chain_tracking >= new_consensus.block_number {
								(Some(new_consensus), None)
							} else {
								(None, Some(new_consensus))
							}
						} else {
							// In this branch we handle the 'happy' case where block numbers are
							// monotonically increasing.
							if *chain_tracking >= new_consensus.block_number {
								// Chain tracking has progressed beyond the latest ingress block so
								// we can ignore any previously pending amounts.
								(Some(new_consensus), None)
							} else if *chain_tracking >= old_consensus.block_number {
								// Chain tracking has progressed beyond the previous deposit block
								// but not the latest. We can confirm the previous amount, the
								// latest will become pending, as long as the amounts are different.
								debug_assert!(new_consensus.amount >= old_consensus.amount);
								// NOTE: We can be sure of this because the block numbers cannot
								// decrease and, if they were equal, we would have entered the
								// initial condition.
								debug_assert!(
									new_consensus.block_number > old_consensus.block_number
								);

								if new_consensus.amount >= old_consensus.amount {
									// Note: balance can be equal on channel closure.
									(Some(old_consensus), Some(new_consensus))
								} else {
									log_or_panic!(
										"Consensus {:?} balance for Solana deposit channel `{:?}` decreased from {:?} to {:?}",
										details.asset,
										account,
										old_consensus.amount,
										new_consensus.amount
									);
									(Some(old_consensus), None)
								}
							} else {
								// Chain tracking has not progressed beyond the previous deposit
								// block. We can confirm neither the previous nor the latest amount.
								// We don't update the pending consensus amount: this is to defend
								// against a malicious actor streaming small amounts, which
								// would otherwise delay the deposit.
								if new_consensus.amount == old_consensus.amount {
									// If amounts are the same it could be account closure.
									(None, Some(new_consensus))
								} else {
									(None, Some(old_consensus))
								}
							}
						}
					},
				};

				if let Some(ready_total) = ready_total {
					let previous_amount = ElectoralAccess::unsynchronised_state_map(&(
						account.clone(),
						details.asset,
					))?
					.map_or(Zero::zero(), |previous_total_ingressed| {
						previous_total_ingressed.amount
					});
					match previous_amount.cmp(&ready_total.amount) {
						Ordering::Less => {
							Sink::on_ingress(
								account.clone(),
								details.asset,
								ready_total.amount - previous_amount,
								ready_total.block_number,
								(),
							);
							ElectoralAccess::set_unsynchronised_state_map(
								(account.clone(), details.asset),
								Some(*ready_total),
							);
							new_properties.entry(account.clone()).and_modify(
								|(_details, total)| {
									*total = *ready_total;
								},
							);
						},
						Ordering::Greater => {
							log::error!(
								"Finalized {:?} balance for Solana deposit channel `{:?}` decreased from {:?} to {:?}",
								details.asset,
								account,
								previous_amount,
								ready_total.amount
							);
						},
						Ordering::Equal => (),
					}
					// Note: we only check for close in this branch to guarantee that both the
					// channel state and chain tracking have caught up to the close block, and all
					// confirmed deposits have been ingressed.
					if ready_total.block_number >= details.close_block {
						Sink::on_channel_closed(account.clone());
						new_properties.remove(account);
					}
				}
				if let Some(future_total) = future_total {
					new_properties.entry(account.clone()).and_modify(|(_details, total)| {
						*total = *future_total;
					});
					new_pending_ingress_totals.insert(account.clone(), *future_total);
				}
			}

			let mut election_access = ElectoralAccess::election_mut(election_identifier);

			if new_properties.is_empty() {
				// Note: it's possible that there are still some remaining pending totals, but if
				// the channel is expired, we need to close it, otherwise an attacker could keep it
				// open indeifitely by streaming small deposits.
				election_access.delete();
			} else if new_properties != old_channel_properties {
				log::debug!("recreate delta based ingress election: recreate since properties changed from: {old_channel_properties:?}, to: {new_properties:?}");

				election_access.clear_votes();
				election_access.set_state(new_pending_ingress_totals)?;
				election_access.refresh(
					election_identifier
						.extra()
						.checked_add(1)
						.ok_or_else(CorruptStorageError::new)?,
					(new_properties, last_channel_opened_at),
				)?;
			} else {
				log::debug!("recreate delta based ingress election: keeping old because properties didn't change: {old_channel_properties:?}");
				election_access.set_state(new_pending_ingress_totals)?;
			}
		}

		Ok(())
	}

	fn check_consensus<ElectionAccess: ElectionReadAccess<ElectoralSystem = Self>>(
		election_access: &ElectionAccess,
		_previous_consensus: Option<&Self::Consensus>,
		consensus_votes: ConsensusVotes<Self>,
	) -> Result<Option<Self::Consensus>, CorruptStorageError> {
		let num_authorities = consensus_votes.num_authorities();
		let threshold = success_threshold_from_share_count(num_authorities);
		let active_votes = consensus_votes.active_votes();
		let num_active_votes = active_votes.len() as u32;
		if num_active_votes >= threshold {
			let (election_channels, _last_channel_opened_at) = election_access.properties()?;

			let mut votes_grouped_by_channel = BTreeMap::<_, Vec<_>>::new();
			for (account, channel_vote) in active_votes.into_iter().flatten() {
				votes_grouped_by_channel.entry(account).or_default().push(channel_vote);
			}

			Ok(Some(
				votes_grouped_by_channel
					.into_iter()
					.filter_map(|(account, channel_votes)| {
						election_channels.get(&account).map(|(_details, recent_ingress_total)| {
							(account, channel_votes, recent_ingress_total)
						})
					})
					.filter_map(|(account, mut channel_votes, recent_ingress_total)| {
						// This approach ensures 2/3rds are needed to decrease the block number or
						// increase the amount. But it has the side effect that potentially less
						// than 1/3 are needed to increase the block number or decrease the amount,
						// but to reliably do it requires validators to be offline and would resolve
						// itself once the validators are back online. Particularly, given A
						// authorities and N are offline then a set of A/3 - N authorities could
						// reliably increase the block number or decrease the amount, while those N
						// authorities are offline.

						// An attacker could also cause this to unreliably happen if no validators
						// are offline, but by pure chance due to the timing
						// of extrinsics being included or produced. For example if an ingress
						// occurs, then authorites will provide new votes reflecting this increase
						// in the channel's total ingressed, but it is reasonable that in the first
						// block where those votes are included, we only include A * 2/3 - N new
						// votes meaning an attacker with N authorities could cause the protocol to
						// get consensus with a lower total ingressed amount than the true value xor
						// with a higher block number than is accurate. This should not be a
						// problem, as once all the votes are included, i.e. in the next few blocks,
						// the consensus would be corrected, and no action is taken based on
						// consensus until the chain tracking reaches the block number which
						// requires atleast A*1/3 to increase artificially if using `UnsafeMedian`
						// or A*2/3 if using `MonotonicMedian`.

						// If an authority has voted, but does not provide a value for a given
						// channel, we assume their vote is that the existing value is accurate.
						// This allows us to safely avoid always providing values for all channels
						// even if they haven't changed. Particularly authorities only need to vote
						// if the amount has changed (from either their current vote or the current
						// ingress total if they haven't provided a vote for that channel) or if the
						// channel has expired (To ensure we get consensus at a block number after
						// the expiry, so we can safely close the channel).
						channel_votes.resize(num_active_votes as usize, *recent_ingress_total);
						channel_votes.sort_by_key(|channel_vote| channel_vote.block_number);
						let contributing_channel_votes = longest_increasing_subsequence_by_key(
							&channel_votes[..],
							|channel_vote| channel_vote.amount,
						);
						if contributing_channel_votes.len() as u32 >= threshold {
							Some((
								account,
								ChannelTotalIngressed {
									// Requires 2/3 to decrease the block_number,
									block_number: contributing_channel_votes
										[threshold as usize - 1]
										.block_number,
									// Requires 2/3 to increase the amount
									amount: contributing_channel_votes
										[contributing_channel_votes.len() - threshold as usize]
										.amount,
								},
							))
						} else {
							None
						}
					})
					.collect(),
			))
		} else {
			Ok(None)
		}
	}
}

fn longest_increasing_subsequence_by_key<V: Clone, X: Ord, F: Fn(&V) -> X>(
	vs: &[V],
	f: F,
) -> Vec<V> {
	if !vs.is_empty() {
		let mut l = 0usize;
		let mut m = vec![0usize; vs.len()];
		let mut p = vec![0usize; vs.len()];

		for (i, v) in vs.iter().enumerate() {
			let little_l = m[..l].partition_point(|j| f(&vs[*j]) <= f(v));

			if little_l > 0 {
				p[i] = m[little_l - 1];
			}
			m[little_l] = i;

			if l < little_l + 1 {
				l = little_l + 1;
			}
		}

		let mut k = m[l - 1];
		let mut s = VecDeque::<V>::new();
		for _ in (0..l).rev() {
			s.push_front(vs[k].clone());
			k = p[k];
		}
		s.into()
	} else {
		Vec::new()
	}
}

#[test]
fn test_longest_increasing_subsequence_by_key() {
	assert_eq!(longest_increasing_subsequence_by_key(&[], |x: &u32| *x), Vec::<u32>::new());
	assert_eq!(longest_increasing_subsequence_by_key(&[5], |x| *x), vec![5]);
	assert_eq!(
		longest_increasing_subsequence_by_key(&[(1, 2), (2, 1), (3, 2)], |(_, y)| *y),
		vec![(2, 1), (3, 2)]
	);
	assert_eq!(
		longest_increasing_subsequence_by_key(
			&[0, 8, 4, 12, 2, 10, 6, 14, 1, 9, 5, 13, 3, 11, 7, 15],
			|x| *x
		),
		vec![0, 2, 6, 9, 11, 15]
	);
	assert_eq!(
		longest_increasing_subsequence_by_key(&[0, 1, 2, 3, 4, 5], |x| *x),
		vec![0, 1, 2, 3, 4, 5]
	);
	assert_eq!(
		longest_increasing_subsequence_by_key(&[10, 0, 1, 2, 3, 4, 5], |x| *x),
		vec![0, 1, 2, 3, 4, 5]
	);
	assert_eq!(
		longest_increasing_subsequence_by_key(&[0, 1, 2, 5, 3, 4, 5], |x| *x),
		vec![0, 1, 2, 3, 4, 5]
	);
	assert_eq!(
		longest_increasing_subsequence_by_key(&[0, 1, 2, 3, 4, 5, 1], |x| *x),
		vec![0, 1, 2, 3, 4, 5]
	);
	assert_eq!(longest_increasing_subsequence_by_key(&[5, 4, 3, 2, 1, 6], |x| *x), vec![1, 6]);
	assert_eq!(longest_increasing_subsequence_by_key(&[5, 4, 3, 2, 6, 1], |x| *x), vec![2, 6]);
	assert_eq!(longest_increasing_subsequence_by_key(&[5, 4, 3, 6, 2, 1], |x| *x), vec![3, 6]);
	assert_eq!(longest_increasing_subsequence_by_key(&[5, 4, 6, 3, 2, 1], |x| *x), vec![4, 6]);
	assert_eq!(longest_increasing_subsequence_by_key(&[5, 6, 4, 3, 2, 1], |x| *x), vec![5, 6]);
	assert_eq!(longest_increasing_subsequence_by_key(&[6, 5, 4, 3, 2, 1], |x| *x), vec![1]);
	assert_eq!(
		longest_increasing_subsequence_by_key(&[10, 9, 2, 5, 3, 7, 101, 18], |x| *x),
		vec![2, 3, 7, 18]
	);
	assert_eq!(longest_increasing_subsequence_by_key(&[4, 4], |x| *x), vec![4, 4]);
	assert_eq!(longest_increasing_subsequence_by_key(&[4, 4, 5, 5], |x| *x), vec![4, 4, 5, 5]);
	assert_eq!(
		longest_increasing_subsequence_by_key(&[1, 4, 4, 5, 5], |x| *x),
		vec![1, 4, 4, 5, 5]
	);
	assert_eq!(longest_increasing_subsequence_by_key(&[34, 23, 23, 45], |x| *x), vec![23, 23, 45]);
	assert_eq!(
		longest_increasing_subsequence_by_key(&[34, 23, 23, 45, 64, 64], |x| *x),
		vec![23, 23, 45, 64, 64]
	);
}
