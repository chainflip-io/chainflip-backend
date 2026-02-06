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
use chainflip_engine::state_chain_observer::client::{
	StateChainClient, base_rpc_api::BaseRpcApi, storage_api::StorageApi,
};
use futures::StreamExt;
use futures_util::FutureExt;
use pallet_cf_elections::{
	ElectionIdentifier, ElectoralSystemTypes, SharedDataHash, UniqueMonotonicIdentifier,
	bitmap_components::ElectionBitmapComponents, vote_storage::VoteStorage,
};
use state_chain_runtime::{
	BitcoinInstance, Runtime, SolanaInstance,
	chainflip::{
		bitcoin_elections::BitcoinElectoralSystemRunner,
		solana_elections::SolanaElectoralSystemRunner,
	},
};
use std::env;

type BitcoinVoteStorageTuple = <BitcoinElectoralSystemRunner as ElectoralSystemTypes>::VoteStorage;
type BitcoinElectionIdentifierExtra =
	<BitcoinElectoralSystemRunner as ElectoralSystemTypes>::ElectionIdentifierExtra;
type BitcoinElectionProperties =
	<BitcoinElectoralSystemRunner as ElectoralSystemTypes>::ElectionProperties;
type BitcoinBitmapComponent = <BitcoinVoteStorageTuple as VoteStorage>::BitmapComponent;

type SolanaVoteStorageTuple = <SolanaElectoralSystemRunner as ElectoralSystemTypes>::VoteStorage;
type SolanaElectionIdentifierExtra =
	<SolanaElectoralSystemRunner as ElectoralSystemTypes>::ElectionIdentifierExtra;
type SolanaElectionProperties =
	<SolanaElectoralSystemRunner as ElectoralSystemTypes>::ElectionProperties;
type SolanaBitmapComponent = <SolanaVoteStorageTuple as VoteStorage>::BitmapComponent;
type BitcoinIndividualComponent = <BitcoinVoteStorageTuple as VoteStorage>::IndividualComponent;
type SolanaIndividualComponent = <SolanaVoteStorageTuple as VoteStorage>::IndividualComponent;

// Type aliases for the ElectionBitmapComponents types
type BitcoinElectionBitmapComponents = ElectionBitmapComponents<Runtime, BitcoinInstance>;
type SolanaElectionBitmapComponents = ElectionBitmapComponents<Runtime, SolanaInstance>;

#[tokio::main(flavor = "multi_thread", worker_threads = 3)]
async fn main() {
	// http://localhost:9944
	let rpc_url = env::var("CF_RPC_NODE").unwrap_or("wss://mainnet-archive.chainflip.io".into());

	observe_elections(rpc_url).await;
}

async fn observe_elections(rpc_url: String) {
	task_scope::task_scope(|scope| async move {

		let (finalized_stream, _unfinalized_stream, client) = StateChainClient::connect_without_account(scope, &rpc_url).await.unwrap();
		finalized_stream.for_each(|block_info| {
			let client_copy = client.clone();
			async move {
				println!("\n{:=^80}", format!(" Block {} ", block_info.number));
				let last_block_hash = client_copy.base_rpc_client.block_hash(block_info.number - 1).await.expect("Shouldn't fail").unwrap();

				// ===== Query SharedData (from both X and X-1 for resolving hashes) =====
				let mut bitcoin_shared_data_map = client_copy.storage_map::<pallet_cf_elections::SharedData::<Runtime, BitcoinInstance>, BTreeMap<_,_>>(block_info.hash).await.expect("Should always exist");
				bitcoin_shared_data_map.extend(
					client_copy.storage_map::<pallet_cf_elections::SharedData::<Runtime, BitcoinInstance>, BTreeMap<_,_>>(last_block_hash).await.expect("Should always exist")
				);
				let mut solana_shared_data_map = client_copy.storage_map::<pallet_cf_elections::SharedData::<Runtime, SolanaInstance>, BTreeMap<_,_>>(block_info.hash).await.expect("Should always exist");
				solana_shared_data_map.extend(
					client_copy.storage_map::<pallet_cf_elections::SharedData::<Runtime, SolanaInstance>, BTreeMap<_,_>>(last_block_hash).await.expect("Should always exist")
				);

				// ===== Query ElectionProperties at X-1 (to get all active elections before this block) =====
				let bitcoin_elections_prev: BTreeMap<ElectionIdentifier<BitcoinElectionIdentifierExtra>, BitcoinElectionProperties> = client_copy
					.storage_map::<pallet_cf_elections::ElectionProperties<Runtime, BitcoinInstance>, _>(last_block_hash)
					.await
					.expect("Should always exist");
				let solana_elections_prev: BTreeMap<ElectionIdentifier<SolanaElectionIdentifierExtra>, SolanaElectionProperties> = client_copy
					.storage_map::<pallet_cf_elections::ElectionProperties<Runtime, SolanaInstance>, _>(last_block_hash)
					.await
					.expect("Should always exist");

				// ===== Query ElectionProperties at X (to detect completed elections) =====
				let bitcoin_elections_curr: BTreeMap<ElectionIdentifier<BitcoinElectionIdentifierExtra>, BitcoinElectionProperties> = client_copy
					.storage_map::<pallet_cf_elections::ElectionProperties<Runtime, BitcoinInstance>, _>(block_info.hash)
					.await
					.expect("Should always exist");
				let solana_elections_curr: BTreeMap<ElectionIdentifier<SolanaElectionIdentifierExtra>, SolanaElectionProperties> = client_copy
					.storage_map::<pallet_cf_elections::ElectionProperties<Runtime, SolanaInstance>, _>(block_info.hash)
					.await
					.expect("Should always exist");

				// ===== Query BitmapComponents at X-1 (existing shared votes) =====
				let bitcoin_bitmap_components: BTreeMap<UniqueMonotonicIdentifier, BitcoinElectionBitmapComponents> = client_copy
					.storage_map::<pallet_cf_elections::BitmapComponents<Runtime, BitcoinInstance>, _>(last_block_hash)
					.await
					.expect("Should always exist");
				let solana_bitmap_components: BTreeMap<UniqueMonotonicIdentifier, SolanaElectionBitmapComponents> = client_copy
					.storage_map::<pallet_cf_elections::BitmapComponents<Runtime, SolanaInstance>, _>(last_block_hash)
					.await
					.expect("Should always exist");

				// ===== Build mapping from UniqueMonotonicIdentifier to full ElectionIdentifier =====
				let btc_unique_to_election: BTreeMap<UniqueMonotonicIdentifier, ElectionIdentifier<BitcoinElectionIdentifierExtra>> = bitcoin_elections_prev
					.keys()
					.map(|eid| (*eid.unique_monotonic(), *eid))
					.collect();
				let sol_unique_to_election: BTreeMap<UniqueMonotonicIdentifier, ElectionIdentifier<SolanaElectionIdentifierExtra>> = solana_elections_prev
					.keys()
					.map(|eid| (*eid.unique_monotonic(), *eid))
					.collect();

				// ===== Build vote summary from storage =====
				// Map: ElectionIdentifier -> Vec<(BitmapComponent, vote count)>
				// Using Vec instead of BTreeMap because BitmapComponent doesn't implement Ord
				let mut bitcoin_storage_votes: BTreeMap<ElectionIdentifier<BitcoinElectionIdentifierExtra>, Vec<(BitcoinBitmapComponent, u32)>> = BTreeMap::new();
				let mut solana_storage_votes: BTreeMap<ElectionIdentifier<SolanaElectionIdentifierExtra>, Vec<(SolanaBitmapComponent, u32)>> = BTreeMap::new();

				// Count votes from BitmapComponents
				for (unique_id, bitmap_data) in &bitcoin_bitmap_components {
					if let Some(election_id) = btc_unique_to_election.get(unique_id) {
						let vote_list = bitcoin_storage_votes.entry(*election_id).or_default();
						for (bitmap_component, bitvec) in &bitmap_data.bitmaps {
							let count = bitvec.count_ones() as u32;
							vote_list.push((bitmap_component.clone(), count));
						}
					}
				}
				for (unique_id, bitmap_data) in &solana_bitmap_components {
					if let Some(election_id) = sol_unique_to_election.get(unique_id) {
						let vote_list = solana_storage_votes.entry(*election_id).or_default();
						for (bitmap_component, bitvec) in &bitmap_data.bitmaps {
							let count = bitvec.count_ones() as u32;
							vote_list.push((bitmap_component.clone(), count));
						}
					}
				}

				// // ===== Query IndividualComponents at X-1 =====
				// let validators: Vec<_> = client_copy
				// 	.storage_value::<pallet_cf_validator::CurrentAuthorities::<Runtime>>(last_block_hash)
				// 	.await
				// 	.expect("Should always exist");

				// // Map: ElectionIdentifier -> Vec<(IndividualComponent, vote count)>
				// let mut bitcoin_individual_votes: BTreeMap<ElectionIdentifier<BitcoinElectionIdentifierExtra>, Vec<(BitcoinIndividualComponent, u32)>> = BTreeMap::new();
				// let mut solana_individual_votes: BTreeMap<ElectionIdentifier<SolanaElectionIdentifierExtra>, Vec<(SolanaIndividualComponent, u32)>> = BTreeMap::new();

				// // Query IndividualComponents for Bitcoin elections
				// // Using Vec with linear search because IndividualComponent doesn't implement Ord
				// for election_id in bitcoin_elections_prev.keys() {
				// 	let mut component_counts: Vec<(BitcoinIndividualComponent, u32)> = Vec::new();
				// 	for validator in &validators {
				// 		if let Some((_, component)) = client_copy
				// 			.storage_double_map_entry::<pallet_cf_elections::IndividualComponents::<Runtime, BitcoinInstance>>(
				// 				last_block_hash,
				// 				election_id.unique_monotonic(),
				// 				validator
				// 			)
				// 			.await
				// 			.expect("Should always exist")
				// 		{
				// 			if let Some((_, count)) = component_counts.iter_mut().find(|(c, _)| c == &component) {
				// 				*count += 1;
				// 			} else {
				// 				component_counts.push((component, 1));
				// 			}
				// 		}
				// 	}
				// 	if !component_counts.is_empty() {
				// 		bitcoin_individual_votes.insert(*election_id, component_counts);
				// 	}
				// }

				// // Query IndividualComponents for Solana elections
				// for election_id in solana_elections_prev.keys() {
				// 	let mut component_counts: Vec<(SolanaIndividualComponent, u32)> = Vec::new();
				// 	for validator in &validators {
				// 		if let Some((_, component)) = client_copy
				// 			.storage_double_map_entry::<pallet_cf_elections::IndividualComponents::<Runtime, SolanaInstance>>(
				// 				last_block_hash,
				// 				election_id.unique_monotonic(),
				// 				validator
				// 			)
				// 			.await
				// 			.expect("Should always exist")
				// 		{
				// 			if let Some((_, count)) = component_counts.iter_mut().find(|(c, _)| c == &component) {
				// 				*count += 1;
				// 			} else {
				// 				component_counts.push((component, 1));
				// 			}
				// 		}
				// 	}
				// 	if !component_counts.is_empty() {
				// 		solana_individual_votes.insert(*election_id, component_counts);
				// 	}
				// }

				// ===== Track new votes from extrinsics =====
				let mut bitcoin_extrinsic_votes: BTreeMap<ElectionIdentifier<BitcoinElectionIdentifierExtra>, BTreeMap<<BitcoinVoteStorageTuple as VoteStorage>::PartialVote, u32>> = BTreeMap::new();
				let mut btc_partial_to_vote: BTreeMap<<BitcoinVoteStorageTuple as VoteStorage>::PartialVote, <BitcoinVoteStorageTuple as VoteStorage>::Vote> = BTreeMap::new();

				let mut solana_extrinsic_votes: BTreeMap<ElectionIdentifier<SolanaElectionIdentifierExtra>, BTreeMap<<SolanaVoteStorageTuple as VoteStorage>::PartialVote, u32>> = BTreeMap::new();
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
													solana_extrinsic_votes.entry(election_id).or_default()
														.entry(partial.clone()).and_modify(|entry| *entry += 1).or_insert(1);
												},
												pallet_cf_elections::vote_storage::AuthorityVote::Vote(full_vote) => {
													let partial = <SolanaVoteStorageTuple as VoteStorage>::vote_into_partial_vote(&full_vote, |shared_data| SharedDataHash::of(&shared_data));
													sol_partial_to_vote.insert(partial.clone(), full_vote.clone());
													solana_extrinsic_votes.entry(election_id).or_default()
														.entry(partial.clone()).and_modify(|entry| *entry += 1).or_insert(1);
												},
											}
										}
									},
									pallet_cf_elections::Call::provide_shared_data { shared_data } => {
										let shared_data_hash = SharedDataHash::of(&shared_data);
										println!("  [Solana] Providing shared data: {:?} -> {:?}", shared_data_hash, shared_data);
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
													bitcoin_extrinsic_votes.entry(election_id).or_default()
														.entry(partial.clone()).and_modify(|entry| *entry += 1).or_insert(1);
												},
												pallet_cf_elections::vote_storage::AuthorityVote::Vote(full_vote) => {
													let partial = <BitcoinVoteStorageTuple as VoteStorage>::vote_into_partial_vote(&full_vote, |shared_data| SharedDataHash::of(&shared_data));
													btc_partial_to_vote.insert(partial.clone(), full_vote.clone());
													bitcoin_extrinsic_votes.entry(election_id).or_default()
														.entry(partial.clone()).and_modify(|entry| *entry += 1).or_insert(1);
												},
											}
										}
									},
									pallet_cf_elections::Call::provide_shared_data { shared_data } => {
										let shared_data_hash = SharedDataHash::of(&shared_data);
										println!("  [Bitcoin] Providing shared data: {:?} -> {:?}", shared_data_hash, shared_data);
									},
									_ => {},
								}
							},
							_ => {},
						}
					}

					// ===== Detect completed elections =====
					let bitcoin_completed: Vec<_> = bitcoin_elections_prev.keys()
						.filter(|eid| !bitcoin_elections_curr.contains_key(eid))
						.cloned()
						.collect();
					let solana_completed: Vec<_> = solana_elections_prev.keys()
						.filter(|eid| !solana_elections_curr.contains_key(eid))
						.cloned()
						.collect();

					// ===== Print vote summary =====
					let btc_active = bitcoin_elections_prev.len();
					let btc_completed_count = bitcoin_completed.len();
					let sol_active = solana_elections_prev.len();
					let sol_completed_count = solana_completed.len();

					println!("\nBITCOIN ELECTIONS ({} active, {} completed this block):", btc_active, btc_completed_count);
					for (election_id, _props) in &bitcoin_elections_prev {
						let is_completed = bitcoin_completed.contains(election_id);
						let status = if is_completed { " [COMPLETED]" } else { "" };
						println!("  Election {:?}{}", election_id, status);

						// Show storage votes (from BitmapComponents)
						if let Some(vote_list) = bitcoin_storage_votes.get(election_id) {
							println!("    Bitmap votes (from storage at block {}):", block_info.number - 1);
							for (bitmap_component, count) in vote_list {
								println!("      {:?}: {} votes", bitmap_component, count);
							}
						}

						// // Show storage votes (from IndividualComponents)
						// if let Some(vote_list) = bitcoin_individual_votes.get(election_id) {
						// 	println!("    Individual votes (from storage at block {}):", block_info.number - 1);
						// 	for (individual_component, count) in vote_list {
						// 		println!("      {:?}: {} votes", individual_component, count);
						// 	}
						// }

						// Show new votes from this block's extrinsics
						if let Some(extrinsic_votes) = bitcoin_extrinsic_votes.get(election_id) {
							println!("    New votes this block:");
							for (partial, count) in extrinsic_votes {
								if let Some(full_vote) = btc_partial_to_vote.get(partial) {
									println!("      {:?}: {} votes", full_vote, count);
								} else {
									// Try to resolve partial vote
									match partial {
										pallet_cf_elections::vote_storage::composite::tuple_6_impls::CompositePartialVote::A(inner) => {
											let result = bitcoin_shared_data_map.get(inner);
											println!("      A({:?}): {} votes", result, count);
										},
										pallet_cf_elections::vote_storage::composite::tuple_6_impls::CompositePartialVote::B(inner) => {
											let result = bitcoin_shared_data_map.get(inner);
											println!("      B({:?}): {} votes", result, count);
										},
										pallet_cf_elections::vote_storage::composite::tuple_6_impls::CompositePartialVote::C(inner) => {
											let result = bitcoin_shared_data_map.get(inner);
											println!("      C({:?}): {} votes", result, count);
										},
										pallet_cf_elections::vote_storage::composite::tuple_6_impls::CompositePartialVote::D(inner) => {
											let result = bitcoin_shared_data_map.get(inner);
											println!("      D({:?}): {} votes", result, count);
										},
										pallet_cf_elections::vote_storage::composite::tuple_6_impls::CompositePartialVote::EE(inner) => {
											println!("      EE({:?}): {} votes", inner, count);
										},
										pallet_cf_elections::vote_storage::composite::tuple_6_impls::CompositePartialVote::FF(inner) => {
											println!("      FF({:?}): {} votes", inner, count);
										},
									}
								}
							}
						}
					}

					println!("\nSOLANA ELECTIONS ({} active, {} completed this block):", sol_active, sol_completed_count);
					for (election_id, _props) in &solana_elections_prev {
						let is_completed = solana_completed.contains(election_id);
						let status = if is_completed { " [COMPLETED]" } else { "" };
						println!("  Election {:?}{}", election_id, status);

						// Show storage votes (from BitmapComponents)
						if let Some(vote_list) = solana_storage_votes.get(election_id) {
							println!("    Bitmap votes (from storage at block {}):", block_info.number - 1);
							for (bitmap_component, count) in vote_list {
								println!("      {:?}: {} votes", bitmap_component, count);
							}
						}

						// // Show storage votes (from IndividualComponents)
						// if let Some(vote_list) = solana_individual_votes.get(election_id) {
						// 	println!("    Individual votes (from storage at block {}):", block_info.number - 1);
						// 	for (individual_component, count) in vote_list {
						// 		println!("      {:?}: {} votes", individual_component, count);
						// 	}
						// }

						// Show new votes from this block's extrinsics
						if let Some(extrinsic_votes) = solana_extrinsic_votes.get(election_id) {
							println!("    New votes this block:");
							for (partial, count) in extrinsic_votes {
								if let Some(full_vote) = sol_partial_to_vote.get(partial) {
									println!("      {:?}: {} votes", full_vote, count);
								} else {
									// Try to resolve partial vote
									match partial {
										pallet_cf_elections::vote_storage::composite::tuple_7_impls::CompositePartialVote::A(inner) => {
											println!("      A({:?}): {} votes", inner, count);
										},
										pallet_cf_elections::vote_storage::composite::tuple_7_impls::CompositePartialVote::B(inner) => {
											println!("      B({:?}): {} votes", inner, count);
										},
										pallet_cf_elections::vote_storage::composite::tuple_7_impls::CompositePartialVote::C(inner) => {
											let result = solana_shared_data_map.get(&inner.value);
											println!("      C({:?}) Slot {:?}: {} votes", result, inner.block, count);
										},
										pallet_cf_elections::vote_storage::composite::tuple_7_impls::CompositePartialVote::D(inner) => {
											println!("      D({:?}): {} votes", inner, count);
										},
										pallet_cf_elections::vote_storage::composite::tuple_7_impls::CompositePartialVote::EE(inner) => {
											println!("      EE({:?}): {} votes", inner, count);
										},
										pallet_cf_elections::vote_storage::composite::tuple_7_impls::CompositePartialVote::FF(inner) => {
											let result = solana_shared_data_map.get(inner);
											println!("      FF({:?}): {} votes", result, count);
										},
										pallet_cf_elections::vote_storage::composite::tuple_7_impls::CompositePartialVote::G(inner) => {
											let result = solana_shared_data_map.get(inner);
											println!("      G({:?}): {} votes", result, count);
										},
									}
								}
							}
						}
					}

					println!("\n{}", "=".repeat(80));
				}
			}
		}).await;

		Ok(())

	 }.boxed()).await.unwrap()
}
