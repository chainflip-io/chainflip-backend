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

use std::collections::BTreeMap;

use cf_utilities::task_scope::{self};
use engine_sc_client::{StateChainClient, base_rpc_api::BaseRpcApi, storage_api::StorageApi};
use futures::StreamExt;
use futures_util::FutureExt;
use pallet_cf_elections::{
	ElectionIdentifier, ElectoralSystemTypes, SharedDataHash, vote_storage::VoteStorage,
};
use state_chain_runtime::{
	BitcoinInstance, Runtime, SolanaInstance,
	chainflip::witnessing::{
		bitcoin_elections::BitcoinElectoralSystemRunner,
		solana_elections::SolanaElectoralSystemRunner,
	},
};
use std::env;

type BitcoinVoteStorageTuple = <BitcoinElectoralSystemRunner as ElectoralSystemTypes>::VoteStorage;

type SolanaVoteStorageTuple = <SolanaElectoralSystemRunner as ElectoralSystemTypes>::VoteStorage;

#[tokio::main(flavor = "multi_thread", worker_threads = 3)]
async fn main() {
	// http://localhost:9944
	let rpc_url = env::var("CF_RPC_NODE").unwrap_or("ws://localhost:9944".into());

	observe_elections(rpc_url).await;
}

async fn observe_elections(rpc_url: String) {
	task_scope::task_scope(|scope| async move {

		let (finalized_stream, _unfinalized_stream, client) = StateChainClient::connect_without_account(scope, &rpc_url).await.unwrap();
		finalized_stream.for_each(|block_info| {
			let client_copy = client.clone();
			async move {
				println!("Block {}", block_info.number);
				let last_block = client_copy.base_rpc_client.block_hash(block_info.number - 1).await.expect("Shouldn't fail").unwrap();

				let mut bitcoin_shared_data_map = client_copy.storage_map::<pallet_cf_elections::SharedData::<Runtime, BitcoinInstance>, BTreeMap<_,_>>(block_info.hash).await.expect("Should always exist");
				bitcoin_shared_data_map.extend(
					client_copy.storage_map::<pallet_cf_elections::SharedData::<Runtime, BitcoinInstance>, BTreeMap<_,_>>(last_block).await.expect("Should always exist")
				);
				let mut solana_shared_data_map = client_copy.storage_map::<pallet_cf_elections::SharedData::<Runtime, SolanaInstance>, BTreeMap<_,_>>(block_info.hash).await.expect("Should always exist");
				solana_shared_data_map.extend(
					client_copy.storage_map::<pallet_cf_elections::SharedData::<Runtime, SolanaInstance>, BTreeMap<_,_>>(last_block).await.expect("Should always exist")
				);

				let mut bitcoin_map: BTreeMap<ElectionIdentifier<<BitcoinElectoralSystemRunner as ElectoralSystemTypes>::ElectionIdentifierExtra>, BTreeMap<<BitcoinVoteStorageTuple as VoteStorage>::PartialVote, u32>> = BTreeMap::<_,BTreeMap<<BitcoinVoteStorageTuple as VoteStorage>::PartialVote, _>>::new();
				let mut btc_partial_to_vote: BTreeMap<<BitcoinVoteStorageTuple as VoteStorage>::PartialVote, <BitcoinVoteStorageTuple as VoteStorage>::Vote> = BTreeMap::new();

				let mut solana_map: BTreeMap<ElectionIdentifier<<SolanaElectoralSystemRunner as ElectoralSystemTypes>::ElectionIdentifierExtra>, BTreeMap<<SolanaVoteStorageTuple as VoteStorage>::PartialVote, u32>> = BTreeMap::<_,BTreeMap<<SolanaVoteStorageTuple as VoteStorage>::PartialVote, _>>::new();
				let mut sol_partial_to_vote: BTreeMap<<SolanaVoteStorageTuple as VoteStorage>::PartialVote, <SolanaVoteStorageTuple as VoteStorage>::Vote> = BTreeMap::new();
				let signed_block = client_copy.base_rpc_client.block(block_info.hash).await.unwrap();
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
														votes.entry(partial.clone()).and_modify(|entry| *entry += 1).or_insert(1);
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
														votes.entry(partial.clone()).and_modify(|entry| *entry += 1).or_insert(1);
													} else {
														let mut vote_map = BTreeMap::new();
														vote_map.insert(partial.clone(), 1);
														solana_map.insert(election_id, vote_map);
													}
												},
											}
										}
									},
									pallet_cf_elections::Call::provide_shared_data { shared_data } => {
										let shared_data_hash = SharedDataHash::of(&shared_data);
										println!("Solana Providing shared data for {:?}: {:?}", shared_data_hash, shared_data);
									}
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
														votes.entry(partial.clone()).and_modify(|entry| *entry += 1).or_insert(1);
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
														votes.entry(partial.clone()).and_modify(|entry| *entry += 1).or_insert(1);
													} else {
														let mut vote_map = BTreeMap::new();
														vote_map.insert(partial.clone(), 1);
														bitcoin_map.insert(election_id, vote_map);
													}
												},
											}
										}
									},
									pallet_cf_elections::Call::provide_shared_data { shared_data } => {
										let shared_data_hash = SharedDataHash::of(&shared_data);
										println!("Bitcoin Providing shared data for {:?}: {:?}", shared_data_hash, shared_data);
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
							if let Some(full_vote) = btc_partial_to_vote.get(&partial){
								println!("	{:?}: {}", full_vote, count);
							} else {
								match partial {
									pallet_cf_elections::vote_storage::composite::tuple_6_impls::CompositePartialVote::A(inner) |
									pallet_cf_elections::vote_storage::composite::tuple_6_impls::CompositePartialVote::B(inner) |
									pallet_cf_elections::vote_storage::composite::tuple_6_impls::CompositePartialVote::C(inner) |
									pallet_cf_elections::vote_storage::composite::tuple_6_impls::CompositePartialVote::D(inner) => {
										let result = bitcoin_shared_data_map.get(&inner);
										println!("	{:?}({:?}): {}", result, partial, count);
									},
									// Partial vote == Full vote
									pallet_cf_elections::vote_storage::composite::tuple_6_impls::CompositePartialVote::EE(inner) => {
										println!("	{:?}: {}", inner, count);
									},
									// Partial vote == Full vote
									pallet_cf_elections::vote_storage::composite::tuple_6_impls::CompositePartialVote::FF(inner) => {
										println!("	{:?}: {}", inner, count);
									},
								};
							}
						}
					}
					for (key, vote_map) in solana_map {
						println!("  SOLANA ELECTION {:?}", key);
						for (partial, count) in vote_map {
							if let Some(full_vote) = sol_partial_to_vote.get(&partial) {
								println!("	{:?}: {}", full_vote, count);
							} else {
								match partial {
									// Partial vote == Full vote
									pallet_cf_elections::vote_storage::composite::tuple_7_impls::CompositePartialVote::A(inner) => {
										println!("	{:?}: {}", inner, count);
									},
									// Partial vote == Full vote
									pallet_cf_elections::vote_storage::composite::tuple_7_impls::CompositePartialVote::B(inner) => {
										println!("	{:?}: {}", inner, count);
									},
									pallet_cf_elections::vote_storage::composite::tuple_7_impls::CompositePartialVote::C(inner) => {
										let result = solana_shared_data_map.get(&inner.value);
										println!("	{:?} Slot {:?}: {}", result , inner.block, count);
									},
									// Partial vote == Full vote
									pallet_cf_elections::vote_storage::composite::tuple_7_impls::CompositePartialVote::D(inner) => {
										println!("	{:?}: {}", inner, count);
									}
									// Partial vote == Full vote
									pallet_cf_elections::vote_storage::composite::tuple_7_impls::CompositePartialVote::EE(inner) => {
										println!("	{:?}: {}", inner, count);
									},
									pallet_cf_elections::vote_storage::composite::tuple_7_impls::CompositePartialVote::FF(inner) |
									pallet_cf_elections::vote_storage::composite::tuple_7_impls::CompositePartialVote::G(inner) => {
										let result = solana_shared_data_map.get(&inner);
										println!("	{:?}({:?}): {}", result, partial, count);
									},

								}
							}
						}
					}
					println!();
				}
			}
		}).await;

		Ok(())

	 }.boxed()).await.unwrap()
}
