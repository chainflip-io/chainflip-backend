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
use futures::{SinkExt, StreamExt};
use futures_util::FutureExt;
use pallet_cf_elections::{
	ElectionIdentifier, ElectoralSystemTypes, SharedDataHash, UniqueMonotonicIdentifier,
	bitmap_components::ElectionBitmapComponents, vote_storage::VoteStorage,
};
use serde::Serialize;
use state_chain_runtime::{
	BitcoinInstance, Runtime, SolanaInstance,
	chainflip::{
		bitcoin_elections::BitcoinElectoralSystemRunner,
		solana_elections::SolanaElectoralSystemRunner,
	},
};
use std::{
	env,
	net::{IpAddr, SocketAddr},
};
use tokio::sync::broadcast;
use warp::Filter;

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

// ===== Dashboard types (JSON-serializable for the web UI) =====

#[derive(Serialize, Clone)]
struct BlockUpdate {
	block_number: u32,
	bitcoin: ChainElections,
	solana: ChainElections,
}

#[derive(Serialize, Clone)]
struct ChainElections {
	active_count: usize,
	completed_count: usize,
	elections: Vec<ElectionSummary>,
}

#[derive(Serialize, Clone)]
struct ElectionSummary {
	election_type: String,
	election_id: String,
	completed: bool,
	bitmap_votes: Vec<VoteGroup>,
	individual_votes: Vec<VoteGroup>,
	extrinsic_votes: Vec<ExtrinsicVoteGroup>,
}

#[derive(Serialize, Clone)]
struct VoteGroup {
	component: String,
	count: u32,
}

#[derive(Serialize, Clone)]
struct ExtrinsicVoteGroup {
	variant_name: String,
	detail: String,
	count: u32,
}

// ===== Dashboard web server =====

const DASHBOARD_HTML: &str = include_str!("../dashboard.html");

async fn run_dashboard(bind_addr: SocketAddr, tx: broadcast::Sender<String>) {
	let index = warp::path::end().and(warp::get()).map(|| warp::reply::html(DASHBOARD_HTML));

	let ws_route = warp::path("ws").and(warp::ws()).map(move |ws: warp::ws::Ws| {
		let rx = tx.subscribe();
		ws.on_upgrade(move |websocket| handle_ws_client(websocket, rx))
	});

	let routes = index.or(ws_route);

	println!("Dashboard available at http://{}", bind_addr);
	warp::serve(routes).run(bind_addr).await;
}

async fn handle_ws_client(ws: warp::ws::WebSocket, mut rx: broadcast::Receiver<String>) {
	let (mut ws_tx, mut ws_rx) = ws.split();

	let send_task = tokio::spawn(async move {
		loop {
			match rx.recv().await {
				Ok(msg) =>
					if ws_tx.send(warp::ws::Message::text(msg)).await.is_err() {
						break;
					},
				Err(broadcast::error::RecvError::Lagged(_)) => continue,
				Err(broadcast::error::RecvError::Closed) => break,
			}
		}
	});

	while ws_rx.next().await.is_some() {}

	send_task.abort();
}

// Helper functions for human-readable election and vote type names
use pallet_cf_elections::electoral_systems::composite::{
	tuple_6_impls::CompositeElectionIdentifierExtra as BtcCompositeExtra,
	tuple_7_impls::CompositeElectionIdentifierExtra as SolCompositeExtra,
};

fn bitcoin_election_type_name(
	election_id: &ElectionIdentifier<BitcoinElectionIdentifierExtra>,
) -> &'static str {
	match election_id.extra() {
		BtcCompositeExtra::A(_) => "BlockHeight",
		BtcCompositeExtra::B(_) => "DepositChannel",
		BtcCompositeExtra::C(_) => "VaultDeposit",
		BtcCompositeExtra::D(_) => "Egress",
		BtcCompositeExtra::EE(_) => "FeeTracking",
		BtcCompositeExtra::FF(_) => "Liveness",
	}
}

fn solana_election_type_name(
	election_id: &ElectionIdentifier<SolanaElectionIdentifierExtra>,
) -> &'static str {
	match election_id.extra() {
		SolCompositeExtra::A(_) => "BlockHeight",
		SolCompositeExtra::B(_) => "Ingress",
		SolCompositeExtra::C(_) => "Nonce",
		SolCompositeExtra::D(_) => "Egress",
		SolCompositeExtra::EE(_) => "Liveness",
		SolCompositeExtra::FF(_) => "VaultSwap",
		SolCompositeExtra::G(_) => "ALT",
	}
}

fn bitcoin_vote_variant_name(
	variant: &<BitcoinVoteStorageTuple as VoteStorage>::PartialVote,
) -> &'static str {
	match variant {
		pallet_cf_elections::vote_storage::composite::tuple_6_impls::CompositePartialVote::A(_) =>
			"BlockHeight",
		pallet_cf_elections::vote_storage::composite::tuple_6_impls::CompositePartialVote::B(_) =>
			"DepositChannel",
		pallet_cf_elections::vote_storage::composite::tuple_6_impls::CompositePartialVote::C(_) =>
			"VaultDeposit",
		pallet_cf_elections::vote_storage::composite::tuple_6_impls::CompositePartialVote::D(_) =>
			"Egress",
		pallet_cf_elections::vote_storage::composite::tuple_6_impls::CompositePartialVote::EE(
			_,
		) => "FeeTracking",
		pallet_cf_elections::vote_storage::composite::tuple_6_impls::CompositePartialVote::FF(
			_,
		) => "Liveness",
	}
}

fn solana_vote_variant_name(
	variant: &<SolanaVoteStorageTuple as VoteStorage>::PartialVote,
) -> &'static str {
	match variant {
		pallet_cf_elections::vote_storage::composite::tuple_7_impls::CompositePartialVote::A(_) =>
			"BlockHeight",
		pallet_cf_elections::vote_storage::composite::tuple_7_impls::CompositePartialVote::B(_) =>
			"Ingress",
		pallet_cf_elections::vote_storage::composite::tuple_7_impls::CompositePartialVote::C(_) =>
			"Nonce",
		pallet_cf_elections::vote_storage::composite::tuple_7_impls::CompositePartialVote::D(_) =>
			"Egress",
		pallet_cf_elections::vote_storage::composite::tuple_7_impls::CompositePartialVote::EE(
			_,
		) => "Liveness",
		pallet_cf_elections::vote_storage::composite::tuple_7_impls::CompositePartialVote::FF(
			_,
		) => "VaultSwap",
		pallet_cf_elections::vote_storage::composite::tuple_7_impls::CompositePartialVote::G(_) =>
			"ALT",
	}
}

#[tokio::main(flavor = "multi_thread", worker_threads = 3)]
async fn main() {
	// http://localhost:9944
	let rpc_url = env::var("CF_RPC_NODE").unwrap_or("wss://mainnet-archive.chainflip.io".into());

	observe_elections(rpc_url).await;
}

async fn observe_elections(rpc_url: String) {
	let (ws_tx, _) = broadcast::channel::<String>(32);

	task_scope::task_scope(|scope| async move {

		// Spawn dashboard web server
		let dashboard_port: u16 = env::var("DASHBOARD_PORT")
			.ok()
			.and_then(|p| p.parse().ok())
			.unwrap_or(8080);
		let dashboard_host: IpAddr = env::var("DASHBOARD_HOST")
			.ok()
			.and_then(|host| host.parse().ok())
			.unwrap_or(IpAddr::from([127, 0, 0, 1]));
		let dashboard_addr = SocketAddr::new(dashboard_host, dashboard_port);
		let ws_tx_for_server = ws_tx.clone();
		scope.spawn_weak(async move {
			run_dashboard(dashboard_addr, ws_tx_for_server).await;
			Ok(())
		});

		let (finalized_stream, _unfinalized_stream, client) = StateChainClient::connect_without_account(scope, &rpc_url).await.unwrap();
		finalized_stream.for_each(|block_info| {
			let client_copy = client.clone();
			let ws_tx = ws_tx.clone();
			async move {
				println!("\n{:=^80}", format!(" Block {} ", block_info.number));
				let Some(previous_block_number) = block_info.number.checked_sub(1) else {
					println!("Skipping block {} because there is no previous block", block_info.number);
					return;
				};
				let last_block_hash = client_copy
					.base_rpc_client
					.block_hash(previous_block_number)
					.await
					.expect("Shouldn't fail")
					.unwrap();

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

					// ===== Query IndividualComponents at X-1 (bulk fetch) =====
				let bitcoin_individual_components: Vec<_> = client_copy
					.storage_double_map::<pallet_cf_elections::IndividualComponents::<Runtime, BitcoinInstance>, Vec<_>>(last_block_hash)
					.await
					.expect("Should always exist");
				let solana_individual_components: Vec<_> = client_copy
					.storage_double_map::<pallet_cf_elections::IndividualComponents::<Runtime, SolanaInstance>, Vec<_>>(last_block_hash)
					.await
					.expect("Should always exist");

				// Map: ElectionIdentifier -> Vec<(IndividualComponent, vote count)>
				let mut bitcoin_individual_votes: BTreeMap<ElectionIdentifier<BitcoinElectionIdentifierExtra>, Vec<(BitcoinIndividualComponent, u32)>> = BTreeMap::new();
				let mut solana_individual_votes: BTreeMap<ElectionIdentifier<SolanaElectionIdentifierExtra>, Vec<(SolanaIndividualComponent, u32)>> = BTreeMap::new();

				for ((unique_id, _validator), (_props, component)) in &bitcoin_individual_components {
					if let Some(election_id) = btc_unique_to_election.get(unique_id) {
						let vote_list = bitcoin_individual_votes.entry(*election_id).or_default();
						if let Some((_, count)) = vote_list.iter_mut().find(|(c, _)| c == component) {
							*count += 1;
						} else {
							vote_list.push((component.clone(), 1));
						}
					}
				}
				for ((unique_id, _validator), (_props, component)) in &solana_individual_components {
					if let Some(election_id) = sol_unique_to_election.get(unique_id) {
						let vote_list = solana_individual_votes.entry(*election_id).or_default();
						if let Some((_, count)) = vote_list.iter_mut().find(|(c, _)| c == component) {
							*count += 1;
						} else {
							vote_list.push((component.clone(), 1));
						}
					}
				}

				// ===== Track new votes from extrinsics =====
				let mut bitcoin_extrinsic_votes: BTreeMap<ElectionIdentifier<BitcoinElectionIdentifierExtra>, BTreeMap<<BitcoinVoteStorageTuple as VoteStorage>::PartialVote, u32>> = BTreeMap::new();
				let mut solana_extrinsic_votes: BTreeMap<ElectionIdentifier<SolanaElectionIdentifierExtra>, BTreeMap<<SolanaVoteStorageTuple as VoteStorage>::PartialVote, u32>> = BTreeMap::new();
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
						let election_type = bitcoin_election_type_name(election_id);
						println!("  [{}] Election {:?}{}", election_type, election_id, status);

						// Show storage votes (from BitmapComponents)
						if let Some(vote_list) = bitcoin_storage_votes.get(election_id) {
							println!("    Bitmap votes (from storage at block {}):", previous_block_number);
							for (bitmap_component, count) in vote_list {
								let resolved_component =
									format_bitcoin_bitmap_component(bitmap_component, &bitcoin_shared_data_map);
								println!("      {}: {} votes", resolved_component, count);
							}
						}

						// Show storage votes (from IndividualComponents)
						if let Some(vote_list) = bitcoin_individual_votes.get(election_id) {
							println!("    Individual votes (from storage at block {}):", previous_block_number);
							for (individual_component, count) in vote_list {
								println!("      {:?}: {} votes", individual_component, count);
							}
						}

						// Show new votes from this block's extrinsics
						if let Some(extrinsic_votes) = bitcoin_extrinsic_votes.get(election_id) {
							println!("    New votes this block:");
							for (partial, count) in extrinsic_votes {
								let variant_name = bitcoin_vote_variant_name(partial);
								match partial {
										pallet_cf_elections::vote_storage::composite::tuple_6_impls::CompositePartialVote::A(inner) => {
											let result = bitcoin_shared_data_map.get(inner);
											println!("      [{}] {:?}: {} votes", variant_name, result, count);
										},
										pallet_cf_elections::vote_storage::composite::tuple_6_impls::CompositePartialVote::B(inner) => {
											let result = bitcoin_shared_data_map.get(inner);
											println!("      [{}] {:?}: {} votes", variant_name, result, count);
										},
										pallet_cf_elections::vote_storage::composite::tuple_6_impls::CompositePartialVote::C(inner) => {
											let result = bitcoin_shared_data_map.get(inner);
											println!("      [{}] {:?}: {} votes", variant_name, result, count);
										},
										pallet_cf_elections::vote_storage::composite::tuple_6_impls::CompositePartialVote::D(inner) => {
											let result = bitcoin_shared_data_map.get(inner);
											println!("      [{}] {:?}: {} votes", variant_name, result, count);
										},
										pallet_cf_elections::vote_storage::composite::tuple_6_impls::CompositePartialVote::EE(inner) => {
											println!("      [{}] {:?}: {} votes", variant_name, inner, count);
										},
										pallet_cf_elections::vote_storage::composite::tuple_6_impls::CompositePartialVote::FF(inner) => {
											println!("      [{}] {:?}: {} votes", variant_name, inner, count);
										},
									}
							}
						}
					}

					println!("\nSOLANA ELECTIONS ({} active, {} completed this block):", sol_active, sol_completed_count);
					for (election_id, _props) in &solana_elections_prev {
						let is_completed = solana_completed.contains(election_id);
						let status = if is_completed { " [COMPLETED]" } else { "" };
						let election_type = solana_election_type_name(election_id);
						println!("  [{}] Election {:?}{}", election_type, election_id, status);

						// Show storage votes (from BitmapComponents)
						if let Some(vote_list) = solana_storage_votes.get(election_id) {
							println!("    Bitmap votes (from storage at block {}):", previous_block_number);
							for (bitmap_component, count) in vote_list {
								let resolved_component =
									format_solana_bitmap_component(bitmap_component, &solana_shared_data_map);
								println!("      {}: {} votes", resolved_component, count);
							}
						}

						// Show storage votes (from IndividualComponents)
						if let Some(vote_list) = solana_individual_votes.get(election_id) {
							println!("    Individual votes (from storage at block {}):", previous_block_number);
							for (individual_component, count) in vote_list {
								println!("      {:?}: {} votes", individual_component, count);
							}
						}

						// Show new votes from this block's extrinsics
						if let Some(extrinsic_votes) = solana_extrinsic_votes.get(election_id) {
							println!("    New votes this block:");
							for (partial, count) in extrinsic_votes {
								let variant_name = solana_vote_variant_name(partial);
								match partial {
										pallet_cf_elections::vote_storage::composite::tuple_7_impls::CompositePartialVote::A(inner) => {
											println!("      [{}] {:?}: {} votes", variant_name, inner, count);
										},
										pallet_cf_elections::vote_storage::composite::tuple_7_impls::CompositePartialVote::B(inner) => {
											println!("      [{}] {:?}: {} votes", variant_name, inner, count);
										},
										pallet_cf_elections::vote_storage::composite::tuple_7_impls::CompositePartialVote::C(inner) => {
											let result = solana_shared_data_map.get(&inner.value);
											println!("      [{}] {:?} Slot {:?}: {} votes", variant_name, result, inner.block, count);
										},
										pallet_cf_elections::vote_storage::composite::tuple_7_impls::CompositePartialVote::D(inner) => {
											println!("      [{}] {:?}: {} votes", variant_name, inner, count);
										},
										pallet_cf_elections::vote_storage::composite::tuple_7_impls::CompositePartialVote::EE(inner) => {
											println!("      [{}] {:?}: {} votes", variant_name, inner, count);
										},
										pallet_cf_elections::vote_storage::composite::tuple_7_impls::CompositePartialVote::FF(inner) => {
											let result = solana_shared_data_map.get(inner);
											println!("      [{}] {:?}: {} votes", variant_name, result, count);
										},
										pallet_cf_elections::vote_storage::composite::tuple_7_impls::CompositePartialVote::G(inner) => {
											let result = solana_shared_data_map.get(inner);
											println!("      [{}] {:?}: {} votes", variant_name, result, count);
										},
									}
							}
						}
					}

					println!("\n{}", "=".repeat(80));

					// ===== Build and broadcast dashboard update =====
					let btc_election_summaries: Vec<ElectionSummary> = bitcoin_elections_prev.keys().map(|election_id| {
						let is_completed = bitcoin_completed.contains(election_id);
						let bitmap_votes = bitcoin_storage_votes.get(election_id)
							.map(|votes| votes.iter().map(|(comp, count)| VoteGroup {
								component: format_bitcoin_bitmap_component(comp, &bitcoin_shared_data_map),
								count: *count,
							}).collect())
							.unwrap_or_default();
						let individual_votes_json = bitcoin_individual_votes.get(election_id)
							.map(|votes| votes.iter().map(|(comp, count)| VoteGroup {
								component: format!("{:?}", comp),
								count: *count,
							}).collect())
							.unwrap_or_default();
						let extrinsic_votes_json = bitcoin_extrinsic_votes.get(election_id)
							.map(|votes| votes.iter().map(|(partial, count)| {
								let variant_name = bitcoin_vote_variant_name(partial).to_string();
								let detail = format_bitcoin_vote_detail(partial, &bitcoin_shared_data_map);
								ExtrinsicVoteGroup { variant_name, detail, count: *count }
							}).collect())
							.unwrap_or_default();
						ElectionSummary {
							election_type: bitcoin_election_type_name(election_id).to_string(),
							election_id: format!("{:?}", election_id),
							completed: is_completed,
							bitmap_votes,
							individual_votes: individual_votes_json,
							extrinsic_votes: extrinsic_votes_json,
						}
					}).collect();

					let sol_election_summaries: Vec<ElectionSummary> = solana_elections_prev.keys().map(|election_id| {
						let is_completed = solana_completed.contains(election_id);
						let bitmap_votes = solana_storage_votes.get(election_id)
							.map(|votes| votes.iter().map(|(comp, count)| VoteGroup {
								component: format_solana_bitmap_component(comp, &solana_shared_data_map),
								count: *count,
							}).collect())
							.unwrap_or_default();
						let individual_votes_json = solana_individual_votes.get(election_id)
							.map(|votes| votes.iter().map(|(comp, count)| VoteGroup {
								component: format!("{:?}", comp),
								count: *count,
							}).collect())
							.unwrap_or_default();
						let extrinsic_votes_json = solana_extrinsic_votes.get(election_id)
							.map(|votes| votes.iter().map(|(partial, count)| {
								let variant_name = solana_vote_variant_name(partial).to_string();
								let detail = format_solana_vote_detail(partial, &solana_shared_data_map);
								ExtrinsicVoteGroup { variant_name, detail, count: *count }
							}).collect())
							.unwrap_or_default();
						ElectionSummary {
							election_type: solana_election_type_name(election_id).to_string(),
							election_id: format!("{:?}", election_id),
							completed: is_completed,
							bitmap_votes,
							individual_votes: individual_votes_json,
							extrinsic_votes: extrinsic_votes_json,
						}
					}).collect();

					let block_update = BlockUpdate {
						block_number: block_info.number,
						bitcoin: ChainElections {
							active_count: btc_active,
							completed_count: btc_completed_count,
							elections: btc_election_summaries,
						},
						solana: ChainElections {
							active_count: sol_active,
							completed_count: sol_completed_count,
							elections: sol_election_summaries,
						},
					};

					if let Ok(json) = serde_json::to_string(&block_update) {
						let _ = ws_tx.send(json);
					}
				}
			}
		}).await;

		Ok(())

		 }.boxed()).await.unwrap()
}

fn resolve_shared_data_value(
	shared_data_hash: &SharedDataHash,
	shared_data_map: &BTreeMap<SharedDataHash, impl std::fmt::Debug>,
) -> String {
	match shared_data_map.get(shared_data_hash) {
		Some(value) => format!("{:?}", value),
		None => format!("MissingSharedData({:?})", shared_data_hash),
	}
}

fn format_bitcoin_bitmap_component(
	component: &BitcoinBitmapComponent,
	shared_data_map: &BTreeMap<SharedDataHash, impl std::fmt::Debug>,
) -> String {
	use pallet_cf_elections::vote_storage::composite::tuple_6_impls::CompositeBitmapComponent;
	match component {
		CompositeBitmapComponent::A(shared_data_hash) =>
			format!("A({})", resolve_shared_data_value(shared_data_hash, shared_data_map)),
		CompositeBitmapComponent::B(shared_data_hash) =>
			format!("B({})", resolve_shared_data_value(shared_data_hash, shared_data_map)),
		CompositeBitmapComponent::C(shared_data_hash) =>
			format!("C({})", resolve_shared_data_value(shared_data_hash, shared_data_map)),
		CompositeBitmapComponent::D(shared_data_hash) =>
			format!("D({})", resolve_shared_data_value(shared_data_hash, shared_data_map)),
		CompositeBitmapComponent::EE(value) => format!("EE({:?})", value),
		CompositeBitmapComponent::FF(value) => format!("FF({:?})", value),
	}
}

fn format_solana_bitmap_component(
	component: &SolanaBitmapComponent,
	shared_data_map: &BTreeMap<SharedDataHash, impl std::fmt::Debug>,
) -> String {
	use pallet_cf_elections::vote_storage::composite::tuple_7_impls::CompositeBitmapComponent;
	match component {
		CompositeBitmapComponent::A(value) => format!("A({:?})", value),
		CompositeBitmapComponent::B(value) => format!("B({:?})", value),
		CompositeBitmapComponent::C(shared_data_hash) =>
			format!("C({})", resolve_shared_data_value(shared_data_hash, shared_data_map)),
		CompositeBitmapComponent::D(value) => format!("D({:?})", value),
		CompositeBitmapComponent::EE(value) => format!("EE({:?})", value),
		CompositeBitmapComponent::FF(shared_data_hash) =>
			format!("FF({})", resolve_shared_data_value(shared_data_hash, shared_data_map)),
		CompositeBitmapComponent::G(shared_data_hash) =>
			format!("G({})", resolve_shared_data_value(shared_data_hash, shared_data_map)),
	}
}

fn format_bitcoin_vote_detail(
	partial: &<BitcoinVoteStorageTuple as VoteStorage>::PartialVote,
	shared_data_map: &BTreeMap<SharedDataHash, impl std::fmt::Debug>,
) -> String {
	use pallet_cf_elections::vote_storage::composite::tuple_6_impls::CompositePartialVote;
	match partial {
		CompositePartialVote::A(inner) => format!("{:?}", shared_data_map.get(inner)),
		CompositePartialVote::B(inner) => format!("{:?}", shared_data_map.get(inner)),
		CompositePartialVote::C(inner) => format!("{:?}", shared_data_map.get(inner)),
		CompositePartialVote::D(inner) => format!("{:?}", shared_data_map.get(inner)),
		CompositePartialVote::EE(inner) => format!("{:?}", inner),
		CompositePartialVote::FF(inner) => format!("{:?}", inner),
	}
}

fn format_solana_vote_detail(
	partial: &<SolanaVoteStorageTuple as VoteStorage>::PartialVote,
	shared_data_map: &BTreeMap<SharedDataHash, impl std::fmt::Debug>,
) -> String {
	use pallet_cf_elections::vote_storage::composite::tuple_7_impls::CompositePartialVote;
	match partial {
		CompositePartialVote::A(inner) => format!("{:?}", inner),
		CompositePartialVote::B(inner) => format!("{:?}", inner),
		CompositePartialVote::C(inner) =>
			format!("{:?} Slot {:?}", shared_data_map.get(&inner.value), inner.block),
		CompositePartialVote::D(inner) => format!("{:?}", inner),
		CompositePartialVote::EE(inner) => format!("{:?}", inner),
		CompositePartialVote::FF(inner) => format!("{:?}", shared_data_map.get(inner)),
		CompositePartialVote::G(inner) => format!("{:?}", shared_data_map.get(inner)),
	}
}
