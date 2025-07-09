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
use chainflip_engine::state_chain_observer::client::{StateChainClient, storage_api::StorageApi};
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
	UniqueMonotonicIdentifier,
	electoral_systems::composite::{
		tuple_6_impls::{self, *},
		tuple_7_impls::{self, *},
	},
};
use state_chain_runtime::{
	BitcoinInstance, Runtime, SolanaInstance,
	chainflip::{
		bitcoin_elections::BitcoinElectoralSystemRunner,
		solana_elections::SolanaElectoralSystemRunner,
	},
};
use std::env;
use trace::{NodeDiff, StateTree, diff, get_key_name, map_with_parent};

#[tokio::main(flavor = "multi_thread", worker_threads = 3)]
async fn main() {
	// get env vars
	let rpc_url = env::var("CF_RPC_NODE").unwrap_or("wss://archive.sisyphos.chainflip.io".into());
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

		unfinalized_stream.fold((client, (BTreeMap::new(), BTreeMap::new(), BTreeMap::new(), BTreeMap::new()), tracer), async |(client, (solana_overview_traces, solana_detailed_traces, bitcoin_overview_traces, bitcoin_detailed_traces), tracer), block| {

			let _results = tracer_provider.force_flush();

			let block_hash = block.hash;

			let validators : Vec<_> = client
			.storage_value::<pallet_cf_validator::CurrentAuthorities::<Runtime>>(block_hash)
			.await
			.expect("could not get storage");

			// Solana
			let solana_bitmaps : BTreeMap<UniqueMonotonicIdentifier,
				_
				> = client
				.storage_map::<pallet_cf_elections::BitmapComponents::<Runtime, SolanaInstance>, BTreeMap<_,_>>(block_hash)
				.await
				.expect("could not get storage")
			;

			let solana_all_properties : BTreeMap<_,_> = client
				.storage_map::<pallet_cf_elections::ElectionProperties::<Runtime, SolanaInstance>, BTreeMap<_,_>>(block_hash)
				.await
				.expect("could not get storage");

			let mut solana_individual_components = BTreeMap::new();
			for key in solana_all_properties.keys() {
				for (validator_index, validator) in validators.iter().enumerate() {

					if let Some((_, comp)) = client.storage_double_map_entry::<pallet_cf_elections::IndividualComponents::<Runtime, SolanaInstance>>(block_hash, key.unique_monotonic(), validator)
					.await.expect("could not get storage") {
						println!("got individual component for election {key:?} for vld {validator_index}: {comp:?}");
						solana_individual_components.entry(*key.unique_monotonic()).or_insert(BTreeMap::new()).insert(validator_index, comp);
					}
				}
			}

			let solana_bitmaps = solana_bitmaps.into_iter()
				.map(|(k,v)| (k, v.bitmaps))
				.collect();

			const SOLANA_ELECTORAL_SYSTEM_NAMES : [&str; 6] = ["Blockheight", "Ingress", "Nonce", "Egress", "Liveness", "Vaultswap"];

			let solana_elections = solana_all_properties.iter()
				.map(|(key, val)| {
					let index = match val {
							tuple_7_impls::CompositeElectionProperties::A(_)  => 0,
							tuple_7_impls::CompositeElectionProperties::B(_)  => 1,
							tuple_7_impls::CompositeElectionProperties::C(_)  => 2,
							tuple_7_impls::CompositeElectionProperties::D(_)  => 3,
							tuple_7_impls::CompositeElectionProperties::EE(_) => 4,
							tuple_7_impls::CompositeElectionProperties::FF(_) => 5,
							tuple_7_impls::CompositeElectionProperties::G(_) => 6,
						};
					(*key, (SOLANA_ELECTORAL_SYSTEM_NAMES[index].into(), val.clone()))
				})
				.collect();

			let solana_election_data : ElectionData<SolanaElectoralSystemRunner> = ElectionData {
				height: block.number,
				bitmaps: solana_bitmaps,
				elections: solana_elections,
				individual_components: solana_individual_components,
				validators_count: validators.len() as u32,
				instance: "Solana".to_string(),
				_phantom: Default::default(),
				electoral_system_names: SOLANA_ELECTORAL_SYSTEM_NAMES.iter().map(|name| (*name).into()).collect(),
			};

			let solana_new_full_trace = make_traces(solana_election_data);
			let solana_new_overview_trace = solana_new_full_trace.iter().map(|(k,v)| (k.clone(), v.clone())).filter(|(key, _)| key.len() <= 5).collect::<BTreeMap<_,_>>();
			let solana_new_detailed_traces = solana_new_full_trace.iter().map(|(k,v)| (k.clone(), v.clone())).filter(|(key, _)| key.len() >= 3).collect::<BTreeMap<_,_>>();

			let solana_overview_traces = push_traces(&tracer, solana_overview_traces, solana_new_overview_trace);
			let solana_detailed_traces = push_traces(&tracer, solana_detailed_traces, solana_new_detailed_traces);



			// Bitcoin
			let bitcoin_bitmaps : BTreeMap<UniqueMonotonicIdentifier,
				_
				> = client
				.storage_map::<pallet_cf_elections::BitmapComponents::<Runtime, BitcoinInstance>, BTreeMap<_,_>>(block_hash)
				.await
				.expect("could not get storage")
			;

			let bitcoin_all_properties : BTreeMap<_,_> = client
				.storage_map::<pallet_cf_elections::ElectionProperties::<Runtime, BitcoinInstance>, BTreeMap<_,_>>(block_hash)
				.await
				.expect("could not get storage");

			let mut bitcoin_individual_components = BTreeMap::new();
			for key in bitcoin_all_properties.keys() {
				for (validator_index, validator) in validators.iter().enumerate() {

					if let Some((_, comp)) = client.storage_double_map_entry::<pallet_cf_elections::IndividualComponents::<Runtime, BitcoinInstance>>(block_hash, key.unique_monotonic(), validator)
					.await.expect("could not get storage") {
						println!("got individual component for election {key:?} for vld {validator_index}: {comp:?}");
						bitcoin_individual_components.entry(*key.unique_monotonic()).or_insert(BTreeMap::new()).insert(validator_index, comp);
					}
				}
			}

			let bitcoin_bitmaps = bitcoin_bitmaps.into_iter()
				.map(|(k,v)| (k, v.bitmaps))
				.collect();

			const BITCOIN_ELECTORAL_SYSTEM_NAMES : [&str; 6] = ["BHW", "DepositChannelBW", "VaultBW", "EgressBW", "FeeTracking", "Liveness"];

			let bitcoin_elections = bitcoin_all_properties.iter()
				.map(|(key, val)| {
					let index = match val {
							tuple_6_impls::CompositeElectionProperties::A(_)  => 0,
							tuple_6_impls::CompositeElectionProperties::B(_)  => 1,
							tuple_6_impls::CompositeElectionProperties::C(_)  => 2,
							tuple_6_impls::CompositeElectionProperties::D(_)  => 3,
							tuple_6_impls::CompositeElectionProperties::EE(_) => 4,
							tuple_6_impls::CompositeElectionProperties::FF(_) => 5,
						};
					(*key, (BITCOIN_ELECTORAL_SYSTEM_NAMES[index].into(), val.clone()))
				})
				.collect();

			let bitcoin_election_data : ElectionData<BitcoinElectoralSystemRunner> = ElectionData {
				height: block.number,
				bitmaps: bitcoin_bitmaps,
				elections: bitcoin_elections,
				individual_components: bitcoin_individual_components,
				validators_count: validators.len() as u32,
				instance: "Bitcoin".to_string(),
				_phantom: Default::default(),
				electoral_system_names: BITCOIN_ELECTORAL_SYSTEM_NAMES.iter().map(|name| (*name).into()).collect(),
			};

			let bitcoin_new_full_trace = make_traces(bitcoin_election_data);
			let bitcoin_new_overview_trace = bitcoin_new_full_trace.iter().map(|(k,v)| (k.clone(), v.clone())).filter(|(key, _)| key.len() <= 5).collect::<BTreeMap<_,_>>();
			let bitcoin_new_detailed_traces = bitcoin_new_full_trace.iter().map(|(k,v)| (k.clone(), v.clone())).filter(|(key, _)| key.len() >= 3).collect::<BTreeMap<_,_>>();

			let bitcoin_overview_traces = push_traces(&tracer, bitcoin_overview_traces, bitcoin_new_overview_trace);
			let bitcoin_detailed_traces = push_traces(&tracer, bitcoin_detailed_traces, bitcoin_new_detailed_traces);

			(client, (solana_overview_traces, solana_detailed_traces, bitcoin_overview_traces, bitcoin_detailed_traces), tracer)

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
