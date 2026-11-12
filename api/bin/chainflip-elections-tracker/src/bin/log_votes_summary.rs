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
use engine_sc_client::{StateChainClient, base_rpc_api::BaseRpcApi};

#[path = "../paged_storage.rs"]
mod paged_storage;
use futures::{SinkExt, StreamExt};
use futures_util::FutureExt;
use pallet_cf_elections::{
	ElectionIdentifier, ElectionIdentifierOf, ElectoralSystemTypes, PartialVoteOf, SharedDataHash,
	VoteStorageOf,
	bitmap_components::ElectionBitmapComponents,
	vote_storage::{AuthorityVote, VoteStorage},
};
use serde::Serialize;
use state_chain_runtime::{
	ArbitrumInstance, AssethubInstance, BitcoinInstance, BscInstance, EthereumInstance, Runtime,
	RuntimeCall, SolanaInstance, TronInstance,
	chainflip::witnessing::{
		arbitrum_elections::ArbitrumElectoralSystemRunner,
		assethub_elections::AssethubElectoralSystemRunner,
		bitcoin_elections::BitcoinElectoralSystemRunner, bsc_elections::BscElectoralSystemRunner,
		ethereum_elections::EthereumElectoralSystemRunner,
		solana_elections::SolanaElectoralSystemRunner, tron_elections::TronElectoralSystemRunner,
	},
};
use std::{
	env,
	net::{IpAddr, SocketAddr},
	time::Duration,
};
use tokio::sync::{broadcast, mpsc, oneshot};
use warp::Filter;

type BitcoinVoteStorageTuple = <BitcoinElectoralSystemRunner as ElectoralSystemTypes>::VoteStorage;
type BitcoinElectionIdentifierExtra =
	<BitcoinElectoralSystemRunner as ElectoralSystemTypes>::ElectionIdentifierExtra;
type BitcoinBitmapComponent = <BitcoinVoteStorageTuple as VoteStorage>::BitmapComponent;

type SolanaVoteStorageTuple = <SolanaElectoralSystemRunner as ElectoralSystemTypes>::VoteStorage;
type SolanaElectionIdentifierExtra =
	<SolanaElectoralSystemRunner as ElectoralSystemTypes>::ElectionIdentifierExtra;
type SolanaBitmapComponent = <SolanaVoteStorageTuple as VoteStorage>::BitmapComponent;

type EthereumVoteStorageTuple =
	<EthereumElectoralSystemRunner as ElectoralSystemTypes>::VoteStorage;
type EthereumElectionIdentifierExtra =
	<EthereumElectoralSystemRunner as ElectoralSystemTypes>::ElectionIdentifierExtra;
type EthereumBitmapComponent = <EthereumVoteStorageTuple as VoteStorage>::BitmapComponent;

type ArbitrumVoteStorageTuple =
	<ArbitrumElectoralSystemRunner as ElectoralSystemTypes>::VoteStorage;
type ArbitrumElectionIdentifierExtra =
	<ArbitrumElectoralSystemRunner as ElectoralSystemTypes>::ElectionIdentifierExtra;
type ArbitrumBitmapComponent = <ArbitrumVoteStorageTuple as VoteStorage>::BitmapComponent;

type TronVoteStorageTuple = <TronElectoralSystemRunner as ElectoralSystemTypes>::VoteStorage;
type TronElectionIdentifierExtra =
	<TronElectoralSystemRunner as ElectoralSystemTypes>::ElectionIdentifierExtra;
type TronBitmapComponent = <TronVoteStorageTuple as VoteStorage>::BitmapComponent;

type BscVoteStorageTuple = <BscElectoralSystemRunner as ElectoralSystemTypes>::VoteStorage;
type BscElectionIdentifierExtra =
	<BscElectoralSystemRunner as ElectoralSystemTypes>::ElectionIdentifierExtra;
type BscBitmapComponent = <BscVoteStorageTuple as VoteStorage>::BitmapComponent;

type AssethubVoteStorageTuple =
	<AssethubElectoralSystemRunner as ElectoralSystemTypes>::VoteStorage;
type AssethubElectionIdentifierExtra =
	<AssethubElectoralSystemRunner as ElectoralSystemTypes>::ElectionIdentifierExtra;
type AssethubBitmapComponent = <AssethubVoteStorageTuple as VoteStorage>::BitmapComponent;

type ElectoralSystem<I> = <Runtime as pallet_cf_elections::Config<I>>::ElectoralSystemRunner;
type ChainVoteStorage<I> = VoteStorageOf<ElectoralSystem<I>>;
type SharedDataMap<I> = BTreeMap<SharedDataHash, <ChainVoteStorage<I> as VoteStorage>::SharedData>;
type ExtrinsicVotes<I> = BTreeMap<
	ElectionIdentifierOf<ElectoralSystem<I>>,
	BTreeMap<PartialVoteOf<ElectoralSystem<I>>, u32>,
>;

// ===== Dashboard types (JSON-serializable for the web UI) =====

#[derive(Serialize, Clone)]
struct BlockUpdate {
	block_number: u32,
	bitcoin: ChainElections,
	solana: ChainElections,
	ethereum: ChainElections,
	arbitrum: ChainElections,
	tron: ChainElections,
	bsc: ChainElections,
	assethub: ChainElections,
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

fn json_response(
	status: warp::http::StatusCode,
	body: serde_json::Value,
) -> warp::http::Response<String> {
	warp::http::Response::builder()
		.status(status)
		.header("content-type", "application/json")
		.body(body.to_string())
		.unwrap_or_else(|_| {
			warp::http::Response::new(r#"{"error":"Internal response error"}"#.to_string())
		})
}

fn websocket_error_message(block_number: u32, error: &str) -> String {
	serde_json::json!({
		"type": "error",
		"block_number": block_number,
		"error": error,
	})
	.to_string()
}

async fn run_dashboard(
	bind_addr: SocketAddr,
	tx: broadcast::Sender<String>,
	block_query_tx: BlockQuerySender,
) {
	let index = warp::path::end()
		.and(warp::get())
		.map(|| warp::reply::html(DASHBOARD_HTML.to_string()));

	// Path segments are owned `String`s (and `warp::path!` is expanded by hand) because
	// `&'static str` segments trip higher-ranked lifetime inference in the spawned task
	// ("implementation of `AsRef` is not general enough").
	let ws_route = warp::path("ws".to_string()).and(warp::ws()).map(move |ws: warp::ws::Ws| {
		let rx = tx.subscribe();
		ws.on_upgrade(move |websocket| handle_ws_client(websocket, rx))
	});

	let block_query = warp::path("api".to_string())
		.and(warp::path("block".to_string()))
		.and(warp::path::param::<u32>())
		.and(warp::path::end())
		.and(warp::get())
		.then(move |number: u32| {
			let tx = block_query_tx.clone();
			async move {
				let (reply_tx, reply_rx) = oneshot::channel();
				if tx.send((number, reply_tx)).await.is_err() {
					return json_response(
						warp::http::StatusCode::INTERNAL_SERVER_ERROR,
						serde_json::json!({ "error": "Internal error" }),
					);
				}
				match reply_rx.await {
					Ok(Ok(json)) => match serde_json::from_str(&json) {
						Ok(parsed) => json_response(warp::http::StatusCode::OK, parsed),
						Err(_) => json_response(
							warp::http::StatusCode::INTERNAL_SERVER_ERROR,
							serde_json::json!({ "error": "Invalid response payload" }),
						),
					},
					Ok(Err(e)) => json_response(
						warp::http::StatusCode::BAD_REQUEST,
						serde_json::json!({ "error": e }),
					),
					Err(_) => json_response(
						warp::http::StatusCode::INTERNAL_SERVER_ERROR,
						serde_json::json!({ "error": "Query failed" }),
					),
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
	tuple_5_impls::CompositeElectionIdentifierExtra as Composite5Extra,
	tuple_6_impls::CompositeElectionIdentifierExtra as Composite6Extra,
	tuple_7_impls::CompositeElectionIdentifierExtra as Composite7Extra,
	tuple_8_impls::CompositeElectionIdentifierExtra as Composite8Extra,
};

fn bitcoin_election_type_name(
	election_id: &ElectionIdentifier<BitcoinElectionIdentifierExtra>,
) -> &'static str {
	match election_id.extra() {
		Composite6Extra::A(_) => "BlockHeight",
		Composite6Extra::B(_) => "DepositChannel",
		Composite6Extra::C(_) => "VaultDeposit",
		Composite6Extra::D(_) => "Egress",
		Composite6Extra::EE(_) => "FeeTracking",
		Composite6Extra::FF(_) => "Liveness",
	}
}

fn solana_election_type_name(
	election_id: &ElectionIdentifier<SolanaElectionIdentifierExtra>,
) -> &'static str {
	match election_id.extra() {
		Composite7Extra::A(_) => "BlockHeight",
		Composite7Extra::B(_) => "Ingress",
		Composite7Extra::C(_) => "Nonce",
		Composite7Extra::D(_) => "Egress",
		Composite7Extra::EE(_) => "Liveness",
		Composite7Extra::FF(_) => "VaultSwap",
		Composite7Extra::G(_) => "ALT",
	}
}

fn ethereum_election_type_name(
	election_id: &ElectionIdentifier<EthereumElectionIdentifierExtra>,
) -> &'static str {
	match election_id.extra() {
		Composite8Extra::A(_) => "BlockHeight",
		Composite8Extra::B(_) => "DepositChannel",
		Composite8Extra::C(_) => "VaultDeposit",
		Composite8Extra::D(_) => "KeyManager",
		Composite8Extra::EE(_) => "FeeTracking",
		Composite8Extra::FF(_) => "Liveness",
		Composite8Extra::G(_) => "StateChainGateway",
		Composite8Extra::HH(_) => "ScUtils",
	}
}

fn arbitrum_election_type_name(
	election_id: &ElectionIdentifier<ArbitrumElectionIdentifierExtra>,
) -> &'static str {
	match election_id.extra() {
		Composite6Extra::A(_) => "BlockHeight",
		Composite6Extra::B(_) => "DepositChannel",
		Composite6Extra::C(_) => "VaultDeposit",
		Composite6Extra::D(_) => "KeyManager",
		Composite6Extra::EE(_) => "FeeTracking",
		Composite6Extra::FF(_) => "Liveness",
	}
}

fn tron_election_type_name(
	election_id: &ElectionIdentifier<TronElectionIdentifierExtra>,
) -> &'static str {
	match election_id.extra() {
		Composite5Extra::A(_) => "BlockHeight",
		Composite5Extra::B(_) => "DepositChannel",
		Composite5Extra::C(_) => "VaultDeposit",
		Composite5Extra::D(_) => "KeyManager",
		Composite5Extra::EE(_) => "Liveness",
	}
}

fn bsc_election_type_name(
	election_id: &ElectionIdentifier<BscElectionIdentifierExtra>,
) -> &'static str {
	match election_id.extra() {
		Composite6Extra::A(_) => "BlockHeight",
		Composite6Extra::B(_) => "DepositChannel",
		Composite6Extra::C(_) => "VaultDeposit",
		Composite6Extra::D(_) => "KeyManager",
		Composite6Extra::EE(_) => "FeeTracking",
		Composite6Extra::FF(_) => "Liveness",
	}
}

fn assethub_election_type_name(
	election_id: &ElectionIdentifier<AssethubElectionIdentifierExtra>,
) -> &'static str {
	match election_id.extra() {
		Composite5Extra::A(_) => "BlockHeight",
		Composite5Extra::B(_) => "DepositChannel",
		Composite5Extra::C(_) => "Egress",
		Composite5Extra::D(_) => "FeeTracking",
		Composite5Extra::EE(_) => "Liveness",
	}
}

fn election_calls(call: RuntimeCall) -> Vec<RuntimeCall> {
	match call {
		RuntimeCall::Environment(pallet_cf_environment::Call::submit_elections_votes { votes }) => {
			let mut calls = Vec::new();
			macro_rules! instance_votes {
				($field:ident, $variant:ident) => {
					if let Some(authority_votes) = votes.$field {
						calls.push(RuntimeCall::$variant(pallet_cf_elections::Call::vote {
							authority_votes,
						}));
					}
				};
			}
			instance_votes!(bitcoin, BitcoinElections);
			instance_votes!(solana, SolanaElections);
			instance_votes!(ethereum, EthereumElections);
			instance_votes!(arbitrum, ArbitrumElections);
			instance_votes!(tron, TronElections);
			instance_votes!(bsc, BscElections);
			instance_votes!(assethub, AssethubElections);
			calls
		},
		RuntimeCall::Environment(pallet_cf_environment::Call::batch { calls }) =>
			calls.into_iter().flat_map(election_calls).collect(),
		_ => vec![call],
	}
}

fn collect_election_votes<'a, I: 'static>(
	calls: impl IntoIterator<Item = &'a pallet_cf_elections::Call<Runtime, I>>,
	shared_data: &mut SharedDataMap<I>,
) -> ExtrinsicVotes<I>
where
	Runtime: pallet_cf_elections::Config<I>,
	PartialVoteOf<ElectoralSystem<I>>: Ord,
{
	let mut votes: ExtrinsicVotes<I> = BTreeMap::new();
	for call in calls {
		match call {
			pallet_cf_elections::Call::vote { authority_votes } => {
				for (election_id, vote) in authority_votes.iter() {
					let partial = match vote {
						AuthorityVote::PartialVote(partial) => partial.clone(),
						AuthorityVote::Vote(full_vote) =>
							ChainVoteStorage::<I>::vote_into_partial_vote(full_vote, |data| {
								let hash = SharedDataHash::of(&data);
								shared_data.insert(hash, data);
								hash
							}),
					};
					*votes.entry(*election_id).or_default().entry(partial).or_default() += 1;
				}
			},
			pallet_cf_elections::Call::provide_shared_data { shared_data: data } => {
				shared_data.insert(SharedDataHash::of(data.as_ref()), data.as_ref().clone());
			},
			_ => {},
		}
	}
	votes
}

struct ChainFormat<I: 'static>
where
	Runtime: pallet_cf_elections::Config<I>,
{
	name: &'static str,
	select_call: fn(&RuntimeCall) -> Option<&pallet_cf_elections::Call<Runtime, I>>,
	election_type: fn(&ElectionIdentifierOf<ElectoralSystem<I>>) -> &'static str,
	bitmap_component:
		fn(&<ChainVoteStorage<I> as VoteStorage>::BitmapComponent, &SharedDataMap<I>) -> String,
	vote_detail: fn(&PartialVoteOf<ElectoralSystem<I>>, &SharedDataMap<I>) -> String,
}

async fn build_chain_elections<I: 'static>(
	client: &StateChainClient<()>,
	block_number: u32,
	block_hash: state_chain_runtime::Hash,
	previous_block_hash: state_chain_runtime::Hash,
	calls: &[RuntimeCall],
	format: ChainFormat<I>,
) -> Result<ChainElections, String>
where
	Runtime: pallet_cf_elections::Config<I>,
	PartialVoteOf<ElectoralSystem<I>>: Ord,
{
	let previous_block_number = block_number.saturating_sub(1);
	let mut shared_data =
		paged_storage::storage_map::<pallet_cf_elections::SharedData<Runtime, I>>(
			client.base_rpc_client.as_ref(),
			block_hash,
		)
		.await
		.map_err(|e| {
			format!(
				"Failed to fetch {} shared data at block {}: {:?}",
				format.name, block_number, e
			)
		})?;
	shared_data.extend(
		paged_storage::storage_map::<pallet_cf_elections::SharedData<Runtime, I>>(
			client.base_rpc_client.as_ref(),
			previous_block_hash,
		)
		.await
		.map_err(|e| {
			format!(
				"Failed to fetch {} shared data at block {}: {:?}",
				format.name, previous_block_number, e
			)
		})?,
	);
	let elections_prev = paged_storage::storage_map::<
		pallet_cf_elections::ElectionProperties<Runtime, I>,
	>(client.base_rpc_client.as_ref(), previous_block_hash)
	.await
	.map_err(|e| {
		format!(
			"Failed to fetch {} election properties at block {}: {:?}",
			format.name, previous_block_number, e
		)
	})?;
	let elections_curr = paged_storage::storage_map::<
		pallet_cf_elections::ElectionProperties<Runtime, I>,
	>(client.base_rpc_client.as_ref(), block_hash)
	.await
	.map_err(|e| {
		format!(
			"Failed to fetch {} election properties at block {}: {:?}",
			format.name, block_number, e
		)
	})?;
	let bitmap_components: BTreeMap<_, ElectionBitmapComponents<Runtime, I>> =
		paged_storage::storage_map::<pallet_cf_elections::BitmapComponents<Runtime, I>>(
			client.base_rpc_client.as_ref(),
			previous_block_hash,
		)
		.await
		.map_err(|e| {
			format!(
				"Failed to fetch {} bitmap components at block {}: {:?}",
				format.name, previous_block_number, e
			)
		})?;
	let individual_components = paged_storage::storage_double_map::<
		pallet_cf_elections::IndividualComponents<Runtime, I>,
	>(client.base_rpc_client.as_ref(), previous_block_hash)
	.await
	.map_err(|e| {
		format!(
			"Failed to fetch {} individual components at block {}: {:?}",
			format.name, previous_block_number, e
		)
	})?;

	let mut individual_votes: BTreeMap<_, Vec<(_, u32)>> = BTreeMap::new();
	for ((unique_id, _validator), (_properties, component)) in individual_components {
		let votes = individual_votes.entry(unique_id).or_default();
		if let Some((_, count)) = votes.iter_mut().find(|(c, _)| c == &component) {
			*count += 1;
		} else {
			votes.push((component, 1));
		}
	}
	let extrinsic_votes =
		collect_election_votes::<I>(calls.iter().filter_map(format.select_call), &mut shared_data);
	let elections: Vec<_> = elections_prev
		.iter()
		.map(|(election_id, properties)| {
			let unique_id = election_id.unique_monotonic();
			ElectionSummary {
				election_type: (format.election_type)(election_id).to_string(),
				composite_id: format!("{:?}", election_id.extra()),
				election_properties: format_for_display(&format!("{:?}", properties)),
				completed: !elections_curr.contains_key(election_id),
				bitmap_votes: bitmap_components
					.get(unique_id)
					.into_iter()
					.flat_map(|data| &data.bitmaps)
					.map(|(component, bitmap)| VoteGroup {
						component: (format.bitmap_component)(component, &shared_data),
						count: bitmap.count_ones() as u32,
					})
					.collect(),
				individual_votes: individual_votes
					.get(unique_id)
					.into_iter()
					.flatten()
					.map(|(component, count)| VoteGroup {
						component: format_for_display(&format!("{:?}", component)),
						count: *count,
					})
					.collect(),
				extrinsic_votes: extrinsic_votes
					.get(election_id)
					.into_iter()
					.flatten()
					.map(|(partial, count)| ExtrinsicVoteGroup {
						detail: (format.vote_detail)(partial, &shared_data),
						count: *count,
					})
					.collect(),
			}
		})
		.collect();
	Ok(ChainElections {
		active_count: elections_prev.len(),
		completed_count: elections.iter().filter(|e| e.completed).count(),
		elections,
	})
}

async fn build_block_update(
	client: &StateChainClient<()>,
	block_number: u32,
	block_hash: state_chain_runtime::Hash,
) -> Result<BlockUpdate, String> {
	let previous_block_number =
		block_number.checked_sub(1).ok_or_else(|| "No previous block".to_string())?;
	let previous_block_hash = client
		.base_rpc_client
		.block_hash(previous_block_number)
		.await
		.map_err(|e| format!("RPC error fetching block {} hash: {:?}", previous_block_number, e))?
		.ok_or_else(|| format!("Block {} not found", previous_block_number))?;
	let signed_block = client
		.base_rpc_client
		.block(block_hash)
		.await
		.map_err(|e| format!("Failed to fetch block {} extrinsics: {:?}", block_number, e))?
		.ok_or_else(|| format!("Block {} not found when fetching extrinsics", block_number))?;
	let calls: Vec<_> = signed_block
		.block
		.extrinsics
		.into_iter()
		.flat_map(|ex| election_calls(ex.function))
		.collect();

	macro_rules! chain_elections {
		($name:literal, $instance:ty, $variant:ident, $election_type:ident, $bitmap:ident, $vote:ident) => {
			build_chain_elections::<$instance>(
				client,
				block_number,
				block_hash,
				previous_block_hash,
				&calls,
				ChainFormat {
					name: $name,
					select_call: |call| match call {
						RuntimeCall::$variant(call) => Some(call),
						_ => None,
					},
					election_type: $election_type,
					bitmap_component: $bitmap,
					vote_detail: $vote,
				},
			)
		};
	}

	let (bitcoin, solana, ethereum, arbitrum, tron, bsc, assethub) = tokio::try_join!(
		chain_elections!(
			"Bitcoin",
			BitcoinInstance,
			BitcoinElections,
			bitcoin_election_type_name,
			format_bitcoin_bitmap_component,
			format_bitcoin_vote_detail
		),
		chain_elections!(
			"Solana",
			SolanaInstance,
			SolanaElections,
			solana_election_type_name,
			format_solana_bitmap_component,
			format_solana_vote_detail
		),
		chain_elections!(
			"Ethereum",
			EthereumInstance,
			EthereumElections,
			ethereum_election_type_name,
			format_ethereum_bitmap_component,
			format_ethereum_vote_detail
		),
		chain_elections!(
			"Arbitrum",
			ArbitrumInstance,
			ArbitrumElections,
			arbitrum_election_type_name,
			format_arbitrum_bitmap_component,
			format_arbitrum_vote_detail
		),
		chain_elections!(
			"Tron",
			TronInstance,
			TronElections,
			tron_election_type_name,
			format_tron_bitmap_component,
			format_tron_vote_detail
		),
		chain_elections!(
			"BSC",
			BscInstance,
			BscElections,
			bsc_election_type_name,
			format_bsc_bitmap_component,
			format_bsc_vote_detail
		),
		chain_elections!(
			"Assethub",
			AssethubInstance,
			AssethubElections,
			assethub_election_type_name,
			format_assethub_bitmap_component,
			format_assethub_vote_detail
		),
	)?;
	Ok(BlockUpdate { block_number, bitcoin, solana, ethereum, arbitrum, tron, bsc, assethub })
}

fn print_block_summary(update: &BlockUpdate) {
	let prev = update.block_number.saturating_sub(1);
	for (name, chain) in [
		("BITCOIN", &update.bitcoin),
		("SOLANA", &update.solana),
		("ETHEREUM", &update.ethereum),
		("ARBITRUM", &update.arbitrum),
		("TRON", &update.tron),
		("BSC", &update.bsc),
		("ASSETHUB", &update.assethub),
	] {
		println!(
			"\n{} ELECTIONS ({} active, {} completed this block):",
			name, chain.active_count, chain.completed_count
		);
		for el in &chain.elections {
			let status = if el.completed { " [COMPLETED]" } else { "" };
			println!(
				"  [{}] {} {}{}",
				el.election_type, el.composite_id, el.election_properties, status
			);
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
	}
	println!("\n{}", "=".repeat(80));
}

async fn observe_elections(rpc_url: String) {
	let (ws_tx, _) = broadcast::channel::<String>(32);

	task_scope::task_scope(|scope| {
		async move {
			// Connect client first (needed by both live stream and REST endpoint)
			let (finalized_stream, _unfinalized_stream, client) =
				match StateChainClient::connect_without_account(scope, &rpc_url).await {
					Ok(result) => result,
					Err(e) => {
						println!("Failed to connect to RPC node {}: {:?}", rpc_url, e);
						return Ok(());
					},
				};

			// Block query channel for REST endpoint
			let (block_query_tx, mut block_query_rx) = mpsc::channel::<BlockQueryRequest>(8);

			// Spawn block query handler
			let query_client = client.clone();
			scope.spawn_weak(async move {
				while let Some((block_number, reply_tx)) = block_query_rx.recv().await {
					let result = match query_client.base_rpc_client.block_hash(block_number).await {
						Ok(Some(block_hash)) =>
							match build_block_update(&query_client, block_number, block_hash).await
							{
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
			let dashboard_port: u16 =
				env::var("DASHBOARD_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(8080);
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
						match build_block_update(&client, block_info.number, block_info.hash).await
						{
							Ok(update) => {
								print_block_summary(&update);
								if let Ok(json) = serde_json::to_string(&update) {
									let _ = ws_tx.send(json);
								}
							},
							Err(e) => {
								println!("Error processing block {}: {}", block_info.number, e);
								let _ = ws_tx.send(websocket_error_message(block_info.number, &e));
							},
						}
					}
				})
				.await;

			Ok(())
		}
		.boxed()
	})
	.await
	.unwrap_or_else(|_| println!("Election observer task scope returned an error"));
}

fn strip_composite_prefix(s: &str) -> String {
	for prefix in ["EE(", "FF(", "HH(", "A(", "B(", "C(", "D(", "G("] {
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
		CompositePartialVote::C(inner) => format!(
			"{} Slot {:?}",
			resolve_shared_data_value(&inner.value, shared_data_map),
			inner.block
		),
		CompositePartialVote::D(inner) => bytes_to_hex(&format!("{:?}", inner)),
		CompositePartialVote::EE(inner) => bytes_to_hex(&format!("{:?}", inner)),
		CompositePartialVote::FF(inner) => resolve_shared_data_value(inner, shared_data_map),
		CompositePartialVote::G(inner) => resolve_shared_data_value(inner, shared_data_map),
	}
}

fn format_ethereum_bitmap_component(
	component: &EthereumBitmapComponent,
	shared_data_map: &BTreeMap<SharedDataHash, impl std::fmt::Debug>,
) -> String {
	use pallet_cf_elections::vote_storage::composite::tuple_8_impls::CompositeBitmapComponent;
	match component {
		CompositeBitmapComponent::A(hash) => resolve_shared_data_value(hash, shared_data_map),
		CompositeBitmapComponent::B(hash) => resolve_shared_data_value(hash, shared_data_map),
		CompositeBitmapComponent::C(hash) => resolve_shared_data_value(hash, shared_data_map),
		CompositeBitmapComponent::D(hash) => resolve_shared_data_value(hash, shared_data_map),
		CompositeBitmapComponent::EE(value) => bytes_to_hex(&format!("{:?}", value)),
		CompositeBitmapComponent::FF(value) => bytes_to_hex(&format!("{:?}", value)),
		CompositeBitmapComponent::G(hash) => resolve_shared_data_value(hash, shared_data_map),
		CompositeBitmapComponent::HH(hash) => resolve_shared_data_value(hash, shared_data_map),
	}
}

fn format_arbitrum_bitmap_component(
	component: &ArbitrumBitmapComponent,
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

fn format_bsc_bitmap_component(
	component: &BscBitmapComponent,
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

fn format_tron_bitmap_component(
	component: &TronBitmapComponent,
	shared_data_map: &BTreeMap<SharedDataHash, impl std::fmt::Debug>,
) -> String {
	use pallet_cf_elections::vote_storage::composite::tuple_5_impls::CompositeBitmapComponent;
	match component {
		CompositeBitmapComponent::A(hash) => resolve_shared_data_value(hash, shared_data_map),
		CompositeBitmapComponent::B(hash) => resolve_shared_data_value(hash, shared_data_map),
		CompositeBitmapComponent::C(hash) => resolve_shared_data_value(hash, shared_data_map),
		CompositeBitmapComponent::D(hash) => resolve_shared_data_value(hash, shared_data_map),
		CompositeBitmapComponent::EE(value) => bytes_to_hex(&format!("{:?}", value)),
	}
}

fn format_assethub_bitmap_component(
	component: &AssethubBitmapComponent,
	shared_data_map: &BTreeMap<SharedDataHash, impl std::fmt::Debug>,
) -> String {
	use pallet_cf_elections::vote_storage::composite::tuple_5_impls::CompositeBitmapComponent;
	match component {
		CompositeBitmapComponent::A(hash) => resolve_shared_data_value(hash, shared_data_map),
		CompositeBitmapComponent::B(hash) => resolve_shared_data_value(hash, shared_data_map),
		CompositeBitmapComponent::C(hash) => resolve_shared_data_value(hash, shared_data_map),
		CompositeBitmapComponent::D(value) => bytes_to_hex(&format!("{:?}", value)),
		CompositeBitmapComponent::EE(value) => bytes_to_hex(&format!("{:?}", value)),
	}
}

fn format_ethereum_vote_detail(
	partial: &<EthereumVoteStorageTuple as VoteStorage>::PartialVote,
	shared_data_map: &BTreeMap<SharedDataHash, impl std::fmt::Debug>,
) -> String {
	use pallet_cf_elections::vote_storage::composite::tuple_8_impls::CompositePartialVote;
	match partial {
		CompositePartialVote::A(inner) => resolve_shared_data_value(inner, shared_data_map),
		CompositePartialVote::B(inner) => resolve_shared_data_value(inner, shared_data_map),
		CompositePartialVote::C(inner) => resolve_shared_data_value(inner, shared_data_map),
		CompositePartialVote::D(inner) => resolve_shared_data_value(inner, shared_data_map),
		CompositePartialVote::EE(inner) => bytes_to_hex(&format!("{:?}", inner)),
		CompositePartialVote::FF(inner) => bytes_to_hex(&format!("{:?}", inner)),
		CompositePartialVote::G(inner) => resolve_shared_data_value(inner, shared_data_map),
		CompositePartialVote::HH(inner) => resolve_shared_data_value(inner, shared_data_map),
	}
}

fn format_arbitrum_vote_detail(
	partial: &<ArbitrumVoteStorageTuple as VoteStorage>::PartialVote,
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

fn format_bsc_vote_detail(
	partial: &<BscVoteStorageTuple as VoteStorage>::PartialVote,
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

fn format_tron_vote_detail(
	partial: &<TronVoteStorageTuple as VoteStorage>::PartialVote,
	shared_data_map: &BTreeMap<SharedDataHash, impl std::fmt::Debug>,
) -> String {
	use pallet_cf_elections::vote_storage::composite::tuple_5_impls::CompositePartialVote;
	match partial {
		CompositePartialVote::A(inner) => resolve_shared_data_value(inner, shared_data_map),
		CompositePartialVote::B(inner) => resolve_shared_data_value(inner, shared_data_map),
		CompositePartialVote::C(inner) => resolve_shared_data_value(inner, shared_data_map),
		CompositePartialVote::D(inner) => resolve_shared_data_value(inner, shared_data_map),
		CompositePartialVote::EE(inner) => bytes_to_hex(&format!("{:?}", inner)),
	}
}

fn format_assethub_vote_detail(
	partial: &<AssethubVoteStorageTuple as VoteStorage>::PartialVote,
	shared_data_map: &BTreeMap<SharedDataHash, impl std::fmt::Debug>,
) -> String {
	use pallet_cf_elections::vote_storage::composite::tuple_5_impls::CompositePartialVote;
	match partial {
		CompositePartialVote::A(inner) => resolve_shared_data_value(inner, shared_data_map),
		CompositePartialVote::B(inner) => resolve_shared_data_value(inner, shared_data_map),
		CompositePartialVote::C(inner) => resolve_shared_data_value(inner, shared_data_map),
		CompositePartialVote::D(inner) => bytes_to_hex(&format!("{:?}", inner)),
		CompositePartialVote::EE(inner) => bytes_to_hex(&format!("{:?}", inner)),
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use pallet_cf_elections::{
		AuthorityVoteOf,
		vote_storage::composite::{tuple_5_impls, tuple_6_impls, tuple_7_impls, tuple_8_impls},
	};
	use state_chain_runtime::chainflip::elections::AllElectionInstancesVotes;

	fn vote_call<I: 'static>(
		election_id: ElectionIdentifierOf<ElectoralSystem<I>>,
		vote: AuthorityVoteOf<ElectoralSystem<I>>,
	) -> pallet_cf_elections::Call<Runtime, I>
	where
		Runtime: pallet_cf_elections::Config<I>,
	{
		pallet_cf_elections::Call::vote {
			authority_votes: Box::new(BTreeMap::from([(election_id, vote)]).try_into().unwrap()),
		}
	}

	fn batch(calls: Vec<RuntimeCall>) -> RuntimeCall {
		RuntimeCall::Environment(pallet_cf_environment::Call::batch {
			calls: calls.try_into().unwrap(),
		})
	}

	#[test]
	fn counts_full_and_partial_votes_in_mixed_instance_batches() {
		let mut batched_votes = AllElectionInstancesVotes::default();
		let mut full_votes = Vec::new();
		macro_rules! liveness_vote {
			($field:ident, $instance:ty, $variant:ident, $tuple:ident, $extra:ident, $liveness:ident) => {{
				let call = vote_call::<$instance>(
					ElectionIdentifier::new(1.into(), $extra::$liveness(())),
					AuthorityVote::Vote($tuple::CompositeVote::$liveness(Default::default())),
				);
				let pallet_cf_elections::Call::vote { authority_votes } = &call else {
					unreachable!()
				};
				batched_votes.$field = Some(authority_votes.clone());
				full_votes.push(RuntimeCall::$variant(call));
			}};
		}
		liveness_vote!(
			bitcoin,
			BitcoinInstance,
			BitcoinElections,
			tuple_6_impls,
			Composite6Extra,
			FF
		);
		liveness_vote!(solana, SolanaInstance, SolanaElections, tuple_7_impls, Composite7Extra, EE);
		liveness_vote!(
			ethereum,
			EthereumInstance,
			EthereumElections,
			tuple_8_impls,
			Composite8Extra,
			FF
		);
		liveness_vote!(
			arbitrum,
			ArbitrumInstance,
			ArbitrumElections,
			tuple_6_impls,
			Composite6Extra,
			FF
		);
		liveness_vote!(tron, TronInstance, TronElections, tuple_5_impls, Composite5Extra, EE);
		liveness_vote!(bsc, BscInstance, BscElections, tuple_6_impls, Composite6Extra, FF);
		liveness_vote!(
			assethub,
			AssethubInstance,
			AssethubElections,
			tuple_5_impls,
			Composite5Extra,
			EE
		);

		let mut calls = full_votes.clone();
		calls.push(batch(full_votes));
		calls.push(batch(vec![]));
		calls.push(RuntimeCall::Environment(pallet_cf_environment::Call::submit_elections_votes {
			votes: Box::new(batched_votes),
		}));
		calls.push(RuntimeCall::Environment(pallet_cf_environment::Call::submit_elections_votes {
			votes: Box::default(),
		}));
		calls.push(RuntimeCall::BscElections(vote_call::<BscInstance>(
			ElectionIdentifier::new(1.into(), Composite6Extra::FF(())),
			AuthorityVote::PartialVote(tuple_6_impls::CompositePartialVote::FF(Default::default())),
		)));
		let calls: Vec<_> = calls.into_iter().flat_map(election_calls).collect();

		macro_rules! assert_votes {
			($instance:ty, $variant:ident, $expected:literal) => {
				let votes = collect_election_votes::<$instance>(
					calls.iter().filter_map(|call| match call {
						RuntimeCall::$variant(call) => Some(call),
						_ => None,
					}),
					&mut BTreeMap::new(),
				);
				assert_eq!(votes.len(), 1);
				assert_eq!(
					votes.values().next().unwrap().values().copied().collect::<Vec<_>>(),
					vec![$expected]
				);
			};
		}
		assert_votes!(BitcoinInstance, BitcoinElections, 3);
		assert_votes!(SolanaInstance, SolanaElections, 3);
		assert_votes!(EthereumInstance, EthereumElections, 3);
		assert_votes!(ArbitrumInstance, ArbitrumElections, 3);
		assert_votes!(TronInstance, TronElections, 3);
		assert_votes!(BscInstance, BscElections, 4);
		assert_votes!(AssethubInstance, AssethubElections, 3);
	}

	#[test]
	fn resolves_shared_data_from_full_votes_and_batched_provision() {
		use pallet_cf_elections::electoral_systems::block_height_witnesser::primitives::{
			BHWVote, Header, NonemptyContinuousHeaders,
		};
		use state_chain_runtime::chainflip::witnessing::bsc_elections::{
			BscBlockHeightWitnesser, BscChain,
		};

		let headers = NonemptyContinuousHeaders::<BscChain>::new(Header {
			block_height: Default::default(),
			hash: [1; 32].into(),
			parent_hash: Default::default(),
		});
		let full_vote = tuple_6_impls::CompositeVote::A(
			BHWVote::<BscBlockHeightWitnesser>::try_from(headers).unwrap(),
		);
		let mut expected_data = SharedDataMap::<BscInstance>::new();
		let partial = BscVoteStorageTuple::vote_into_partial_vote(&full_vote, |data| {
			let hash = SharedDataHash::of(&data);
			expected_data.insert(hash, data);
			hash
		});
		let election_id = ElectionIdentifier::new(2.into(), Composite6Extra::A(()));
		let full = RuntimeCall::BscElections(vote_call::<BscInstance>(
			election_id,
			AuthorityVote::Vote(full_vote),
		));
		let mut extracted_data = SharedDataMap::<BscInstance>::new();
		collect_election_votes::<BscInstance>(
			[match &full {
				RuntimeCall::BscElections(call) => call,
				_ => unreachable!(),
			}],
			&mut extracted_data,
		);
		assert_eq!(extracted_data, expected_data);

		let provision = RuntimeCall::BscElections(pallet_cf_elections::Call::provide_shared_data {
			shared_data: Box::new(expected_data.values().next().unwrap().clone()),
		});
		let pallet_cf_elections::Call::vote { authority_votes } =
			vote_call::<BscInstance>(election_id, AuthorityVote::PartialVote(partial.clone()))
		else {
			unreachable!()
		};
		let calls = election_calls(batch(vec![
			RuntimeCall::Environment(pallet_cf_environment::Call::submit_elections_votes {
				votes: Box::new(AllElectionInstancesVotes {
					bsc: Some(authority_votes),
					..Default::default()
				}),
			}),
			provision,
			RuntimeCall::BscElections(pallet_cf_elections::Call::ignore_my_votes {}),
		]));
		let mut provided_data = SharedDataMap::<BscInstance>::new();
		let votes = collect_election_votes::<BscInstance>(
			calls.iter().filter_map(|call| match call {
				RuntimeCall::BscElections(call) => Some(call),
				_ => None,
			}),
			&mut provided_data,
		);
		assert_eq!(provided_data, expected_data);
		assert_eq!(votes, BTreeMap::from([(election_id, BTreeMap::from([(partial.clone(), 1)]))]));
		assert!(!format_bsc_vote_detail(&partial, &provided_data).contains("MissingSharedData"));
	}
}

#[tokio::main(flavor = "multi_thread", worker_threads = 3)]
async fn main() {
	let rpc_url = env::var("CF_RPC_NODE").unwrap_or("ws://localhost:9944".into());

	loop {
		observe_elections(rpc_url.clone()).await;
		println!("Election observer stopped. Reconnecting in 3 seconds...");
		tokio::time::sleep(Duration::from_secs(3)).await;
	}
}
