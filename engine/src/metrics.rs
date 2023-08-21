//! Metric monitoring for the CFE
//! allowing prometheus server to query metrics from the CFE
//! Returns the metrics encoded in a prometheus format
//! Method returns a Sender, allowing graceful termination of the infinite loop

use std::net::IpAddr;

use crate::settings;
use lazy_static;
use prometheus::{IntCounterVec, Opts, Registry};
use tracing::info;
use utilities::task_scope;
use warp::Filter;

lazy_static::lazy_static! {
	pub static ref RPC_RETRIER_REQUESTS: IntCounterVec = IntCounterVec::new(Opts::new("rpc_requests", "Count the rpc calls made by the retrier, it doesn't keep into account the number of retrials"), &["client","rpcMethod"]).expect("Metric succesfully created");
	pub static ref RPC_RETRIER_TOTAL_REQUESTS: IntCounterVec = IntCounterVec::new(Opts::new("rpc_requests_total", "Count all the rpc calls made by the retrier, it counts every single call even if it is the same made multiple times"), &["client", "rpcMethod"]).expect("Metric succesfully created");
	static ref REGISTRY: Registry = { 
		let reg = Registry::new();
		reg.register(Box::new(RPC_RETRIER_REQUESTS.clone()))
			.expect("Metric succesfully register");
		reg.register(Box::new(RPC_RETRIER_TOTAL_REQUESTS.clone()))
			.expect("Metric succesfully register");
		reg
	};
}

#[tracing::instrument(name = "prometheus-metric", skip_all)]
pub async fn start<'a, 'env>(
	scope: &'a task_scope::Scope<'env, anyhow::Error>,
	prometheus_settings: &'a settings::Prometheus,
) -> Result<(), anyhow::Error> {
	info!("Starting");

	const PATH: &str = "metrics";

	let future =
		warp::serve(warp::any().and(warp::path(PATH)).and(warp::path::end()).map(metrics_handler))
			.bind((prometheus_settings.hostname.parse::<IpAddr>()?, prometheus_settings.port));

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
	match String::from_utf8(buffer) {
		Ok(v) => v,
		Err(e) => {
			eprintln!("custom metrics could not be from_utf8'd: {}", e);
			String::default()
		},
	}
}