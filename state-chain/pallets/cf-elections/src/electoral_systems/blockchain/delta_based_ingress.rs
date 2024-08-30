use crate::{
	electoral_system::{
		AuthorityVoteOf, ElectionReadAccess, ElectionWriteAccess, ElectoralSystem,
		ElectoralWriteAccess, VotePropertiesOf,
	},
	vote_storage::{self, VoteStorage},
	CorruptStorageError, ElectionIdentifier,
};
use cf_chains::{assets::any::Asset, Chain};
use cf_primitives::AuthorityCount;
use cf_traits::IngressSink;
use cf_utilities::success_threshold_from_share_count;
use codec::{Decode, Encode, MaxEncodedLen};
use core::cmp::Ordering;
use frame_support::{
	pallet_prelude::{MaybeSerializeDeserialize, Member},
	storage::bounded_btree_map::BoundedBTreeMap,
	Parameter,
};
use scale_info::TypeInfo;
use sp_core::ConstU32;
use sp_std::{
	collections::{btree_map::BTreeMap, vec_deque::VecDeque},
	vec,
	vec::Vec,
};

pub const MAXIMUM_CHANNELS_PER_ELECTION: u32 = 50;

/// Represents the total ingressed amount over all time of a given asset at a particular
/// `block_number`.
#[derive(Clone, PartialEq, Eq, Debug, Encode, Decode, TypeInfo, MaxEncodedLen)]
#[scale_info(skip_type_params(TargetChain))]
pub struct ChannelTotalIngressed<TargetChain: Chain> {
	pub block_number: <TargetChain as Chain>::ChainBlockNumber,
	pub amount: <TargetChain as Chain>::ChainAmount,
}
impl<TargetChain: Chain> Copy for ChannelTotalIngressed<TargetChain> {}

#[derive(Clone, PartialEq, Eq, Debug, Encode, Decode, TypeInfo, MaxEncodedLen)]
#[scale_info(skip_type_params(TargetChain))]
pub struct OpenChannelDetails<TargetChain: Chain> {
	pub asset: <TargetChain as Chain>::ChainAsset,
	pub close_block: <TargetChain as Chain>::ChainBlockNumber,
}

pub struct DeltaBasedIngress<Sink: IngressSink, Settings> {
	_phantom: core::marker::PhantomData<(Sink, Settings)>,
}
impl<
		Sink: IngressSink + 'static,
		Settings: Parameter + Member + MaybeSerializeDeserialize + Eq,
	> DeltaBasedIngress<Sink, Settings>
where
	<Sink::Chain as Chain>::DepositDetails: Default,
{
	pub fn open_channel<ElectoralAccess: ElectoralWriteAccess<ElectoralSystem = Self>>(
		election_identifiers: Vec<
			ElectionIdentifier<<Self as ElectoralSystem>::ElectionIdentifierExtra>,
		>,
		electoral_access: &mut ElectoralAccess,
		channel: <Sink::Chain as Chain>::ChainAccount,
		asset: <Sink::Chain as Chain>::ChainAsset,
		close_block: <Sink::Chain as Chain>::ChainBlockNumber,
	) -> Result<(), CorruptStorageError> {
		let channel_details = (
			OpenChannelDetails { asset, close_block },
			electoral_access.unsynchronised_state_map(&(channel.clone(), asset))?.unwrap_or(
				ChannelTotalIngressed {
					block_number: Default::default(),
					amount: Default::default(),
				},
			),
		);
		if let Some(election_identifier) = election_identifiers.last() {
			let mut election_access = electoral_access.election_mut(*election_identifier)?;
			let mut channels = election_access.properties()?;
			if channels.len() < MAXIMUM_CHANNELS_PER_ELECTION as usize {
				channels.insert(channel, channel_details);
				election_access.refresh(
					election_identifier
						.extra()
						.checked_add(1)
						.ok_or_else(CorruptStorageError::new)?,
					channels,
				)?;
				return Ok(())
			}
		}

		electoral_access.new_election(
			Default::default(), /* We use the lowest value, so we can refresh the elections the
			                     * maximum number of times */
			[(channel, channel_details)].into_iter().collect(),
			Default::default(),
		)?;

		Ok(())
	}
}
impl<
		Sink: IngressSink + 'static,
		Settings: Parameter + Member + MaybeSerializeDeserialize + Eq,
	> ElectoralSystem for DeltaBasedIngress<Sink, Settings>
where
	<Sink::Chain as Chain>::DepositDetails: Default,
{
	type ElectoralUnsynchronisedState = ();

	// Stores the total ingressed amounts for all channels that have already been dispatched i.e. we
	// told the `IngressEgress` pallet about, and for example, for swap deposit channels, has been
	// scheduled to be swapped.
	type ElectoralUnsynchronisedStateMapKey =
		(<Sink::Chain as Chain>::ChainAccount, <Sink::Chain as Chain>::ChainAsset);
	type ElectoralUnsynchronisedStateMapValue = ChannelTotalIngressed<Sink::Chain>;

	type ElectoralUnsynchronisedSettings = ();
	type ElectoralSettings = Settings;
	type ElectionIdentifierExtra = u32;

	// Stores the channels a given election is witnessing, and a recent total ingressed value.
	type ElectionProperties = BTreeMap<
		<Sink::Chain as Chain>::ChainAccount,
		(OpenChannelDetails<Sink::Chain>, ChannelTotalIngressed<Sink::Chain>),
	>;

	// Stores the any pending total ingressed values that are waiting for
	// the safety margin to pass.
	type ElectionState =
		BTreeMap<<Sink::Chain as Chain>::ChainAccount, ChannelTotalIngressed<Sink::Chain>>;
	type Vote = vote_storage::individual::Individual<
		(),
		vote_storage::individual::identity::Identity<
			BoundedBTreeMap<
				<Sink::Chain as Chain>::ChainAccount,
				ChannelTotalIngressed<Sink::Chain>,
				ConstU32<MAXIMUM_CHANNELS_PER_ELECTION>,
			>,
		>,
	>;
	type Consensus =
		BTreeMap<<Sink::Chain as Chain>::ChainAccount, ChannelTotalIngressed<Sink::Chain>>;
	type OnFinalizeContext = <Sink::Chain as Chain>::ChainBlockNumber;
	type OnFinalizeReturn = ();

	fn is_vote_desired<ElectionAccess: ElectionReadAccess<ElectoralSystem = Self>>(
		_election_identifier_with_extra: ElectionIdentifier<Self::ElectionIdentifierExtra>,
		_election_access: &ElectionAccess,
		_current_vote: Option<(VotePropertiesOf<Self>, AuthorityVoteOf<Self>)>,
	) -> Result<bool, CorruptStorageError> {
		Ok(true)
	}

	fn is_vote_needed(
		(_, current_partial_vote, _): (
			VotePropertiesOf<Self>,
			<Self::Vote as VoteStorage>::PartialVote,
			AuthorityVoteOf<Self>,
		),
		(proposed_partial_vote, _): (
			<Self::Vote as VoteStorage>::PartialVote,
			<Self::Vote as VoteStorage>::Vote,
		),
	) -> bool {
		current_partial_vote != proposed_partial_vote
	}

	fn generate_vote_properties(
		_election_identifier: ElectionIdentifier<Self::ElectionIdentifierExtra>,
		_previous_vote: Option<(VotePropertiesOf<Self>, AuthorityVoteOf<Self>)>,
		_vote: &<Self::Vote as VoteStorage>::PartialVote,
	) -> Result<VotePropertiesOf<Self>, CorruptStorageError> {
		Ok(())
	}

	fn on_finalize<ElectoralAccess: ElectoralWriteAccess<ElectoralSystem = Self>>(
		electoral_access: &mut ElectoralAccess,
		election_identifiers: Vec<ElectionIdentifier<Self::ElectionIdentifierExtra>>,
		chain_tracking: &Self::OnFinalizeContext,
	) -> Result<Self::OnFinalizeReturn, CorruptStorageError> {
		for election_identifier in election_identifiers {
			let (mut channels, mut pending_ingress_totals, option_consensus) = {
				let mut election_access = electoral_access.election_mut(election_identifier)?;
				(
					election_access.properties()?,
					election_access.state()?,
					election_access.check_consensus()?.has_consensus(),
				)
			};

			let mut closed_channels = Vec::new();
			for (account, (details, _)) in &channels {
				let (
					option_ingress_total_before_chain_tracking,
					option_ingress_total_after_chain_tracking,
				) = match option_consensus.as_ref().and_then(|consensus| consensus.get(account)) {
					None => (None, None),
					Some(consensus_ingress_total) => {
						if consensus_ingress_total.block_number <= *chain_tracking {
							(Some(*consensus_ingress_total), None)
						} else {
							match pending_ingress_totals.remove(account) {
								None => (None, Some(*consensus_ingress_total)),
								Some(pending_ingress_total) => {
									if pending_ingress_total.block_number <
										consensus_ingress_total.block_number && pending_ingress_total
										.amount <
										consensus_ingress_total.amount
									{
										if pending_ingress_total.block_number <= *chain_tracking {
											(
												Some(pending_ingress_total),
												Some(*consensus_ingress_total),
											)
										} else {
											(None, Some(pending_ingress_total))
										}
									} else {
										(None, Some(*consensus_ingress_total))
									}
								},
							}
						}
					},
				};

				if let Some(ingress_total) = option_ingress_total_before_chain_tracking {
					let previous_amount = electoral_access
						.unsynchronised_state_map(&(account.clone(), details.asset))?
						.map_or(Default::default(), |previous_total_ingressed| {
							previous_total_ingressed.amount
						});
					match previous_amount.cmp(&ingress_total.amount) {
						Ordering::Less => {
							Sink::on_ingress(
								account.clone(),
								details.asset,
								ingress_total.amount - previous_amount,
								ingress_total.block_number,
								Default::default(),
							);
							electoral_access.set_unsynchronised_state_map(
								(account.clone(), details.asset),
								Some(ingress_total),
							)?;
						},
						Ordering::Greater => {
							Sink::on_ingress_reverted(
								account.clone(),
								details.asset,
								ingress_total.amount - previous_amount,
							);
						},
						Ordering::Equal => (),
					}
					if ingress_total.block_number >= details.close_block {
						Sink::on_channel_closed(account.clone());
						closed_channels.push(account.clone());
					}
				}
				if let Some(ingress_total_after_chain_tracking) =
					option_ingress_total_after_chain_tracking
				{
					pending_ingress_totals
						.insert(account.clone(), ingress_total_after_chain_tracking);
				}
			}

			let mut election_access = electoral_access.election_mut(election_identifier)?;
			if !closed_channels.is_empty() {
				for closed_channel in closed_channels {
					pending_ingress_totals.remove(&closed_channel);
					channels.remove(&closed_channel);
				}

				if channels.is_empty() {
					election_access.delete();
				} else {
					election_access.set_state(pending_ingress_totals)?;
					election_access.refresh(
						// This value is meaningless. We increment as it is required to use a new
						// higher value to refresh the election.
						election_identifier
							.extra()
							.checked_add(1)
							.ok_or_else(CorruptStorageError::new)?,
						channels,
					)?;
				}
			} else {
				election_access.set_state(pending_ingress_totals)?;
			}
		}

		Ok(())
	}

	fn check_consensus<ElectionAccess: ElectionReadAccess<ElectoralSystem = Self>>(
		_election_identifier: ElectionIdentifier<Self::ElectionIdentifierExtra>,
		election_access: &ElectionAccess,
		_previous_consensus: Option<&Self::Consensus>,
		votes: Vec<(VotePropertiesOf<Self>, <Self::Vote as VoteStorage>::Vote)>,
		authorities: AuthorityCount,
	) -> Result<Option<Self::Consensus>, CorruptStorageError> {
		let threshold = success_threshold_from_share_count(authorities) as usize;
		let votes_count = votes.len();
		if votes_count >= threshold {
			let election_channels = election_access.properties()?;

			let mut votes_grouped_by_channel = BTreeMap::<_, Vec<_>>::new();
			for (account, channel_vote) in votes.into_iter().flat_map(|(_properties, vote)| vote) {
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
						channel_votes.resize(votes_count, *recent_ingress_total);
						channel_votes.sort_by_key(|channel_vote| channel_vote.block_number);
						let contributing_channel_votes = longest_increasing_subsequence_by_key(
							&channel_votes[..],
							|channel_vote| channel_vote.amount,
						);
						if contributing_channel_votes.len() >= threshold {
							let new_block_number =
								contributing_channel_votes[threshold - 1].block_number;
							let new_amount = contributing_channel_votes
								[contributing_channel_votes.len() - threshold]
								.amount;
							Some((
								account,
								ChannelTotalIngressed {
									// Requires 2/3 to decrease the block_number,
									block_number: new_block_number,
									// Requires 2/3 to increase the amount
									amount: new_amount,
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

#[cfg(test)]
mod test_delta_based_ingress {
	use super::*;
	use crate::electoral_system::mocks::MockElectoralSystem;
	use cf_chains::mocks::MockEthereum;
	use itertools::Itertools;

	const EXPECTED_ASSET_AMOUNT: u128 = 200;
	const UNEXPECTED_ASSET_AMOUNT: u128 = 100;

	const INITIAL_ASSET_AMOUNT: u128 = 3;

	const EXPECTED_BLOCK_NUMBER: u64 = 2;
	const INITIAL_BLOCK_NUMBER: u64 = 1;

	pub struct MockResink;

	impl IngressSink for MockResink {
		type Chain = MockEthereum;

		fn on_ingress(
			channel: <Self::Chain as Chain>::ChainAccount,
			asset: <Self::Chain as Chain>::ChainAsset,
			amount: <Self::Chain as Chain>::ChainAmount,
			block_number: <Self::Chain as Chain>::ChainBlockNumber,
			details: <Self::Chain as Chain>::DepositDetails,
		) {
			unimplemented!()
		}
		fn on_ingress_reverted(
			channel: <Self::Chain as Chain>::ChainAccount,
			asset: <Self::Chain as Chain>::ChainAsset,
			amount: <Self::Chain as Chain>::ChainAmount,
		) {
			unimplemented!()
		}
		fn on_channel_closed(channel: <Self::Chain as Chain>::ChainAccount) {
			unimplemented!()
		}
	}

	mod helpers {
		use super::*;

		pub fn generate_election_channels(
			amount_of_channels_to_create: u64,
			asset: <MockEthereum as Chain>::ChainAsset,
		) -> BTreeMap<u64, (OpenChannelDetails<MockEthereum>, ChannelTotalIngressed<MockEthereum>)>
		{
			(1..=amount_of_channels_to_create)
				.map(|i| {
					(
						i,
						(
							OpenChannelDetails { asset, close_block: INITIAL_BLOCK_NUMBER },
							ChannelTotalIngressed {
								block_number: INITIAL_BLOCK_NUMBER,
								amount: INITIAL_ASSET_AMOUNT,
							},
						),
					)
				})
				.collect()
		}

		pub fn generate_vote(
			channel_id: u64,
			channel_to_ingressed: ChannelTotalIngressed<MockEthereum>,
		) -> BoundedBTreeMap<u64, ChannelTotalIngressed<MockEthereum>, ConstU32<50>> {
			BoundedBTreeMap::try_from(
				[(channel_id, channel_to_ingressed)].into_iter().collect::<BTreeMap<_, _>>(),
			)
			.unwrap()
		}

		pub fn check_consensus(
			authorities: u64,
			number_of_elections: u64,
			is_voting_factor: u64,
			correct_vote_factor: u64,
			state: BTreeMap<u64, ChannelTotalIngressed<MockEthereum>>,
			consensus_callback: impl Fn(Option<BTreeMap<u64, ChannelTotalIngressed<MockEthereum>>>),
		) {
			let mut electoral_system =
				MockElectoralSystem::<DeltaBasedIngress<MockResink, ()>>::new((), (), ());
			let election_channels = generate_election_channels(3, MockEthereum::GAS_ASSET);
			let mut votes = vec![];

			for (channel_id, _) in election_channels.iter().enumerate() {
				for i in 1..=authorities {
					if i % is_voting_factor == 0 {
						votes.push((
							(),
							generate_vote(
								channel_id.try_into().unwrap(),
								ChannelTotalIngressed {
									block_number: EXPECTED_BLOCK_NUMBER,
									amount: 4u128,
								},
							),
						));
					}
				}
			}

			consensus_callback(
				electoral_system
					.new_election(1, election_channels, state.clone())
					.unwrap()
					.check_consensus(None, votes, authorities.try_into().unwrap())
					.expect("No storage error!"),
			);
		}
	}

	#[test]
	fn consensus_easy_path() {
		const NUMBER_OF_AUTHORITIES: u32 = 10;
		const NUMBER_OF_CHANNELS: u64 = 3;

		let mut electoral_system =
			MockElectoralSystem::<DeltaBasedIngress<MockResink, ()>>::new((), (), ());

		// Setup the channels for an election
		let election_channels =
			helpers::generate_election_channels(NUMBER_OF_CHANNELS, MockEthereum::GAS_ASSET);

		let voted_amounts: Vec<u128> = vec![1, 5, 3, 4, 7, 100, 8, 2, 9, 10];

		// Verify the underlying expectations around the math is correct.
		assert_eq!(
			longest_increasing_subsequence_by_key(&voted_amounts, |x| *x),
			vec![1, 3, 4, 7, 8, 9, 10]
		);

		let mut votes = vec![];

		for channel_id in 1..=NUMBER_OF_CHANNELS {
			for amount in voted_amounts.iter() {
				votes.push((
					(),
					helpers::generate_vote(
						channel_id,
						ChannelTotalIngressed {
							block_number: EXPECTED_BLOCK_NUMBER,
							amount: *amount,
						},
					),
				));
			}
		}

		let empty_state: BTreeMap<u64, ChannelTotalIngressed<MockEthereum>> = BTreeMap::new();

		let consensus = electoral_system
			.new_election(1, election_channels.clone(), empty_state.clone())
			.unwrap()
			.check_consensus(None, votes, NUMBER_OF_AUTHORITIES)
			.expect("No storage error!");

		let consensus = consensus.expect("To have consensus!");

		let expected_state: BTreeMap<u64, ChannelTotalIngressed<MockEthereum>> = election_channels
			.iter()
			.map(|(channel_id, (_, channel_total_ingressed))| {
				(*channel_id, *channel_total_ingressed)
			})
			.collect();

		assert_eq!(consensus, empty_state);
	}

	#[test]
	fn consensus_with_majority() {
		use helpers::check_consensus;

		const NUMBER_OF_AUTHORITIES: u64 = 10;
		const NUMBER_OF_ELECTIONS: u64 = 3;
		// Every authority is voting correct.
		const CORRECT_VOTE_FACTOR: u64 = 1;
		// Every authority gives a vote.
		const IS_VOTING_FACTOR: u64 = 1;

		let expected_state: BTreeMap<u64, ChannelTotalIngressed<MockEthereum>> =
			BTreeMap::from_iter((1..=NUMBER_OF_ELECTIONS).map(|i| {
				(i, ChannelTotalIngressed { block_number: i, amount: EXPECTED_ASSET_AMOUNT })
			}));

		// Dosent matter for the consensus function.
		let empty_state: BTreeMap<u64, ChannelTotalIngressed<MockEthereum>> = BTreeMap::new();

		// 2/3 of the authorities vote for the expected amount, the rest vote for the unexpected
		// amount -> consensus should be the expected amount
		helpers::check_consensus(
			NUMBER_OF_AUTHORITIES,
			NUMBER_OF_ELECTIONS,
			IS_VOTING_FACTOR,
			CORRECT_VOTE_FACTOR,
			empty_state,
			|state| {
				assert_eq!(state, Some(expected_state.clone()));
			},
		);
	}

	#[test]
	fn no_consensus_threshold_not_reached() {
		use helpers::check_consensus;

		const NUMBER_OF_AUTHORITIES: u64 = 10;
		const NUMBER_OF_ELECTIONS: u64 = 3;
		// Every second authority votes for the unexpected amount.
		const FALSE_VOTE_FACTOR: u64 = 2;
		// Every third authority gives a vote.
		const IS_VOTING_FACTOR: u64 = 3;

		let expected_state: BTreeMap<u64, ChannelTotalIngressed<MockEthereum>> =
			BTreeMap::from_iter((1..=NUMBER_OF_ELECTIONS).map(|i| {
				(i, ChannelTotalIngressed { block_number: i, amount: EXPECTED_ASSET_AMOUNT })
			}));
		// 2/3 of the authorities vote for the expected amount, the rest vote for the unexpected
		// amount -> consensus should be the expected amount
		helpers::check_consensus(
			NUMBER_OF_AUTHORITIES,
			NUMBER_OF_ELECTIONS,
			IS_VOTING_FACTOR,
			FALSE_VOTE_FACTOR,
			expected_state.clone(),
			|state| {
				assert_eq!(state, None);
			},
		);
	}

	#[test]
	fn on_finalize() {
		// TODO: Write tests for `on_finalize`
	}

	#[test]
	fn contributing_channels() {
		assert_eq!(
			longest_increasing_subsequence_by_key(&[1, 5, 3, 6, 10, 8, 9, 14, 2, 3], |x| *x),
			vec![1, 3, 6, 10, 14]
		);
	}
}
