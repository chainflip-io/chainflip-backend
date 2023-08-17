//! Metric monitoring for the CFE
//! allowing prometheus server to query metrics from the CFE
//! Returns the metrics encoded in a prometheus format
//! Method returns a Sender, allowing graceful termination of the infinite loop

use std::net::IpAddr;

// use crate::settings;
use lazy_static;
use prometheus::{IntCounterVec, IntGauge, Opts, Registry};
use tracing::info;
use utilities::task_scope;
use warp::Filter;

lazy_static::lazy_static! {
	static ref REGISTRY: Registry = Registry::new();
	pub static ref RPC_COUNTER: IntCounterVec = IntCounterVec::new(Opts::new("rpc_counter", "Count of all the rpc calls made by the rpcClient"), &["rpcClient", "rpcMethod"]).expect("Metric succesfully created");
	// not used for now
	pub static ref METRIC_GAUGE: IntGauge = IntGauge::new("metric2", "help2").expect("Metric succesfully created");
}

#[tracing::instrument(name = "prometheus-metric", skip_all)]
pub async fn start<'a, 'env>(
	scope: &'a task_scope::Scope<'env, anyhow::Error>,
	// prometheus_settings: &'a settings::Prometheus,
) -> Result<(), anyhow::Error> {
	info!("Starting");
	let hostname = "127.0.0.1".to_string();
	let port: u16 = 5566;
	const PATH: &str = "metrics";

	let future = warp::serve(
		warp::any()
			.and(warp::path(PATH))
			.and(warp::path::end())
			.map(move || metrics_handler()),
	)
	// .bind((prometheus_settings.hostname.parse::<IpAddr>()?, prometheus_settings.port));
	.bind((hostname.parse::<IpAddr>()?, port));

	scope.spawn_weak(async move {
		future.await;
		Ok(())
	});

	Ok(())
}

fn metrics_handler() -> String {
	use prometheus::Encoder;
	let encoder = prometheus::TextEncoder::new();

	let mut buffer = Vec::new();
	if let Err(e) = encoder.encode(&REGISTRY.gather(), &mut buffer) {
		eprintln!("could not encode custom metrics: {}", e);
	};
	let res = match String::from_utf8(buffer) {
		Ok(v) => v,
		Err(e) => {
			eprintln!("custom metrics could not be from_utf8'd: {}", e);
			String::default()
		},
	};

	res
}

pub fn register_metrics() {
	REGISTRY
		.register(Box::new(RPC_COUNTER.clone()))
		.expect("Metric succesfully register");
	REGISTRY
		.register(Box::new(METRIC_GAUGE.clone()))
		.expect("Metric succesfully register");
}
