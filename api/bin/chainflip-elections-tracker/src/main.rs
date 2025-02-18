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
	UniqueMonotonicIdentifier, electoral_systems::composite::tuple_6_impls::*,
};
use state_chain_runtime::{
	Runtime, SolanaInstance, chainflip::solana_elections::SolanaElectoralSystemRunner,
};
use std::env;
use trace::{NodeDiff, StateTree, diff, get_key_name, map_with_parent};

#[tokio::main(flavor = "multi_thread", worker_threads = 3)]
async fn main() {
	// get env vars
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

		unfinalized_stream.fold((client, (BTreeMap::new(), BTreeMap::new()), tracer), async |(client, (overview_trace, detailed_traces), tracer), block| {

			let _results = tracer_provider.force_flush();

			let block_hash = block.hash;

			let bitmaps : BTreeMap<UniqueMonotonicIdentifier,
				_
				> = client
				.storage_map::<pallet_cf_elections::BitmapComponents::<Runtime, SolanaInstance>, BTreeMap<_,_>>(block_hash)
				.await
				.expect("could not get storage")
			;

			let all_properties : BTreeMap<_,_> = client
				.storage_map::<pallet_cf_elections::ElectionProperties::<Runtime, SolanaInstance>, BTreeMap<_,_>>(block_hash)
				.await
				.expect("could not get storage");

			let validators : Vec<_> = client
				.storage_value::<pallet_cf_validator::CurrentAuthorities::<Runtime>>(block_hash)
				.await
				.expect("could not get storage");

			let mut individual_components = BTreeMap::new();
			for key in all_properties.keys() {
				for (validator_index, validator) in validators.iter().enumerate() {

					if let Some((_, comp)) = client.storage_double_map_entry::<pallet_cf_elections::IndividualComponents::<Runtime, SolanaInstance>>(block_hash, key.unique_monotonic(), validator)
					.await.expect("could not get storage") {
						println!("got individual component for election {key:?} for vld {validator_index}: {comp:?}");
						individual_components.entry(*key.unique_monotonic()).or_insert(BTreeMap::new()).insert(validator_index, comp);
					}
				}
			}

			let bitmaps = bitmaps.into_iter()
				.map(|(k,v)| (k, v.bitmaps))
				.collect();

			const ELECTORAL_SYSTEM_NAMES : [&'static str; 6] = ["Blockheight", "Ingress", "Nonce", "Egress", "Liveness", "Vaultswap"];

			let elections = all_properties.iter()
				.map(|(key, val)| {
					let index = match val {
							CompositeElectionProperties::A(_)  => 0,
							CompositeElectionProperties::B(_)  => 1,
							CompositeElectionProperties::C(_)  => 2,
							CompositeElectionProperties::D(_)  => 3,
							CompositeElectionProperties::EE(_) => 4,
							CompositeElectionProperties::FF(_) => 5,
						};
					(*key, (ELECTORAL_SYSTEM_NAMES[index].into(), val.clone()))
				})
				.collect();

			let result : ElectionData<SolanaElectoralSystemRunner> = ElectionData {
				height: block.number,
				bitmaps,
				elections,
				individual_components,
				validators_count: validators.len() as u32,
				_phantom: Default::default(),
				electoral_system_names: ELECTORAL_SYSTEM_NAMES.iter().map(|name| (*name).into()).collect(),
			};

			let new_full_trace = make_traces(result);
			let new_overview_trace = new_full_trace.iter().map(|(k,v)| (k.clone(), v.clone())).filter(|(key, _)| key.len() <= 4).collect::<BTreeMap<_,_>>();
			let new_detailed_traces = new_full_trace.iter().map(|(k,v)| (k.clone(), v.clone())).filter(|(key, _)| key.len() >= 2).collect::<BTreeMap<_,_>>();

			let overview_trace = push_traces(&tracer, overview_trace, new_overview_trace);
			let detailed_traces = push_traces(&tracer, detailed_traces, new_detailed_traces);

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
