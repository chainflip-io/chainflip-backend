//! Metric monitoring for the CFE
//! allowing prometheus server to query metrics from the CFE
//! Returns the metrics encoded in a prometheus format
//! Method returns a Sender, allowing graceful termination of the infinite loop

use std::net::IpAddr;

use crate::settings;
use lazy_static;
use prometheus::{IntCounter, IntCounterVec, IntGauge, Opts, Registry};
use tracing::info;
use utilities::task_scope;
use warp::Filter;

lazy_static::lazy_static! {
	pub static ref REGISTRY: Registry = Registry::new();

	pub static ref RPC_RETRIER_REQUESTS: IntCounterVec = create_and_register_counter_vec("rpc_requests", "Count the rpc calls made by the engine, it doesn't keep into account the number of retrials", &["client","rpcMethod"]);
	pub static ref RPC_RETRIER_TOTAL_REQUESTS: IntCounterVec = create_and_register_counter_vec("rpc_requests_total", "Count all the rpc calls made by the retrier, it counts every single call even if it is the same made multiple times", &["client", "rpcMethod"]);

	pub static ref P2P_MSG_SENT: IntCounter = create_and_register_counter("p2p_msg_sent", "Count all the p2p msgs sent by the engine");
	pub static ref P2P_MSG_RECEIVED: IntCounter = create_and_register_counter("p2p_msg_received", "Count all the p2p msgs received by the engine (raw before any processing)");
	pub static ref P2P_RECONNECT_PEERS: IntGauge = create_and_register_gauge("p2p_reconnect_peers", "Count the number of peers we need to reconnect to");
	pub static ref P2P_ACTIVE_CONNECTIONS: IntGauge = create_and_register_gauge("p2p_active_connections", "Count the number of active connections");
	pub static ref P2P_MONITOR_EVENT: IntCounterVec = create_and_register_counter_vec("p2p_monitor_event", "Count the number of events received from the engine/monitor", &["eventType"]);
	pub static ref P2P_ALLOWED_PUBKEYS: IntGauge = create_and_register_gauge("p2p_allowed_pubkeys", "Count the number of allowed pubkeys");
	pub static ref P2P_DECLINED_CONNECTIONS: IntGauge = create_and_register_gauge("p2p_declined_connections", "Count the number times we decline a connection");

	pub static ref P2P_BAD_MSG: IntCounterVec = create_and_register_counter_vec("p2p_bad_msg", "Count all the bad p2p msgs received by the engine and labels them by the reason they got discarded", &["reason"]);
}

fn create_and_register_counter_vec(name: &str, help: &str, labels: &[&str]) -> IntCounterVec {
	let m = IntCounterVec::new(Opts::new(name, help), labels).expect("Metric succesfully created");
	REGISTRY.register(Box::new(m.clone())).expect("Metric succesfully register");
	m
}

fn create_and_register_counter(name: &str, help: &str) -> IntCounter {
	let m = IntCounter::new(name, help).expect("Metric succesfully created");
	REGISTRY.register(Box::new(m.clone())).expect("Metric succesfully register");
	m
}

fn create_and_register_gauge(name: &str, help: &str) -> IntGauge {
	let m = IntGauge::new(name, help).expect("Metric succesfully created");
	REGISTRY.register(Box::new(m.clone())).expect("Metric succesfully register");
	m
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
		tracing::error!("could not encode custom metrics: {}", e);
	};
	match String::from_utf8(buffer) {
		Ok(v) => v,
		Err(e) => {
			tracing::error!("custom metrics could not be from_utf8'd: {}", e);
			String::default()
		},
	}
}

#[cfg(test)]
mod test {
	use crate::metrics;

	use futures_util::FutureExt;

	use crate::settings::Settings;

	use super::*;

	#[tokio::test]
	async fn prometheus_test() {
		let prometheus_settings = Settings::new_test().unwrap().prometheus.unwrap();
		create_and_register_metric();

		task_scope::task_scope(|scope| {
			async {
				start(scope, &prometheus_settings).await.unwrap();

				let request_test = |path: &'static str,
				                    expected_status: reqwest::StatusCode,
				                    expected_text: &'static str| {
					let prometheus_settings = prometheus_settings.clone();

					async move {
						let resp = reqwest::get(&format!(
							"http://{}:{}/{}",
							&prometheus_settings.hostname, &prometheus_settings.port, path
						))
						.await
						.unwrap();

						assert_eq!(expected_status, resp.status());
						assert_eq!(resp.text().await.unwrap(), expected_text);
					}
				};

				request_test("metrics", reqwest::StatusCode::OK, "# HELP test test help\n# TYPE test counter\ntest{label=\"A\"} 1\ntest{label=\"B\"} 10\n").await;
				request_test("invalid", reqwest::StatusCode::NOT_FOUND, "").await;

				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap();
	}

	fn create_and_register_metric() {
		let metric = metrics::create_and_register_counter_vec("test", "test help", &["label"]);
		metric.with_label_values(&["A"]).inc();
		metric.with_label_values(&["B"]).inc_by(10);

		assert_eq!(metric.with_label_values(&["A"]).get(), 1);
		assert_eq!(metric.with_label_values(&["B"]).get(), 10);
	}
}
