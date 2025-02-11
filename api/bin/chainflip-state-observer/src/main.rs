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
use opentelemetry::global::ObjectSafeSpan;
use opentelemetry::trace::{mark_span_as_active, Span, TraceContextExt as _, Tracer, TracerProvider as _};
use chainflip_engine::state_chain_observer::client::base_rpc_api::BaseRpcClient;
use custom_rpc::CustomApiClient;
use elections::make_traces;
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
use pallet_cf_elections::electoral_system::{BitmapComponentOf, ElectionData};
use pallet_cf_elections::{ElectionDataFor, UniqueMonotonicIdentifier};
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
        .with_resource(Resource::new(vec![KeyValue::new("service.name", "example2")]))
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

// #[derive(Debug, PartialEq)]
// struct KeyValue(&'static str);

async fn new_watch<T: Tracer + Send>(tracer: T, tracer_provider: TracerProvider) 
 where T::Span : Span + Send + Sync + 'static

// async fn new_watch() 
{

	// let root = span!(tracing::Level::TRACE, "app_start", work_units = 2);
	// let _enter = root.enter();

	// let (scope, stream) = Scope::new();

	task_scope::task_scope_local(|scope| async move { 

		let rpc_url = env::var("CF_RPC_NODE").expect("CF_RPC_NODE required");

		let (finalized_stream, _, client) = StateChainClient::connect_without_account(&scope, &rpc_url).await.unwrap();

		let traces = BTreeMap::new();

		finalized_stream.fold((client, traces, tracer), async |(client, traces, tracer), block| {

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

			let bitmaps = bitmaps.into_iter()
				.map(|(k,v)| (k, v.bitmaps))
				.collect();

			let result : ElectionDataFor<Runtime, SolanaInstance> = ElectionData {
				bitmaps,
				_phantom: Default::default()
			};

			let new_traces = make_traces(result);
			// println!("got election data: {traces:?}");

			let δ = diff(traces, new_traces);
			let traces = map_with_parent(δ, |k, p: Option<&Option<Context>>, d: NodeDiff<Context, ()>| match d {
					trace::NodeDiff::Left(context) => {
						println!("closing trace {k:?}"); 
						context.span().end();
						None
					},
					trace::NodeDiff::Right(y) => {Some(
						if let Some(Some(context)) = p {

							// let _guard = Context::attach(context.clone());


							let key = get_key_name(k);

							let span = tracer.start_with_context(key, &context);
							let context = context.with_span(span);


							println!("open trace {k:?}"); 
							// span!(parent: parent, tracing::Level::TRACE, get_key_name(k), key = format!("{k:?}")).entered()

							// let name = "";
							// let target = module_path!();
							// let level = Level::TRACE;

							(context)
							

							// let identifier = tracing_core::identify_callsite!();


							// struct MyCallsite<'a> {
							// 	metadata: Option<tracing::Metadata<'a>>

							// };
							// impl<'a> Callsite for MyCallsite<'a> {
							// 		fn set_interest(&self, interest: tracing_core::Interest) {
							// 		}
							
							// 		fn metadata(&self) -> &tracing::Metadata<'_> {
							// 			&self.metadata
							// 		}
							// 	};

							// let __CALLSITE: tracing::__macro_support::MacroCallsite =  {

							// 	let META: tracing::Metadata<'static> = {
							// 		tracing::metadata::Metadata::new(
							// 			name,
							// 			target,
							// 			level,
							// 			tracing::__macro_support::Option::Some(tracing::__macro_support::file!()),
							// 			tracing::__macro_support::Option::Some(0u32),
							// 			tracing::__macro_support::Option::Some(
							// 				module_path!(),
							// 			),
							// 			tracing::field::FieldSet::new(
							// 				(&[]),
							// 				// tracing::callsite::Identifier((&__CALLSITE)),
							// 				// tracing::callsite::Identifier((&__CALLSITE)),
							// 				identifier
							// 			),
							// 			(tracing::metadata::Kind::SPAN),
							// 		)
							// 	};
							// 	tracing::callsite::DefaultCallsite::new(&META)
							// }
							
							// tracing::callsite2! {
							// 	name: name,
							// 	kind: tracing::metadata::Kind::SPAN,
							// 	target: target,
							// 	level: level,
							// 	fields: 
							// };
            // let mut interest = $crate::subscriber::Interest::never();
            // if $crate::level_enabled!($lvl)
            //     && { interest = __CALLSITE.interest(); !interest.is_never() }
            //     && $crate::__macro_support::__is_enabled(__CALLSITE.metadata(), interest)
            // {
							// let callsite = Box::new(MyCallsite { metadata: None });
							// let callsite: &'static mut MyCallsite = Box::leak(callsite);
							// let meta = 
							// 		tracing::metadata::Metadata::new(
							// 			name,
							// 			target,
							// 			level,
							// 			tracing::__macro_support::Option::Some(tracing::__macro_support::file!()),
							// 			tracing::__macro_support::Option::Some(0u32),
							// 			tracing::__macro_support::Option::Some(
							// 				module_path!(),
							// 			),
							// 			tracing::field::FieldSet::new(
							// 				(&[]),
							// 				// tracing::callsite::Identifier((&__CALLSITE)),
							// 				// tracing::callsite::Identifier((&__CALLSITE)),
							// 				tracing::callsite::Identifier(callsite)
							// 				// identifier
							// 			),
							// 			(tracing::metadata::Kind::SPAN),
							// 		);
							// callsite.metadata = Some(meta);
							// // let meta = __CALLSITE.metadata();
							// // span with explicit parent
							// tracing::Span::child_of(
							// 	parent,
							// 	&meta,
							// 	&tracing::valueset!(meta.fields(), ),
							// 	// &tracing::valueset!(meta.fields(), $($fields)*),
							// ).entered()
            // } else {
            //     let span = $crate::__macro_support::__disabled_span(__CALLSITE.metadata());
            //     $crate::if_log_enabled! { $lvl, {
            //         span.record_all(&$crate::valueset!(__CALLSITE.metadata().fields(), $($fields)*));
            //     }};
            //     span
            // }

						} else {

							let key = get_key_name(k);

							let context = Context::new().with_value(KeyValue::new("key", format!("{key:?}")));
							let span = tracer.start_with_context(key, &context);
							let context = context.with_span(span);
							// start_with_context(key, &Context::current_with_span(parent.clone()));

							// tracer.start(format!("{k:?}"))
							println!("open trace {k:?} [NO PARENT]"); 
							// span!(tracing::Level::TRACE, "root", key = format!("{k:?}")).entered()
							context
						}
					)},
					trace::NodeDiff::Both(x, _) => {
						Some(x)
					},
				}
			)
			.into_iter().filter_map(|(k, v)| match v {Some(v) => Some((k,v)), None => None}).collect();

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

			(client, traces, tracer)

		}).await;

		// stream.all(|_| true).await;

		Ok(())

	 }.boxed_local()).await.unwrap()
}



// async fn watch_stuck_solana_ingress() {

// 	task_scope::task_scope(|scope| async move { 

// 		// StateChainClient: ElectoralApi<Instance> + SignedExtrinsicApi + ChainApi,
// 		let (_, _, client) = StateChainClient::connect_without_account(scope, "ws://localhost:9944").await.unwrap();

// 		let block_hash = client.latest_finalized_block().hash;

// 		let all_properties : BTreeMap<_,_> = client
// 			.storage_map::<pallet_cf_elections::ElectionProperties::<Runtime, SolanaInstance>, BTreeMap<_,_>>(block_hash)
// 			.await
// 			.expect("could not get storage");

// 		let delta_properties : Vec<_> =
// 			all_properties.iter().map(|(_, value)| match value {
// 				pallet_cf_elections::electoral_systems::composite::tuple_6_impls::CompositeElectionProperties::B(props) => Some(props),
// 				_ => None
// 			})
// 			.collect();

// 		let block_height_properties : Vec<_> =
// 			all_properties.iter().filter_map(|(_, value)| match value {
// 				pallet_cf_elections::electoral_systems::composite::tuple_6_impls::CompositeElectionProperties::A(props) => Some(props),
// 				_ => None
// 			})
// 			.collect();

// 		for delta_prop in delta_properties {
// 			println!("delta: {delta_prop:?}");
// 		}






// 		/*
// 		let bitmaps : BTreeMap<UniqueMonotonicIdentifier, _ > = client
// 			.storage_map::<pallet_cf_elections::BitmapComponents::<Runtime, SolanaInstance>, BTreeMap<_,_>>(block_hash)
// 			.await
// 			.expect("could not get storage")
// 		;

// 		let bitmaps = bitmaps.into_iter()
// 			.map(|(k,v)| (k, v.bitmaps))
// 			.collect();

// 		let result : ElectionDataFor<Runtime, SolanaInstance> = ElectionData {
// 			bitmaps,
// 			_phantom: Default::default()
// 		};

// 		let traces = traces(result);

// 		println!("got election data: {traces:?}");

// 		*/

// 		Ok(())

// 	 }.boxed()).await.unwrap()
// }



