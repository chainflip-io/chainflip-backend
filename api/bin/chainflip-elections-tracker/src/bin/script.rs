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

//! The chainflip elections tracker.

use std::collections::{BTreeMap};

use cf_utilities::task_scope::{self};
use chainflip_engine::state_chain_observer::client::{
	StateChainClient, base_rpc_api::BaseRpcApi,
};
use futures::StreamExt;
use futures_util::FutureExt;
use pallet_cf_elections::{
	ElectionIdentifier, ElectoralSystemTypes, SharedDataHash,
	vote_storage::{VoteStorage},
};
use state_chain_runtime::{
	chainflip::{
		bitcoin_elections::BitcoinElectoralSystemRunner,
		solana_elections::SolanaElectoralSystemRunner,
	},
};
use std::env;

type BitcoinVoteStorageTuple = <BitcoinElectoralSystemRunner as ElectoralSystemTypes>::VoteStorage;

type SolanaVoteStorageTuple = <SolanaElectoralSystemRunner as ElectoralSystemTypes>::VoteStorage;

#[tokio::main(flavor = "multi_thread", worker_threads = 3)]
async fn main() {
	// get env vars
	// wss://archive.sisyphos.chainflip.io
	// wss://archive.perseverance.chainflip.io
	// http://localhost:9944
	let rpc_url = env::var("CF_RPC_NODE").unwrap_or("wss://archive.sisyphos.chainflip.io".into());

	observe_elections(rpc_url).await;
}

async fn observe_elections(rpc_url: String) {
	task_scope::task_scope(|scope| async move {

		let (finalized_stream, _unfinalized_stream, client) = StateChainClient::connect_without_account(scope, &rpc_url).await.unwrap();
		finalized_stream.for_each(|block| { 
			let client_copy = client.clone();
			async move {
				println!("Block {}", block.number);
				let mut bitcoin_map: BTreeMap<ElectionIdentifier<<BitcoinElectoralSystemRunner as ElectoralSystemTypes>::ElectionIdentifierExtra>, BTreeMap<<BitcoinVoteStorageTuple as VoteStorage>::PartialVote, u32>> = BTreeMap::<_,BTreeMap<<BitcoinVoteStorageTuple as VoteStorage>::PartialVote, _>>::new();
				let mut btc_partial_to_vote: BTreeMap<<BitcoinVoteStorageTuple as VoteStorage>::PartialVote, <BitcoinVoteStorageTuple as VoteStorage>::Vote> = BTreeMap::new();

				let mut solana_map: BTreeMap<ElectionIdentifier<<SolanaElectoralSystemRunner as ElectoralSystemTypes>::ElectionIdentifierExtra>, BTreeMap<<SolanaVoteStorageTuple as VoteStorage>::PartialVote, u32>> = BTreeMap::<_,BTreeMap<<SolanaVoteStorageTuple as VoteStorage>::PartialVote, _>>::new();
				let mut sol_partial_to_vote: BTreeMap<<SolanaVoteStorageTuple as VoteStorage>::PartialVote, <SolanaVoteStorageTuple as VoteStorage>::Vote> = BTreeMap::new();
				let signed_block = client_copy.base_rpc_client.block(block.hash).await.unwrap();
				if let Some(block) = signed_block {
					let extrinsics = block.block.extrinsics;
					for ex in extrinsics {
						match ex.function {
							state_chain_runtime::RuntimeCall::SolanaElections(call) => {
								match call {
									pallet_cf_elections::Call::vote { authority_votes } => {
										for (election_id, vote) in *authority_votes {
											match vote {
												pallet_cf_elections::vote_storage::AuthorityVote::PartialVote(partial) => {
													if let Some(votes) = solana_map.get_mut(&election_id) {
														votes.entry(partial.clone()).and_modify(|entry| *entry = *entry + 1).or_insert(1);
													} else {
														let mut vote_map = BTreeMap::new();
														vote_map.insert(partial.clone(), 1);
														solana_map.insert(election_id, vote_map);
													}
												},
												pallet_cf_elections::vote_storage::AuthorityVote::Vote(full_vote) => {
													let partial = <SolanaVoteStorageTuple as VoteStorage>::vote_into_partial_vote(&full_vote, |shared_data| SharedDataHash::of(&shared_data));
													sol_partial_to_vote.insert(partial.clone(), full_vote.clone());
													if let Some(votes) = solana_map.get_mut(&election_id) {
														votes.entry(partial.clone()).and_modify(|entry| *entry = *entry + 1).or_insert(1);
													} else {
														let mut vote_map = BTreeMap::new();
														vote_map.insert(partial.clone(), 1);
														solana_map.insert(election_id, vote_map);
													}
												},
											}
										}
									},
									_ => {},
								}
							},
							state_chain_runtime::RuntimeCall::BitcoinElections(call) => {
								match call {
									pallet_cf_elections::Call::vote { authority_votes } => {
										for (election_id, vote) in *authority_votes {
											match vote {
												pallet_cf_elections::vote_storage::AuthorityVote::PartialVote(partial) => {
													if let Some(votes) = bitcoin_map.get_mut(&election_id) {
														votes.entry(partial.clone()).and_modify(|entry| *entry = *entry + 1).or_insert(1);
													} else {
														let mut vote_map = BTreeMap::new();
														vote_map.insert(partial.clone(), 1);
														bitcoin_map.insert(election_id, vote_map);
													}
												},
												pallet_cf_elections::vote_storage::AuthorityVote::Vote(full_vote) => {
													let partial = <BitcoinVoteStorageTuple as VoteStorage>::vote_into_partial_vote(&full_vote, |shared_data| SharedDataHash::of(&shared_data));
													btc_partial_to_vote.insert(partial.clone(), full_vote.clone());
													if let Some(votes) = bitcoin_map.get_mut(&election_id) {
														votes.entry(partial.clone()).and_modify(|entry| *entry = *entry + 1).or_insert(1);
													} else {
														let mut vote_map = BTreeMap::new();
														vote_map.insert(partial.clone(), 1);
														bitcoin_map.insert(election_id, vote_map);
													}
												},
											}
										}
									},
									_ => {},
								}
							},
							_ => {},
						}
					}

					println!("VOTE SUMMARY block {:?}:", block.block.header.number);
					for (key, vote_map) in bitcoin_map {
						println!("  BITCOIN ELECTION {:?}", key);
						for (partial, count) in vote_map {
							let full_vote = btc_partial_to_vote.get(&partial);
							println!("	{:?}: {}", full_vote, count);
						}
					}
					for (key, vote_map) in solana_map {
						println!("  SOLANA ELECTION {:?}", key);
						for (partial, count) in vote_map {
							let full_vote = sol_partial_to_vote.get(&partial);
							println!("	{:?}: {}", full_vote, count);
						}
					}
					println!("");

				}
			}
		}).await;

		Ok(())

	 }.boxed()).await.unwrap()
}
