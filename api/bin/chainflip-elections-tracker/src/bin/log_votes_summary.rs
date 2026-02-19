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
use tokio::sync::{broadcast, mpsc, oneshot};
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
	composite_id: String,
	election_properties: String,
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
	detail: String,
	count: u32,
}

// ===== Dashboard web server =====

const DASHBOARD_HTML: &str = include_str!("../dashboard.html");

type BlockQueryRequest = (u32, oneshot::Sender<Result<String, String>>);
type BlockQuerySender = mpsc::Sender<BlockQueryRequest>;

async fn run_dashboard(bind_addr: SocketAddr, tx: broadcast::Sender<String>, block_query_tx: BlockQuerySender) {
	let index = warp::path::end().and(warp::get()).map(|| warp::reply::html(DASHBOARD_HTML));

	let ws_route = warp::path("ws").and(warp::ws()).map(move |ws: warp::ws::Ws| {
		let rx = tx.subscribe();
		ws.on_upgrade(move |websocket| handle_ws_client(websocket, rx))
	});

	let block_query = warp::path!("api" / "block" / u32)
		.and(warp::get())
		.then(move |number: u32| {
			let tx = block_query_tx.clone();
			async move {
				let (reply_tx, reply_rx) = oneshot::channel();
				if tx.send((number, reply_tx)).await.is_err() {
					return warp::http::Response::builder()
						.status(500)
						.header("content-type", "application/json")
						.body(r#"{"error":"Internal error"}"#.to_string())
						.unwrap();
				}
				match reply_rx.await {
					Ok(Ok(json)) => warp::http::Response::builder()
						.status(200)
						.header("content-type", "application/json")
						.body(json)
						.unwrap(),
					Ok(Err(e)) => warp::http::Response::builder()
						.status(400)
						.header("content-type", "application/json")
						.body(format!(r#"{{"error":"{}"}}"#, e.replace('"', r#"\""#)))
						.unwrap(),
					Err(_) => warp::http::Response::builder()
						.status(500)
						.header("content-type", "application/json")
						.body(r#"{"error":"Query failed"}"#.to_string())
						.unwrap(),
				}
			}
		});

	let routes = index.or(ws_route).or(block_query);

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

async fn build_block_update(
	client: &StateChainClient<()>,
	block_number: u32,
	block_hash: state_chain_runtime::Hash,
) -> Result<BlockUpdate, String> {
	let previous_block_number = block_number
		.checked_sub(1)
		.ok_or_else(|| "No previous block".to_string())?;
	let last_block_hash = client
		.base_rpc_client
		.block_hash(previous_block_number)
		.await
		.map_err(|e| format!("RPC error: {:?}", e))?
		.ok_or_else(|| format!("Block {} not found", previous_block_number))?;

	// ===== Query SharedData (from both X and X-1 for resolving hashes) =====
	let mut bitcoin_shared_data_map = client.storage_map::<pallet_cf_elections::SharedData::<Runtime, BitcoinInstance>, BTreeMap<_,_>>(block_hash).await.expect("Should always exist");
	bitcoin_shared_data_map.extend(
		client.storage_map::<pallet_cf_elections::SharedData::<Runtime, BitcoinInstance>, BTreeMap<_,_>>(last_block_hash).await.expect("Should always exist")
	);
	let mut solana_shared_data_map = client.storage_map::<pallet_cf_elections::SharedData::<Runtime, SolanaInstance>, BTreeMap<_,_>>(block_hash).await.expect("Should always exist");
	solana_shared_data_map.extend(
		client.storage_map::<pallet_cf_elections::SharedData::<Runtime, SolanaInstance>, BTreeMap<_,_>>(last_block_hash).await.expect("Should always exist")
	);

	// ===== Query ElectionProperties at X-1 (to get all active elections before this block) =====
	let bitcoin_elections_prev: BTreeMap<ElectionIdentifier<BitcoinElectionIdentifierExtra>, BitcoinElectionProperties> = client
		.storage_map::<pallet_cf_elections::ElectionProperties<Runtime, BitcoinInstance>, _>(last_block_hash)
		.await
		.expect("Should always exist");
	let solana_elections_prev: BTreeMap<ElectionIdentifier<SolanaElectionIdentifierExtra>, SolanaElectionProperties> = client
		.storage_map::<pallet_cf_elections::ElectionProperties<Runtime, SolanaInstance>, _>(last_block_hash)
		.await
		.expect("Should always exist");

	// ===== Query ElectionProperties at X (to detect completed elections) =====
	let bitcoin_elections_curr: BTreeMap<ElectionIdentifier<BitcoinElectionIdentifierExtra>, BitcoinElectionProperties> = client
		.storage_map::<pallet_cf_elections::ElectionProperties<Runtime, BitcoinInstance>, _>(block_hash)
		.await
		.expect("Should always exist");
	let solana_elections_curr: BTreeMap<ElectionIdentifier<SolanaElectionIdentifierExtra>, SolanaElectionProperties> = client
		.storage_map::<pallet_cf_elections::ElectionProperties<Runtime, SolanaInstance>, _>(block_hash)
		.await
		.expect("Should always exist");

	// ===== Query BitmapComponents at X-1 (existing shared votes) =====
	let bitcoin_bitmap_components: BTreeMap<UniqueMonotonicIdentifier, BitcoinElectionBitmapComponents> = client
		.storage_map::<pallet_cf_elections::BitmapComponents<Runtime, BitcoinInstance>, _>(last_block_hash)
		.await
		.expect("Should always exist");
	let solana_bitmap_components: BTreeMap<UniqueMonotonicIdentifier, SolanaElectionBitmapComponents> = client
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
	let mut bitcoin_storage_votes: BTreeMap<ElectionIdentifier<BitcoinElectionIdentifierExtra>, Vec<(BitcoinBitmapComponent, u32)>> = BTreeMap::new();
	let mut solana_storage_votes: BTreeMap<ElectionIdentifier<SolanaElectionIdentifierExtra>, Vec<(SolanaBitmapComponent, u32)>> = BTreeMap::new();

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
	let bitcoin_individual_components: Vec<_> = client
		.storage_double_map::<pallet_cf_elections::IndividualComponents::<Runtime, BitcoinInstance>, Vec<_>>(last_block_hash)
		.await
		.expect("Should always exist");
	let solana_individual_components: Vec<_> = client
		.storage_double_map::<pallet_cf_elections::IndividualComponents::<Runtime, SolanaInstance>, Vec<_>>(last_block_hash)
		.await
		.expect("Should always exist");

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
	let signed_block = client.base_rpc_client.block(block_hash).await.unwrap();
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
										let partial = <SolanaVoteStorageTuple as VoteStorage>::vote_into_partial_vote(&full_vote, |shared_data| {
											let hash = SharedDataHash::of(&shared_data);
											solana_shared_data_map.insert(hash, shared_data);
											hash
										});
										solana_extrinsic_votes.entry(election_id).or_default()
											.entry(partial.clone()).and_modify(|entry| *entry += 1).or_insert(1);
									},
								}
							}
						},
						pallet_cf_elections::Call::provide_shared_data { shared_data } => {
							let shared_data_hash = SharedDataHash::of(&shared_data);
							solana_shared_data_map.insert(shared_data_hash, *shared_data);
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
										let partial = <BitcoinVoteStorageTuple as VoteStorage>::vote_into_partial_vote(&full_vote, |shared_data| {
											let hash = SharedDataHash::of(&shared_data);
											bitcoin_shared_data_map.insert(hash, shared_data);
											hash
										});
										bitcoin_extrinsic_votes.entry(election_id).or_default()
											.entry(partial.clone()).and_modify(|entry| *entry += 1).or_insert(1);
									},
								}
							}
						},
						pallet_cf_elections::Call::provide_shared_data { shared_data } => {
							let shared_data_hash = SharedDataHash::of(&shared_data);
							bitcoin_shared_data_map.insert(shared_data_hash, *shared_data);
						},
						_ => {},
					}
				},
				_ => {},
			}
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

	let btc_active = bitcoin_elections_prev.len();
	let btc_completed_count = bitcoin_completed.len();
	let sol_active = solana_elections_prev.len();
	let sol_completed_count = solana_completed.len();

	// ===== Build dashboard update =====
	let btc_election_summaries: Vec<ElectionSummary> = bitcoin_elections_prev.iter().map(|(election_id, props)| {
		let is_completed = bitcoin_completed.contains(election_id);
		let bitmap_votes = bitcoin_storage_votes.get(election_id)
			.map(|votes| votes.iter().map(|(comp, count)| VoteGroup {
				component: format_bitcoin_bitmap_component(comp, &bitcoin_shared_data_map),
				count: *count,
			}).collect())
			.unwrap_or_default();
		let individual_votes = bitcoin_individual_votes.get(election_id)
			.map(|votes| votes.iter().map(|(comp, count)| VoteGroup {
				component: format_for_display(&format!("{:?}", comp)),
				count: *count,
			}).collect())
			.unwrap_or_default();
		let extrinsic_votes = bitcoin_extrinsic_votes.get(election_id)
			.map(|votes| votes.iter().map(|(partial, count)| {
				let detail = format_bitcoin_vote_detail(partial, &bitcoin_shared_data_map);
				ExtrinsicVoteGroup { detail, count: *count }
			}).collect())
			.unwrap_or_default();
		ElectionSummary {
			election_type: bitcoin_election_type_name(election_id).to_string(),
			composite_id: format!("{:?}", election_id.extra()),
			election_properties: format_for_display(&format!("{:?}", props)),
			completed: is_completed,
			bitmap_votes,
			individual_votes,
			extrinsic_votes,
		}
	}).collect();

	let sol_election_summaries: Vec<ElectionSummary> = solana_elections_prev.iter().map(|(election_id, props)| {
		let is_completed = solana_completed.contains(election_id);
		let bitmap_votes = solana_storage_votes.get(election_id)
			.map(|votes| votes.iter().map(|(comp, count)| VoteGroup {
				component: format_solana_bitmap_component(comp, &solana_shared_data_map),
				count: *count,
			}).collect())
			.unwrap_or_default();
		let individual_votes = solana_individual_votes.get(election_id)
			.map(|votes| votes.iter().map(|(comp, count)| VoteGroup {
				component: format_for_display(&format!("{:?}", comp)),
				count: *count,
			}).collect())
			.unwrap_or_default();
		let extrinsic_votes = solana_extrinsic_votes.get(election_id)
			.map(|votes| votes.iter().map(|(partial, count)| {
				let detail = format_solana_vote_detail(partial, &solana_shared_data_map);
				ExtrinsicVoteGroup { detail, count: *count }
			}).collect())
			.unwrap_or_default();
		ElectionSummary {
			election_type: solana_election_type_name(election_id).to_string(),
			composite_id: format!("{:?}", election_id.extra()),
			election_properties: format_for_display(&format!("{:?}", props)),
			completed: is_completed,
			bitmap_votes,
			individual_votes,
			extrinsic_votes,
		}
	}).collect();

	Ok(BlockUpdate {
		block_number,
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
	})
}

fn print_block_summary(update: &BlockUpdate) {
	let prev = update.block_number.saturating_sub(1);

	println!("\nBITCOIN ELECTIONS ({} active, {} completed this block):",
		update.bitcoin.active_count, update.bitcoin.completed_count);
	for el in &update.bitcoin.elections {
		let status = if el.completed { " [COMPLETED]" } else { "" };
		println!("  [{}] {} {}{}", el.election_type, el.composite_id, el.election_properties, status);
		if !el.bitmap_votes.is_empty() {
			println!("    Bitmap votes (from storage at block {}):", prev);
			for v in &el.bitmap_votes {
				println!("      {}: {} votes", v.component, v.count);
			}
		}
		if !el.individual_votes.is_empty() {
			println!("    Individual votes (from storage at block {}):", prev);
			for v in &el.individual_votes {
				println!("      {}: {} votes", v.component, v.count);
			}
		}
		if !el.extrinsic_votes.is_empty() {
			println!("    New votes this block:");
			for v in &el.extrinsic_votes {
				println!("      {}: {} votes", v.detail, v.count);
			}
		}
	}

	println!("\nSOLANA ELECTIONS ({} active, {} completed this block):",
		update.solana.active_count, update.solana.completed_count);
	for el in &update.solana.elections {
		let status = if el.completed { " [COMPLETED]" } else { "" };
		println!("  [{}] {} {}{}", el.election_type, el.composite_id, el.election_properties, status);
		if !el.bitmap_votes.is_empty() {
			println!("    Bitmap votes (from storage at block {}):", prev);
			for v in &el.bitmap_votes {
				println!("      {}: {} votes", v.component, v.count);
			}
		}
		if !el.individual_votes.is_empty() {
			println!("    Individual votes (from storage at block {}):", prev);
			for v in &el.individual_votes {
				println!("      {}: {} votes", v.component, v.count);
			}
		}
		if !el.extrinsic_votes.is_empty() {
			println!("    New votes this block:");
			for v in &el.extrinsic_votes {
				println!("      {}: {} votes", v.detail, v.count);
			}
		}
	}

	println!("\n{}", "=".repeat(80));
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
		// Connect client first (needed by both live stream and REST endpoint)
		let (finalized_stream, _unfinalized_stream, client) =
			StateChainClient::connect_without_account(scope, &rpc_url).await.unwrap();

		// Block query channel for REST endpoint
		let (block_query_tx, mut block_query_rx) = mpsc::channel::<BlockQueryRequest>(8);

		// Spawn block query handler
		let query_client = client.clone();
		scope.spawn_weak(async move {
			while let Some((block_number, reply_tx)) = block_query_rx.recv().await {
				let result = match query_client.base_rpc_client.block_hash(block_number).await {
					Ok(Some(block_hash)) =>
						match build_block_update(&query_client, block_number, block_hash).await {
							Ok(update) => serde_json::to_string(&update)
								.map_err(|e| format!("Serialization error: {:?}", e)),
							Err(e) => Err(e),
						},
					Ok(None) => Err(format!("Block {} not found", block_number)),
					Err(e) => Err(format!("RPC error: {:?}", e)),
				};
				let _ = reply_tx.send(result);
			}
			Ok(())
		});

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
			run_dashboard(dashboard_addr, ws_tx_for_server, block_query_tx).await;
			Ok(())
		});

		finalized_stream
			.for_each(|block_info| {
				let client = client.clone();
				let ws_tx = ws_tx.clone();
				async move {
					println!("\n{:=^80}", format!(" Block {} ", block_info.number));
					match build_block_update(&client, block_info.number, block_info.hash).await {
						Ok(update) => {
							print_block_summary(&update);
							if let Ok(json) = serde_json::to_string(&update) {
								let _ = ws_tx.send(json);
							}
						},
						Err(e) => {
							println!("Error processing block {}: {}", block_info.number, e);
						},
					}
				}
			})
			.await;

		Ok(())

	}.boxed())
	.await
	.unwrap()
}

fn strip_composite_prefix(s: &str) -> String {
	for prefix in ["EE(", "FF(", "A(", "B(", "C(", "D(", "G("] {
		if s.starts_with(prefix) && s.ends_with(')') {
			let inner = &s[prefix.len()..s.len() - 1];
			// Verify parentheses are balanced in the inner content
			let mut depth = 0i32;
			let balanced = inner.chars().all(|ch| {
				match ch {
					'(' => depth += 1,
					')' => {
						depth -= 1;
						if depth < 0 {
							return false;
						}
					},
					_ => {},
				}
				true
			}) && depth == 0;
			if balanced {
				return inner.to_string();
			}
		}
	}
	s.to_string()
}

/// Convert byte array patterns like `[186, 23, 45, ...]` to hex `0xba172d...`
fn bytes_to_hex(s: &str) -> String {
	let chars: Vec<char> = s.chars().collect();
	let mut result = String::with_capacity(s.len());
	let mut i = 0;

	while i < chars.len() {
		if chars[i] == '[' {
			let start = i;
			i += 1;
			let content_start = i;
			let mut depth = 1;
			while i < chars.len() && depth > 0 {
				match chars[i] {
					'[' => depth += 1,
					']' => depth -= 1,
					_ => {},
				}
				if depth > 0 {
					i += 1;
				}
			}
			if depth == 0 {
				let content: String = chars[content_start..i].iter().collect();
				i += 1; // skip ']'

				let mut bytes = Vec::new();
				let mut valid = !content.trim().is_empty();
				if valid {
					for part in content.split(',') {
						match part.trim().parse::<u16>() {
							Ok(n) if n <= 255 => bytes.push(n as u8),
							_ => {
								valid = false;
								break;
							},
						}
					}
				}

				if valid && bytes.len() >= 8 {
					result.push_str("0x");
					for b in &bytes {
						result.push_str(&format!("{:02x}", b));
					}
				} else {
					result.push('[');
					result.push_str(&content);
					result.push(']');
				}
			} else {
				// Unbalanced brackets
				let remainder: String = chars[start..i].iter().collect();
				result.push_str(&remainder);
			}
		} else {
			result.push(chars[i]);
			i += 1;
		}
	}

	result
}

/// Strip composite prefix and convert byte arrays to hex
fn format_for_display(debug_str: &str) -> String {
	bytes_to_hex(&strip_composite_prefix(debug_str))
}

fn resolve_shared_data_value(
	shared_data_hash: &SharedDataHash,
	shared_data_map: &BTreeMap<SharedDataHash, impl std::fmt::Debug>,
) -> String {
	match shared_data_map.get(shared_data_hash) {
		Some(value) => format_for_display(&format!("{:?}", value)),
		None => format!("MissingSharedData({:?})", shared_data_hash),
	}
}

fn format_bitcoin_bitmap_component(
	component: &BitcoinBitmapComponent,
	shared_data_map: &BTreeMap<SharedDataHash, impl std::fmt::Debug>,
) -> String {
	use pallet_cf_elections::vote_storage::composite::tuple_6_impls::CompositeBitmapComponent;
	match component {
		CompositeBitmapComponent::A(hash) => resolve_shared_data_value(hash, shared_data_map),
		CompositeBitmapComponent::B(hash) => resolve_shared_data_value(hash, shared_data_map),
		CompositeBitmapComponent::C(hash) => resolve_shared_data_value(hash, shared_data_map),
		CompositeBitmapComponent::D(hash) => resolve_shared_data_value(hash, shared_data_map),
		CompositeBitmapComponent::EE(value) => bytes_to_hex(&format!("{:?}", value)),
		CompositeBitmapComponent::FF(value) => bytes_to_hex(&format!("{:?}", value)),
	}
}

fn format_solana_bitmap_component(
	component: &SolanaBitmapComponent,
	shared_data_map: &BTreeMap<SharedDataHash, impl std::fmt::Debug>,
) -> String {
	use pallet_cf_elections::vote_storage::composite::tuple_7_impls::CompositeBitmapComponent;
	match component {
		CompositeBitmapComponent::A(value) => bytes_to_hex(&format!("{:?}", value)),
		CompositeBitmapComponent::B(value) => bytes_to_hex(&format!("{:?}", value)),
		CompositeBitmapComponent::C(hash) => resolve_shared_data_value(hash, shared_data_map),
		CompositeBitmapComponent::D(value) => bytes_to_hex(&format!("{:?}", value)),
		CompositeBitmapComponent::EE(value) => bytes_to_hex(&format!("{:?}", value)),
		CompositeBitmapComponent::FF(hash) => resolve_shared_data_value(hash, shared_data_map),
		CompositeBitmapComponent::G(hash) => resolve_shared_data_value(hash, shared_data_map),
	}
}

fn format_bitcoin_vote_detail(
	partial: &<BitcoinVoteStorageTuple as VoteStorage>::PartialVote,
	shared_data_map: &BTreeMap<SharedDataHash, impl std::fmt::Debug>,
) -> String {
	use pallet_cf_elections::vote_storage::composite::tuple_6_impls::CompositePartialVote;
	match partial {
		CompositePartialVote::A(inner) => resolve_shared_data_value(inner, shared_data_map),
		CompositePartialVote::B(inner) => resolve_shared_data_value(inner, shared_data_map),
		CompositePartialVote::C(inner) => resolve_shared_data_value(inner, shared_data_map),
		CompositePartialVote::D(inner) => resolve_shared_data_value(inner, shared_data_map),
		CompositePartialVote::EE(inner) => bytes_to_hex(&format!("{:?}", inner)),
		CompositePartialVote::FF(inner) => bytes_to_hex(&format!("{:?}", inner)),
	}
}

fn format_solana_vote_detail(
	partial: &<SolanaVoteStorageTuple as VoteStorage>::PartialVote,
	shared_data_map: &BTreeMap<SharedDataHash, impl std::fmt::Debug>,
) -> String {
	use pallet_cf_elections::vote_storage::composite::tuple_7_impls::CompositePartialVote;
	match partial {
		CompositePartialVote::A(inner) => bytes_to_hex(&format!("{:?}", inner)),
		CompositePartialVote::B(inner) => bytes_to_hex(&format!("{:?}", inner)),
		CompositePartialVote::C(inner) =>
			format!("{} Slot {:?}", resolve_shared_data_value(&inner.value, shared_data_map), inner.block),
		CompositePartialVote::D(inner) => bytes_to_hex(&format!("{:?}", inner)),
		CompositePartialVote::EE(inner) => bytes_to_hex(&format!("{:?}", inner)),
		CompositePartialVote::FF(inner) =>
			resolve_shared_data_value(inner, shared_data_map),
		CompositePartialVote::G(inner) =>
			resolve_shared_data_value(inner, shared_data_map),
	}
}
