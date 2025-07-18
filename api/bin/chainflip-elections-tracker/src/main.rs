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
#![feature(btree_extract_if)]

pub mod elections;
pub mod trace;

use std::collections::BTreeMap;

use cf_utilities::task_scope::{self};
use chainflip_engine::state_chain_observer::client::{base_rpc_api::BaseRpcApi, chain_api::ChainApi, storage_api::StorageApi, StateChainClient};
use elections::{ElectionData, Key, TraceInit, make_traces};
use futures::StreamExt;
use futures_util::FutureExt;
use opentelemetry::{
	Context, KeyValue, global,
	trace::{Span, TraceContextExt as _, Tracer, TracerProvider as _},
};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{
	Resource,
	trace::{RandomIdGenerator, TracerProvider},
};
use pallet_cf_elections::{
	electoral_systems::composite::tuple_7_impls::*, vote_storage::{composite::tuple_6_impls::CompositeVote, VoteStorage}, ElectoralSystemTypes, SharedDataHash, UniqueMonotonicIdentifier
};
use state_chain_runtime::{
	chainflip::{bitcoin_elections::BitcoinElectoralSystemRunner, solana_elections::SolanaElectoralSystemRunner}, Runtime, SolanaInstance
};
use std::env;
use trace::{NodeDiff, StateTree, diff, get_key_name, map_with_parent};

type VoteStorageTuple = <BitcoinElectoralSystemRunner as ElectoralSystemTypes>::VoteStorage;
type CompositeVoteType = <VoteStorageTuple as VoteStorage>::Vote;

#[tokio::main(flavor = "multi_thread", worker_threads = 3)]
async fn main() {
	// get env vars
	// wss://archive.sisyphos.chainflip.io
	// wss://archive.perseverance.chainflip.io
	let rpc_url = env::var("CF_RPC_NODE").unwrap_or("http://localhost:9944".into());
	let opentelemetry_backend_url =
		env::var("OTLP_BACKEND").unwrap_or("http://localhost:4317".into());

	// setup opentelemetry tracer
	let tracer_provider = opentelemetry_sdk::trace::TracerProvider::builder()
		.with_batch_exporter(
			opentelemetry_otlp::SpanExporter::builder()
				.with_tonic()
				.with_endpoint(&opentelemetry_backend_url)
				.build()
				.unwrap(),
			opentelemetry_sdk::runtime::Tokio,
		)
		.with_id_generator(RandomIdGenerator::default())
		.with_resource(Resource::new(vec![KeyValue::new("service.name", "es-overview")]))
		.build();

	global::set_tracer_provider(tracer_provider.clone());
	let tracer = tracer_provider.tracer("tracer-name-new");
	observe_elections(tracer, tracer_provider, rpc_url).await;
}

async fn observe_elections<T: Tracer + Send>(
	tracer: T,
	tracer_provider: TracerProvider,
	rpc_url: String,
) where
	T::Span: Span + Send + Sync + 'static,
{
	task_scope::task_scope(|scope| async move {

		let (_finalized_stream, unfinalized_stream, client) = StateChainClient::connect_without_account(scope, &rpc_url).await.unwrap();

		unfinalized_stream.fold((client, (BTreeMap::<u32,u32>::new(), BTreeMap::<u32,u32>::new()), tracer), async |(client, (overview_trace, detailed_traces), tracer), block| {

			let signed_block = client.base_rpc_client.block(block.hash).await.unwrap();
			if let Some(block) = signed_block {
				let extrinsics = block.block.extrinsics;
				for ex in extrinsics {

					match ex.function {
							state_chain_runtime::RuntimeCall::SolanaElections(call) => {
								println!("");
								println!("");
								println!("Solana");
								match call {
									pallet_cf_elections::Call::vote { authority_votes } => {
										for vote in *authority_votes {
											println!("{:?}", vote);
										}
									},
									_ => {},
								}
							},
							state_chain_runtime::RuntimeCall::BitcoinElections(call) => {
								println!("");
								println!("");
								println!("Bitcoin");
								match call {
									pallet_cf_elections::Call::vote { authority_votes } => {
										for (election_id, vote) in *authority_votes {
											println!("Election {:?}", election_id);
											match vote {
												pallet_cf_elections::vote_storage::AuthorityVote::PartialVote(partial) => {},
												pallet_cf_elections::vote_storage::AuthorityVote::Vote(full_vote) => {
													let partial = <VoteStorageTuple as VoteStorage>::vote_into_partial_vote(&full_vote, |shared_data| SharedDataHash::of(&shared_data));
													println!("PartialVote: {:?}", partial);
													match full_vote {
														pallet_cf_elections::vote_storage::composite::tuple_6_impls::CompositeVote::A(v) => {
															println!("{:?}",v);
														},
														pallet_cf_elections::vote_storage::composite::tuple_6_impls::CompositeVote::B(v) => {
															println!("{:?}",v);
														},
														pallet_cf_elections::vote_storage::composite::tuple_6_impls::CompositeVote::C(v) => {
															println!("{:?}",v);
														},
														pallet_cf_elections::vote_storage::composite::tuple_6_impls::CompositeVote::D(v) => {
															println!("{:?}",v);
														},
														pallet_cf_elections::vote_storage::composite::tuple_6_impls::CompositeVote::EE(v) =>{
															println!("{:?}",v);
														},
														pallet_cf_elections::vote_storage::composite::tuple_6_impls::CompositeVote::FF(v) =>{
															println!("{:?}",v);
														},
													}
													println!("");
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
			}

			// let _results = tracer_provider.force_flush();

			// let block_hash = block.hash;

			// let bitmaps : BTreeMap<UniqueMonotonicIdentifier,
			// 	_
			// 	> = client
			// 	.storage_map::<pallet_cf_elections::BitmapComponents::<Runtime, SolanaInstance>, BTreeMap<_,_>>(block_hash)
			// 	.await
			// 	.expect("could not get storage")
			// ;

			// let all_properties : BTreeMap<_,_> = client
			// 	.storage_map::<pallet_cf_elections::ElectionProperties::<Runtime, SolanaInstance>, BTreeMap<_,_>>(block_hash)
			// 	.await
			// 	.expect("could not get storage");

			// let validators : Vec<_> = client
			// 	.storage_value::<pallet_cf_validator::CurrentAuthorities::<Runtime>>(block_hash)
			// 	.await
			// 	.expect("could not get storage");

			// let mut individual_components = BTreeMap::new();
			// for key in all_properties.keys() {
			// 	for (validator_index, validator) in validators.iter().enumerate() {

			// 		if let Some((_, comp)) = client.storage_double_map_entry::<pallet_cf_elections::IndividualComponents::<Runtime, SolanaInstance>>(block_hash, key.unique_monotonic(), validator)
			// 		.await.expect("could not get storage") {
			// 			println!("got individual component for election {key:?} for vld {validator_index}: {comp:?}");
			// 			individual_components.entry(*key.unique_monotonic()).or_insert(BTreeMap::new()).insert(validator_index, comp);
			// 		}
			// 	}
			// }

			// let bitmaps = bitmaps.into_iter()
			// 	.map(|(k,v)| (k, v.bitmaps))
			// 	.collect();

			// const ELECTORAL_SYSTEM_NAMES : [&str; 6] = ["Blockheight", "Ingress", "Nonce", "Egress", "Liveness", "Vaultswap"];

			// let elections = all_properties.iter()
			// 	.map(|(key, val)| {
			// 		let index = match val {
			// 				CompositeElectionProperties::A(_)  => 0,
			// 				CompositeElectionProperties::B(_)  => 1,
			// 				CompositeElectionProperties::C(_)  => 2,
			// 				CompositeElectionProperties::D(_)  => 3,
			// 				CompositeElectionProperties::EE(_) => 4,
			// 				CompositeElectionProperties::FF(_) => 5,
			// 				CompositeElectionProperties::G(_) => 6,
			// 			};
			// 		(*key, (ELECTORAL_SYSTEM_NAMES[index].into(), val.clone()))
			// 	})
			// 	.collect();

			// let result : ElectionData<SolanaElectoralSystemRunner> = ElectionData {
			// 	height: block.number,
			// 	bitmaps,
			// 	elections,
			// 	individual_components,
			// 	validators_count: validators.len() as u32,
			// 	_phantom: Default::default(),
			// 	electoral_system_names: ELECTORAL_SYSTEM_NAMES.iter().map(|name| (*name).into()).collect(),
			// };

			// let new_full_trace = make_traces(result);
			// let new_overview_trace = new_full_trace.iter().map(|(k,v)| (k.clone(), v.clone())).filter(|(key, _)| key.len() <= 4).collect::<BTreeMap<_,_>>();
			// let new_detailed_traces = new_full_trace.iter().map(|(k,v)| (k.clone(), v.clone())).filter(|(key, _)| key.len() >= 2).collect::<BTreeMap<_,_>>();

			// let overview_trace = push_traces(&tracer, overview_trace, new_overview_trace);
			// let detailed_traces = push_traces(&tracer, detailed_traces, new_detailed_traces);

			(client, (overview_trace, detailed_traces), tracer)

		}).await;

		Ok(())

	 }.boxed()).await.unwrap()
}

fn push_traces<T: Tracer + Send>(
	tracer: &T,
	current: StateTree<Key, Context>,
	new: StateTree<Key, TraceInit>,
) -> StateTree<Key, Context>
where
	T::Span: Span + Send + Sync + 'static,
{
	let traces = map_with_parent(
		diff(current, new),
		|k, p: Option<&Option<Context>>, d: NodeDiff<Context, TraceInit>| match d {
			trace::NodeDiff::Left(context) => {
				println!("closing trace {k:?}");
				context.span().end();
				None
			},
			trace::NodeDiff::Right(TraceInit { end_immediately, attributes: values }) => {
				let context = if let Some(Some(context)) = p {
					let key = get_key_name(k);

					let mut span = tracer.start_with_context(key, context);
					for (key, value) in values {
						span.set_attribute(KeyValue::new(key, value));
					}
					let context = context.with_span(span);

					println!("open trace {k:?}");

					context
				} else {
					let key = get_key_name(k);

					let context =
						Context::new().with_value(KeyValue::new("key", format!("{key:?}")));
					let mut span = tracer.start_with_context(key, &context);
					for (key, value) in values {
						span.set_attribute(KeyValue::new(key, value));
					}
					let context = context.with_span(span);
					println!("open trace {k:?} [NO PARENT]");
					context
				};
				if end_immediately {
					context.span().end();
				}
				Some(context)
			},
			trace::NodeDiff::Both(x, _) => Some(x),
		},
	)
	.into_iter()
	.filter_map(|(k, v)| v.map(|v| (k, v)))
	.collect();
	traces
}
