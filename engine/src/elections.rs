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

pub mod voter_api;

use engine_sc_client::{
	chain_api::ChainApi,
	electoral_api::ElectoralApi,
	extrinsic_api::signed::{SignedExtrinsicApi, UntilInBlock},
};

use crate::retrier::{RequestLog, RetrierClient, MAX_RPC_RETRY_DELAY};
use anyhow::anyhow;
use cf_primitives::MILLISECONDS_PER_BLOCK;
use cf_utilities::{future_map::FutureMap, task_scope::Scope, UnendingStream};
use futures::{stream, StreamExt, TryStreamExt};
use pallet_cf_elections::{
	vote_storage::{AuthorityVote, VoteStorage},
	ElectionIdentifierOf, ElectoralSystemTypes, PartialVoteOf, SharedDataHash, VoteOf,
	VoteStorageOf, MAXIMUM_VOTES_PER_EXTRINSIC,
};
use rand::Rng;
use std::{
	collections::{BTreeMap, HashMap},
	sync::Arc,
};
use tokio::sync::mpsc;
use tracing::Level;
use voter_api::CompositeVoterApi;

const MAXIMUM_CONCURRENT_FILTER_REQUESTS: usize = 16;
const LIFETIME_OF_SHARED_DATA_IN_CACHE: std::time::Duration = std::time::Duration::from_secs(90);
const MAXIMUM_SHARED_DATA_CACHE_ITEMS: usize = 1024;
const MAXIMUM_CONCURRENT_VOTER_REQUESTS: u32 = 32;
const INITIAL_VOTER_REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

pub struct Voter<
	Instance: 'static,
	StateChainClient: ElectoralApi<Instance> + SignedExtrinsicApi + ChainApi,
	VoterClient: CompositeVoterApi<<state_chain_runtime::Runtime as pallet_cf_elections::Config<Instance>>::ElectoralSystemRunner> + Send + Sync + 'static,
> where
	state_chain_runtime::Runtime:
		pallet_cf_elections::Config<Instance>,
{
	state_chain_client: Arc<StateChainClient>,
	voter: RetrierClient<VoterClient>,
	voter_name: &'static str,
	cache_invalidation_senders: Option<Vec<mpsc::Sender<()>>>,
	_phantom: core::marker::PhantomData<Instance>,
}

impl<
		Instance: Send + Sync + 'static,
		StateChainClient: ElectoralApi<Instance> + SignedExtrinsicApi + ChainApi,
		VoterClient: CompositeVoterApi<<state_chain_runtime::Runtime as pallet_cf_elections::Config<Instance>>::ElectoralSystemRunner> + Clone + Send + Sync + 'static,
	> Voter<Instance, StateChainClient, VoterClient>
where
	state_chain_runtime::Runtime:
		pallet_cf_elections::Config<Instance>,
	pallet_cf_elections::Call<state_chain_runtime::Runtime, Instance>:
		std::convert::Into<state_chain_runtime::RuntimeCall>,
{
	pub fn new(
		scope: &Scope<'_, anyhow::Error>,
		state_chain_client: Arc<StateChainClient>,
		voter: VoterClient,
		cache_invalidation_senders: Option<Vec<mpsc::Sender<()>>>,
		voter_name: &'static str,
	) -> Self {
		Self {
			state_chain_client,
			voter: RetrierClient::new(
				scope,
				voter_name,
				futures::future::ready(voter),
				None,
				INITIAL_VOTER_REQUEST_TIMEOUT,
				MAX_RPC_RETRY_DELAY,
				MAXIMUM_CONCURRENT_VOTER_REQUESTS,
			),
			voter_name,
			cache_invalidation_senders,
			_phantom: Default::default(),
		}
	}

	pub fn log(&self, level: Level, message: &str) {
		match level {
			Level::ERROR => tracing::error!(voter = self.voter_name, "{}", message),
			Level::WARN => tracing::warn!(voter = self.voter_name, "{}", message),
			Level::INFO => tracing::info!(voter = self.voter_name, "{}", message),
			Level::DEBUG => tracing::debug!(voter = self.voter_name, "{}", message),
			Level::TRACE => tracing::trace!(voter = self.voter_name, "{}", message),
		}
	}

	pub async fn continuously_vote(self) {
		loop {
			self.log(Level::INFO, "Beginning voting");
			if let Err(error) = self.reset_and_continuously_vote().await {
				self.log(Level::ERROR, &format!("Voting reset due to error: '{}'", error));
			}
		}
	}

	#[tracing::instrument(name = "voter-task", skip(self))]
	async fn reset_and_continuously_vote(&self) -> Result<(), anyhow::Error> {
		let mut rng = rand::rngs::OsRng;
		let latest_unfinalized_block = self.state_chain_client.latest_unfinalized_block();
		if let Some(_electoral_data) = self.state_chain_client.electoral_data(latest_unfinalized_block).await {
			let extrinsic_data = self.state_chain_client.submit_signed_extrinsic(pallet_cf_elections::Call::<state_chain_runtime::Runtime, Instance>::ignore_my_votes {}).await.until_in_block().await?;

			if let Some(electoral_data) = self.state_chain_client.electoral_data(extrinsic_data.header.into()).await {
				stream::iter(electoral_data.current_elections).map(|(election_identifier, election_data)| {
					let state_chain_client = &self.state_chain_client;
					async move {
						if election_data.option_existing_vote.is_some() {
							state_chain_client.finalize_signed_extrinsic(pallet_cf_elections::Call::<state_chain_runtime::Runtime, Instance>::delete_vote {
								election_identifier,
							}).await.until_in_block().await?;
						}
						Ok::<_, anyhow::Error>(())
					}
				}).buffer_unordered(32).try_collect::<Vec<_>>().await?;

				self.state_chain_client.submit_signed_extrinsic(pallet_cf_elections::Call::<state_chain_runtime::Runtime, Instance>::stop_ignoring_my_votes {}).await.until_in_block().await?;
			}
		}

		let mut unfinalized_block_stream = self.state_chain_client.unfinalized_block_stream().await;
		const BLOCK_TIME: std::time::Duration =
			std::time::Duration::from_millis(MILLISECONDS_PER_BLOCK);
		let mut submit_interval = tokio::time::interval(BLOCK_TIME);
		let mut pending_submissions = BTreeMap::<
			ElectionIdentifierOf<<state_chain_runtime::Runtime as pallet_cf_elections::Config<Instance>>::ElectoralSystemRunner>,
			(
				PartialVoteOf<<state_chain_runtime::Runtime as pallet_cf_elections::Config<Instance>>::ElectoralSystemRunner>,
				VoteOf<<state_chain_runtime::Runtime as pallet_cf_elections::Config<Instance>>::ElectoralSystemRunner>,
			)
		>::default();
		let mut vote_tasks = FutureMap::default();
		let mut shared_data_cache = HashMap::<
			SharedDataHash,
			(
				<<<state_chain_runtime::Runtime as pallet_cf_elections::Config<Instance>>::ElectoralSystemRunner as ElectoralSystemTypes>::VoteStorage as VoteStorage>::SharedData,
				std::time::Instant,
			)
		>::default();

		let mut authority_count = 1;
		cf_utilities::loop_select! {
			let _ = submit_interval.tick() => {
				stream::iter(core::mem::take(&mut pending_submissions).into_iter()).chunks(MAXIMUM_VOTES_PER_EXTRINSIC as usize /*We use the same constant as if it is reasonable for the extrinsic maximum this should also be reasonable for the RPC maximum*/).map(|votes| {
					let state_chain_client = &self.state_chain_client;
					async move {
						let votes = BTreeMap::from_iter(votes);
						let filtered_votes = state_chain_client.filter_votes(votes.iter().map(|(election_identifier, (_partial_vote, vote))| (*election_identifier, vote.clone())).collect()).await;
						(votes, filtered_votes)
					}
				}).buffer_unordered(MAXIMUM_CONCURRENT_FILTER_REQUESTS).flat_map(|(mut votes, filtered_votes)| {
					let submit_full_vote = rng.gen_bool(required_full_vote_probability(authority_count, 0.0025));

					stream::iter(filtered_votes.into_iter().filter_map(move |election_identifier| {
						votes.remove(&election_identifier).map(|(partial_vote, vote)| {
							(
								election_identifier,
								if submit_full_vote {
									AuthorityVote::Vote(vote)
								} else {
									AuthorityVote::PartialVote(partial_vote)
								}
							)
						})
					}))
				}).chunks(MAXIMUM_VOTES_PER_EXTRINSIC as usize).for_each_concurrent(None, |votes| {
					let state_chain_client = &self.state_chain_client;
					async move {
						for (election_identifier, _) in votes.iter() {
							self.log(Level::DEBUG, &format!("Submitting vote for election: '{:?}'", election_identifier));
						}
						// TODO: Use block hash you got this vote tasks details from as the based of the mortal of the extrinsic
						state_chain_client.submit_signed_extrinsic(pallet_cf_elections::Call::<state_chain_runtime::Runtime, Instance>::vote {
							authority_votes: Box::new(BTreeMap::from_iter(votes).try_into().unwrap(/*Safe due to chunking*/)),
						}).await;
					}
				}).await;
			},
			let (election_identifier, result_vote) = vote_tasks.next_or_pending() => {
				match result_vote {
					Ok(Some(vote)) => {
						self.log(Level::DEBUG, &format!("Voting task for election: '{:?}' succeeded.", election_identifier));
						// Create the partial_vote early so that SharedData can be provided as soon as the vote has been generated, rather than only after it is submitted.
						let partial_vote = VoteStorageOf::<<state_chain_runtime::Runtime as pallet_cf_elections::Config<Instance>>::ElectoralSystemRunner>::vote_into_partial_vote(&vote, |shared_data| {
							let shared_data_hash = SharedDataHash::of(&shared_data);
							if shared_data_cache.len() > MAXIMUM_SHARED_DATA_CACHE_ITEMS {
								for shared_data_hash in shared_data_cache.keys().cloned().take(shared_data_cache.len() - MAXIMUM_SHARED_DATA_CACHE_ITEMS).collect::<Vec<_>>() {
									shared_data_cache.remove(&shared_data_hash);
								}
							}
							shared_data_cache.insert(shared_data_hash, (shared_data, std::time::Instant::now()));
							shared_data_hash
						});

						pending_submissions.insert(election_identifier,	(partial_vote, vote));
					},
					Ok(None) => {
						self.log(Level::DEBUG, &format!("Voting task for election '{:?}' returned 'None' (nothing to submit).", election_identifier));
					},
					Err(error) => {
						self.log(Level::WARN, &format!("Voting task for election '{:?}' failed with error: '{:?}'.", election_identifier, error));
					}
				}
			},
			if let Some(block_info) = unfinalized_block_stream.next() => {
				// Give vote tasks some time to run, and then batch the finished ones, ideally submitting them early enough to be included in the next block.
				submit_interval.reset_after(BLOCK_TIME.mul_f32(0.5)); // TODO: Allow this to be configured in the pallet. But bound between 0 and BLOCK_TIME.

				// Only filtering per-block means the cache can have SharedData in it older than LIFETIME_OF_SHARED_DATA_IN_CACHE and therefore cache size could build up. But as blocks are the only way SharedData can enter the cache this is reasonable.
				shared_data_cache.retain(|_shared_data_hash, (_shared_data, added_to_cache)| {
					added_to_cache.elapsed() < LIFETIME_OF_SHARED_DATA_IN_CACHE
				});

				if let Some(electoral_data) = self.state_chain_client.electoral_data(block_info).await {
					authority_count = core::cmp::max(electoral_data.authority_count, 1);
					if electoral_data.contributing {
						if let Some(caches) = &self.cache_invalidation_senders {
							for sender in caches {
								if let Err(e) = sender.send(()).await {
									self.log(Level::WARN, &format!("Cache receiver dropped: {e}"))
								}
							}
						}
						for (election_identifier, election_data) in electoral_data.current_elections {
							if election_data.is_vote_desired {
								if !vote_tasks.contains_key(&election_identifier) {
									self.log(Level::DEBUG, &format!("Voting task for election: '{:?}' initiated.", election_identifier));
									vote_tasks.insert(
										election_identifier,
										Box::pin(self.voter.request_with_limit(
											RequestLog::new(format!("{} | {}", self.voter_name, "vote"), Some(format!("{election_identifier:?}"))),
											Box::pin(move |client| {
												let election_data = election_data.clone();
												Box::pin(async move {
													client.vote(
														election_data.settings,
														election_data.properties,
													).await
												})
											}),
											3,
										))
									);
								} else {
									self.log(Level::DEBUG, &format!("Voting task for election: '{:?}' not initiated as a task is already running for that election.", election_identifier));
								}
							}
						}

						for (unprovided_shared_data_hash, reference_details) in electoral_data.unprovided_shared_data_hashes {
							if let Some((shared_data, _)) = shared_data_cache.get(&unprovided_shared_data_hash) {
								// We hit this branch *only if* no authority provided the full vote hence we can use a higher falure rate here (i.e. 1%)
								// The probability of no authority submitting the shared data is then:
								// probability of no authority submitting full vote * (probability of no authority submitting the shared data)^number of times we retry to submit the shared data
								// which keeps decreasing exponentially (i.e. with the current values => 2 blocks delay: 1 in 40k, 3 blocks delay: 1 in 4million ...)
								if (reference_details.created..reference_details.expires).contains(&block_info.number) && rng.gen_bool(required_full_vote_probability(authority_count, 0.01)) {
									self.state_chain_client.submit_signed_extrinsic(pallet_cf_elections::Call::<state_chain_runtime::Runtime, Instance>::provide_shared_data {
										shared_data: Box::new(shared_data.clone()),
									}).await;
								}
							}
						}
					} else {
						// We expect this to happen when a validator joins the set, since they won't be contributing, but will be a validator.
						// Therefore they get Some() from `electoral_data` but `contributing` is false, until we reset the voting by throwing an error here.
						return Err(anyhow!("{} | Validator has just joined the authority set, or has been unexpectedly set as not contributing.", self.voter_name));
					}
				} else {
					self.log(Level::INFO, "Not voting as not an authority.");
				}
			} else break Ok(()),
		}
	}
}

/// This function calculates the probability of submitting a full-vote
/// given the number of authority in the active set and the acceptable failure_rate.
/// Failure rate is defined as the probability of no authority submitting a full-vote hence
/// delaying consensus by 1 block. p is the probability of submitting a full-vote
///
/// Given that (1 - p)^authority_count = failure_rate
/// 1 - p = failure_rate^(1 / authority_count)
/// p = 1 - failure_rate^(1 / authority_count)
fn required_full_vote_probability(authority_count: u32, failure_rate: f64) -> f64 {
	1.0 - failure_rate.powf(1.0 / authority_count as f64)
}
