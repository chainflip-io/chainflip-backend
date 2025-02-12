#![feature(btree_extract_if)]

pub mod elections;
pub mod trace;

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration};
// use std::task::ContextBuilder;

use bitvec::vec::BitVec;
use chainflip_engine::state_chain_observer::client::chain_api::ChainApi;
use chainflip_engine::witness::dot::polkadot::runtime_apis::metadata::Metadata;
use chainflip_engine::witness::dot::polkadot::storage;
use codec::{Decode, Encode};
use chainflip_engine::state_chain_observer::client::{
	base_rpc_api::RawRpcApi, extrinsic_api::signed::SignedExtrinsicApi, BlockInfo,
	StateChainClient,
};
// use opentelemetry::global::ObjectSafeSpan;
use opentelemetry::trace::{mark_span_as_active, Span, TraceContextExt as _, Tracer, TracerProvider as _};
use chainflip_engine::state_chain_observer::client::base_rpc_api::BaseRpcClient;
use custom_rpc::CustomApiClient;
use elections::{make_traces, Key, TraceInit};
use pallet_cf_validator::AuthorityIndex;
use state_chain_runtime::chainflip::solana_elections::SolanaElectoralSystemRunner;
use tokio::time::sleep;
use tracing::instrument::WithSubscriber;
use tracing::{event, span, Instrument, Level};
use tracing_core::Callsite;
use tracing_opentelemetry::PreSampledTracer;
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::Registry;
// use tracing_subscriber::layer::{Context, SubscriberExt};
use opentelemetry::{global, Context, KeyValue};
use opentelemetry_sdk::trace::{RandomIdGenerator, TracerProvider};
use opentelemetry_sdk::Resource;
use pallet_cf_elections::electoral_system::{BitmapComponentOf};
use pallet_cf_elections::{ElectionDataFor, ElectionIdentifierOf, ElectionProperties, ElectoralSystemTypes, IndividualComponentOf, UniqueMonotonicIdentifier};
use state_chain_runtime::{Runtime, SolanaInstance};
use cf_utilities::task_scope::{self, Scope};
use futures_util::FutureExt;
use chainflip_engine::state_chain_observer::client::storage_api::StorageApi;
use futures::{stream, StreamExt, TryStreamExt};
use pallet_cf_elections::{
	electoral_systems::composite::tuple_6_impls::*,
};
use trace::{diff, get_key_name, map_with_parent, NodeDiff, Trace};
use std::env;


#[derive(Debug, Eq, PartialEq, Clone, Encode, Decode)]
pub struct ElectionData<ES: ElectoralSystemTypes> {
    pub bitmaps: BTreeMap<
		UniqueMonotonicIdentifier,
		Vec<(BitmapComponentOf<ES>, BitVec<u8, bitvec::order::Lsb0>)>
		>,

    pub individual_components: BTreeMap<
		UniqueMonotonicIdentifier,
		BTreeMap<usize, IndividualComponentOf<ES>>
		>,

	pub elections: BTreeMap<ElectionIdentifierOf<ES>, (String, ES::ElectionProperties)>,

	pub electoral_system_names: Vec<String>,

	pub validators: u32,

    pub _phantom: std::marker::PhantomData<ES>
}


#[tokio::main(flavor = "multi_thread", worker_threads = 3)]
async fn main() {
	println!("Hello, world!");

    let tracer_provider = opentelemetry_sdk::trace::TracerProvider::builder()
        .with_batch_exporter(
            opentelemetry_otlp::SpanExporter::builder()
                .with_tonic()
                .build()
                .unwrap(),
            opentelemetry_sdk::runtime::Tokio,
        )
        // .with_sampler(Sampler::AlwaysOn)
        .with_id_generator(RandomIdGenerator::default())
        // .with_max_events_per_span(64)
        // .with_max_attributes_per_span(16)
        // .with_max_events_per_span(16)
        .with_resource(Resource::new(vec![KeyValue::new("service.name", "es-overview")]))
        .build();

    global::set_tracer_provider(tracer_provider.clone());
    let tracer = tracer_provider.tracer("tracer-name-new");

    // Create a tracing layer with the configured tracer
    // let telemetry = tracing_opentelemetry::layer().with_tracer(tracer);

    // Use the tracing subscriber `Registry`, or any other subscriber that impls `LookupSpan`
    // let subscriber = Registry::default().with(telemetry);

    // let _guard = tracing::subscriber::set_default(subscriber);

    // event!(Level::INFO, "in hello!");

	// new_watch(tracer).await;


	{

	let ctx = Context::new().with_value(KeyValue::new("key", "value"));
	// let _guard = ctx.attach();

	let builder = tracer.span_builder("test_proc")
		.with_start_time(std::time::SystemTime::now())
		.with_span_id(tracer.new_span_id());

	let span = builder.start_with_context(&tracer, &ctx);

	// .start_with_context("test_proc", &ctx);
	let ctx1 = ctx.with_span(span);

	sleep(Duration::from_secs(1)).await;

	// mark_span_as_active(ctx1.span());

	ctx1.span().end();

	let _results = tracer_provider.force_flush();



	// let span2 = tracer.start_with_context("test_proc_child", &ctx1);
	// let ctx2 = ctx1.with_span(span2);

	// sleep(Duration::from_secs(1)).await;

	// let span3 = tracer.start_with_context("test_proc_child2", &ctx2);
	// let ctx3 = ctx1.with_span(span3);

	// ctx3.span().add_event("starting??", Vec::new());

	// sleep(Duration::from_secs(1)).await;
	// ctx3.span().end();


	// ctx1.span().end();
	// let x = ctx2.span().with_current_subscriber();
	// x.inner().set_attributes([KeyValue::new("mykey", "myvalue")]);

	// sleep(Duration::from_secs(1)).await;

	// ctx2.span().end();
	// sleep(Duration::from_secs(1)).await;

	}

	// let results = tracer_provider.force_flush();
	// for result in results {
	// 	println!("result: {result:?}");
	// }

	// let result = tracer_provider.shutdown();
	// println!("{result:?}");

	new_watch(tracer, tracer_provider).await;
}


fn push_traces<T: Tracer + Send>(tracer: &T, current: Trace<Key, Context>, new: Trace<Key, TraceInit>) -> Trace<Key, Context> 
 where T::Span : Span + Send + Sync + 'static
{
			let δ = diff(current, new);
			let traces = map_with_parent(δ, |k, p: Option<&Option<Context>>, d: NodeDiff<Context, TraceInit>| match d {
					trace::NodeDiff::Left(context) => {
						println!("closing trace {k:?}"); 
						context.span().end();
						None
					},
					trace::NodeDiff::Right(TraceInit { end_immediately, attributes: values }) => {
						let context = 
						if let Some(Some(context)) = p {


							let key = get_key_name(k);

							let mut span = tracer.start_with_context(key, &context);
							for  (key, value) in values {
								span.set_attribute(KeyValue::new(key, value));
							}
							let context = context.with_span(span);


							println!("open trace {k:?}"); 

							context

						} else {

							let key = get_key_name(k);

							let context = Context::new().with_value(KeyValue::new("key", format!("{key:?}")));
							let span = tracer.start_with_context(key, &context);
							let context = context.with_span(span);
							println!("open trace {k:?} [NO PARENT]"); 
							context
						};
						if end_immediately {
							context.span().end();
						}
						Some(context)
					},
					trace::NodeDiff::Both(x, _) => {
						Some(x)
					},
				}
			)
			.into_iter().filter_map(|(k, v)| match v {Some(v) => Some((k,v)), None => None}).collect();
		traces
}

async fn new_watch<T: Tracer + Send>(tracer: T, tracer_provider: TracerProvider) 
 where T::Span : Span + Send + Sync + 'static

{


	task_scope::task_scope_local(|scope| async move { 

		let rpc_url = env::var("CF_RPC_NODE").expect("CF_RPC_NODE required");

		let (finalized_stream, unfinalized_stream, client) = StateChainClient::connect_without_account(&scope, &rpc_url).await.unwrap();

		unfinalized_stream.fold((client, (BTreeMap::new(), BTreeMap::new()), tracer), async |(client, (overview_trace, detailed_traces), tracer), block| {

			let _results = tracer_provider.force_flush();

			// let block_hash = client.latest_finalized_block().hash;
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
			for (key, prop) in &all_properties {
				for (validator_index, validator) in validators.iter().enumerate() {

					if let Some((_, comp)) = client.storage_double_map_entry::<pallet_cf_elections::IndividualComponents::<Runtime, SolanaInstance>>(block_hash, key.unique_monotonic(), validator)
					.await.expect("could not get storage") {
						println!("got individual component for election {key:?} for vld {validator_index}: {comp:?}");
						individual_components.entry(*key.unique_monotonic()).or_insert(BTreeMap::new()).insert(validator_index, comp);
						// individual_components.insert(key.unique_monotonic(), comp);
					}
				}
			}

			let bitmaps = bitmaps.into_iter()
				.map(|(k,v)| (k, v.bitmaps))
				.collect();

			let elections = all_properties.iter()
				.map(|(key, val)| {
					let name = match val {
							CompositeElectionProperties::A(_)  => "Blockheight",
							CompositeElectionProperties::B(_)  => "Ingress",
							CompositeElectionProperties::C(_)  => "Nonce",
							CompositeElectionProperties::D(_)  => "Egress",
							CompositeElectionProperties::EE(_) => "Liveness",
							CompositeElectionProperties::FF(_) => "Vaultswap",
						};
					(key.clone(), (name.into(), val.clone()))
				})
				.collect();

			let result : ElectionData<SolanaElectoralSystemRunner> = ElectionData {
				bitmaps,
				elections,
				individual_components,
				validators: validators.len() as u32,
				_phantom: Default::default(),
				electoral_system_names: vec![
							"Blockheight".into(),
							"Ingress".into(),
							"Nonce".into(),
							"Egress".into(),
							"Liveness".into(),
							"Vaultswap".into(),
				],
			};

			let new_full_trace = make_traces(result);
			let new_overview_trace = new_full_trace.iter().map(|(k,v)| (k.clone(), v.clone())).filter(|(key, _)| key.len() <= 3).collect::<BTreeMap<_,_>>();
			let new_detailed_traces = new_full_trace.iter().map(|(k,v)| (k.clone(), v.clone())).filter(|(key, _)| key.len() >= 1).collect::<BTreeMap<_,_>>();

			let overview_trace = push_traces(&tracer, overview_trace, new_overview_trace);
			let detailed_traces = push_traces(&tracer, detailed_traces, new_detailed_traces);


				// δ.into_iter().filter_map(|(k,d)| match d ).collect();

			// let all_properties : BTreeMap<_,_> = client
			// 	.storage_map::<pallet_cf_elections::ElectionProperties::<Runtime, SolanaInstance>, BTreeMap<_,_>>(block_hash)
			// 	.await
			// 	.expect("could not get storage");

			// let delta_properties : Vec<_> =
			// 	all_properties.iter().map(|(_, value)| match value {
			// 		pallet_cf_elections::electoral_systems::composite::tuple_6_impls::CompositeElectionProperties::C(props) => Some(props),
			// 		_ => None
			// 	})
			// 	.collect();

			// let all_state_map : BTreeMap<_,_> = client
			// 	.storage_map::<pallet_cf_elections::ElectoralUnsynchronisedStateMap::<Runtime, SolanaInstance>, BTreeMap<_,_>>(block_hash)
			// 	.await
			// 	.expect("could not get storage");

			// let delta_state : BTreeMap<_,_> =
			// 	all_state_map.iter().filter_map(|(key, value)| match (key,value) {
			// 		(CompositeElectoralUnsynchronisedStateMapKey::C(key), CompositeElectoralUnsynchronisedStateMapValue::C(val))
			// 		=> Some((key,val)),
			// 		_ => None
			// 	})
			// 	.collect();

			// let block_height_state = client
			// 	.storage_value::<pallet_cf_elections::ElectoralUnsynchronisedState::<Runtime, SolanaInstance>>(block_hash)
			// 	.await
			// 	.expect("could not get storage")
			// 	.map(|(value, ..)| value)
			// 	.expect("could not get block height");

			(client, (overview_trace, detailed_traces), tracer)

		}).await;

		Ok(())

	 }.boxed_local()).await.unwrap()
}





