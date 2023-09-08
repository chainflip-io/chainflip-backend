//! Metric monitoring for the CFE
//! allowing prometheus server to query metrics from the CFE
//! Returns the metrics encoded in a prometheus format
//! Method returns a Sender, allowing graceful termination of the infinite loop

use super::{super::Port, task_scope};
use async_channel::{unbounded, Receiver, Sender};
use itertools::Itertools;
use lazy_static;
use prometheus::{
	core::MetricVecBuilder, register_int_counter_vec_with_registry,
	register_int_counter_with_registry, register_int_gauge_vec_with_registry,
	register_int_gauge_with_registry, IntCounter, IntCounterVec, IntGauge, IntGaugeVec, Opts,
	Registry,
};
use serde::Deserialize;
use std::net::IpAddr;
use tokio::spawn;
use tracing::info;
use warp::Filter;

pub enum DeleteMetricCommand {
	CounterPair(IntCounterVec, Vec<String>),
	GaugePair(IntGaugeVec, Vec<String>),
}

#[derive(Debug, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct Prometheus {
	pub hostname: String,
	pub port: Port,
}

fn collect_metric_to_delete() -> Vec<DeleteMetricCommand> {
	let mut metric_pair = vec![];
	while let Ok(msg) = DELETE_METRIC_CHANNEL.1.try_recv() {
		metric_pair.push(msg);
	}

	metric_pair
}

struct MetricCounter<'a> {
	metric: &'a IntCounterVec,
	labels: &'a [&'a str],
}

impl<'a> MetricCounter<'a> {
	fn new(metric: &'a IntCounterVec, labels: &'a [&'a str]) -> MetricCounter<'a> {
		MetricCounter { metric, labels }
	}

	fn inc(&self) {
		if let Ok(m) = self.metric.get_metric_with_label_values(self.labels) {
			m.inc();
		}
	}

	fn inc_by(&self, val: u64) {
		if let Ok(m) = self.metric.get_metric_with_label_values(self.labels) {
			m.inc_by(val);
		}
	}
}

impl<'a> Drop for MetricCounter<'a> {
	fn drop(&mut self) {
		let metric = self.metric.clone();
		let labels: Vec<String> = self.labels.iter().map(|s| s.to_string()).collect();
		tokio::spawn(async move {
			let _ = DELETE_METRIC_CHANNEL
				.0
				.send(DeleteMetricCommand::CounterPair(metric, labels))
				.await;
		});
	}
}

lazy_static::lazy_static! {
	static ref REGISTRY: Registry = Registry::new();

	pub static ref DELETE_METRIC_CHANNEL: (Sender<DeleteMetricCommand>, Receiver<DeleteMetricCommand>) = unbounded::<DeleteMetricCommand>();

	pub static ref RPC_RETRIER_REQUESTS: IntCounterVec = register_int_counter_vec_with_registry!(Opts::new("rpc_requests", "Count the rpc calls made by the engine, it doesn't keep into account the number of retrials"), &["client","rpc_method"], REGISTRY).unwrap();
	pub static ref RPC_RETRIER_TOTAL_REQUESTS: IntCounterVec = register_int_counter_vec_with_registry!(Opts::new("rpc_requests_total", "Count all the rpc calls made by the retrier, it counts every single call even if it is the same made multiple times"), &["client", "rpc_method"], REGISTRY).unwrap();

	pub static ref P2P_MSG_SENT: IntCounter = register_int_counter_with_registry!(Opts::new("p2p_msg_sent", "Count all the p2p msgs sent by the engine"), REGISTRY).unwrap();
	pub static ref P2P_MSG_RECEIVED: IntCounter = register_int_counter_with_registry!(Opts::new("p2p_msg_received", "Count all the p2p msgs received by the engine (raw before any processing)"), REGISTRY).unwrap();
	pub static ref P2P_RECONNECT_PEERS: IntGauge = register_int_gauge_with_registry!(Opts::new("p2p_reconnect_peers", "Count the number of peers we need to reconnect to"), REGISTRY).unwrap();
	pub static ref P2P_ACTIVE_CONNECTIONS: IntGauge = register_int_gauge_with_registry!(Opts::new("p2p_active_connections", "Count the number of active connections"), REGISTRY).unwrap();
	pub static ref P2P_MONITOR_EVENT: IntCounterVec = register_int_counter_vec_with_registry!(Opts::new("p2p_monitor_event", "Count the number of events observed by the zmq connection monitor"), &["event_type"], REGISTRY).unwrap();
	pub static ref P2P_ALLOWED_PUBKEYS: IntGauge = register_int_gauge_with_registry!(Opts::new("p2p_allowed_pubkeys", "Count the number of allowed pubkeys"), REGISTRY).unwrap();
	pub static ref P2P_DECLINED_CONNECTIONS: IntGauge = register_int_gauge_with_registry!(Opts::new("p2p_declined_connections", "Count the number times we decline a connection"), REGISTRY).unwrap();
	pub static ref P2P_BAD_MSG: IntCounterVec = register_int_counter_vec_with_registry!(Opts::new("p2p_bad_msg", "Count all the bad p2p msgs received by the engine and labels them by the reason they got discarded"), &["reason"], REGISTRY).unwrap();

	pub static ref UNAUTHORIZED_CEREMONY: IntGaugeVec = register_int_gauge_vec_with_registry!(Opts::new("unauthorized_ceremony", "Gauge keeping track of the number of unauthorized ceremony currently awaiting authorisation"), &["chain", "type"], REGISTRY).unwrap();
	pub static ref CEREMONY_BAD_MSG: IntCounterVec = register_int_counter_vec_with_registry!(Opts::new("ceremony_bad_msg", "Count all the bad msgs processed during a ceremony"), &["reason", "chain"], REGISTRY).unwrap();

	pub static ref CEREMONY_PROCESSED_MSG: IntCounterVec = register_int_counter_vec_with_registry!(Opts::new("ceremony_msg", "Count all the processed messages for a given ceremony"), &["ceremony_id"], REGISTRY).unwrap();
}

#[tracing::instrument(name = "prometheus-metric", skip_all)]
pub async fn start<'a, 'env>(
	scope: &'a task_scope::Scope<'env, anyhow::Error>,
	prometheus_settings: &'a Prometheus,
) -> Result<(), anyhow::Error> {
	info!("Starting");

	const PATH: &str = "metrics";

	let future = {
		warp::serve(warp::any().and(warp::path(PATH)).and(warp::path::end()).map(metrics_handler))
			.bind((prometheus_settings.hostname.parse::<IpAddr>()?, prometheus_settings.port))
	};

	scope.spawn_weak(async move {
		future.await;
		Ok(())
	});

	Ok(())
}

fn metrics_handler() -> String {
	use prometheus::Encoder;
	let encoder = prometheus::TextEncoder::new();

	let metric_pairs = collect_metric_to_delete();
	let mut buffer = Vec::new();
	if let Err(e) = encoder.encode(&REGISTRY.gather(), &mut buffer) {
		tracing::error!("could not encode custom metrics: {}", e);
	};
	let res = match String::from_utf8(buffer) {
		Ok(v) => v,
		Err(e) => {
			tracing::error!("custom metrics could not be from_utf8'd: {}", e);
			String::default()
		},
	};
	delete_labels(&metric_pairs);
	res
}

fn delete_labels(metric_pairs: &Vec<DeleteMetricCommand>) {
	for command in metric_pairs {
		match command {
			DeleteMetricCommand::CounterPair(metric, labels) => {
				let labels = labels.iter().map(|s| s.as_str()).collect::<Vec<&str>>();
				if let Err(e) = metric.remove_label_values(&labels) {
					tracing::error!("error removing label values: {}", e);
				}
			},
			DeleteMetricCommand::GaugePair(metric, labels) => {
				let labels = labels.iter().map(|s| s.as_str()).collect::<Vec<&str>>();
				if let Err(e) = metric.remove_label_values(&labels) {
					tracing::error!("error removing label values: {}", e);
				}
			},
		}
	}
}

#[cfg(test)]
mod test {
	use crate::metrics;

	use super::*;
	use futures::FutureExt;

	#[tokio::test]
	async fn prometheus_test() {
		let prometheus_settings = Prometheus { hostname: "0.0.0.0".to_string(), port: 5566 };
		let metric = create_and_register_metric();

		let _ = DELETE_METRIC_CHANNEL
			.0
			.send(DeleteMetricCommand::CounterPair(metric.clone(), ["A".to_string()].to_vec()))
			.await;
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

				request_test("metrics", reqwest::StatusCode::OK, "# HELP test test help\n# TYPE test counter\ntest{label=\"A\"} 1\ntest{label=\"B\"} 10\ntest{label=\"C\"} 100\n").await;
				request_test("invalid", reqwest::StatusCode::NOT_FOUND, "").await;
				let _ = DELETE_METRIC_CHANNEL.0.send(DeleteMetricCommand::CounterPair(metric.clone(), ["C".to_string()].to_vec())).await;
				request_test("metrics", reqwest::StatusCode::OK, "# HELP test test help\n# TYPE test counter\ntest{label=\"B\"} 10\ntest{label=\"C\"} 100\n").await;
				request_test("metrics", reqwest::StatusCode::OK, "# HELP test test help\n# TYPE test counter\ntest{label=\"B\"} 10\n").await;

				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap();
	}

	fn create_and_register_metric() -> IntCounterVec {
		let metric = metrics::create_and_register_counter_vec("test", "test help", &["label"]);
		metric.with_label_values(&["A"]).inc();
		metric.with_label_values(&["B"]).inc_by(10);
		metric.with_label_values(&["C"]).inc_by(100);

		assert_eq!(metric.with_label_values(&["A"]).get(), 1);
		assert_eq!(metric.with_label_values(&["B"]).get(), 10);
		assert_eq!(metric.with_label_values(&["C"]).get(), 100);

		metric
	}
}
