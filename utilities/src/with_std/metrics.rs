//! Metric monitoring for the CFE
//! allowing prometheus server to query metrics from the CFE
//! Returns the metrics encoded in a prometheus format
//! Method returns a Sender, allowing graceful termination of the infinite loop
use super::{super::Port, task_scope};
use async_channel::{unbounded, Receiver, Sender};
use lazy_static;
use prometheus::{
	register_int_counter_vec_with_registry, register_int_counter_with_registry,
	register_int_gauge_vec_with_registry, register_int_gauge_with_registry, IntCounter,
	IntCounterVec, IntGauge, IntGaugeVec, Opts, Registry,
};
use serde::Deserialize;
use std::net::IpAddr;
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
/// wrapper around a Gauge which needs to be deleted later on
/// labels are defined only at creation, allowing the code to be more clear and less error prone
#[derive(Clone)]
pub struct MetricGauge<const N: usize> {
	metric: &'static IntGaugeVecWrapper<N>,
	labels: [String; N],
}

impl<const N: usize> MetricGauge<N> {
	pub fn new(metric: &'static IntGaugeVecWrapper<N>, labels: [String; N]) -> MetricGauge<N> {
		MetricGauge { metric, labels }
	}

	pub fn inc(&self) {
		let labels = self.labels.each_ref().map(|s| s.as_str());
		self.metric.inc(&labels);
	}

	pub fn dec(&self) {
		let labels = self.labels.each_ref().map(|s| s.as_str());
		self.metric.dec(&labels);
	}

	pub fn set(&self, val: i64) {
		let labels = self.labels.each_ref().map(|s| s.as_str());
		self.metric.set(&labels, val);
	}
}

impl<const N: usize> Drop for MetricGauge<N> {
	fn drop(&mut self) {
		let metric = self.metric.metric.clone();
		let labels: Vec<String> = self.labels.iter().map(|s| s.to_string()).collect();

		let _ = DELETE_METRIC_CHANNEL.0.try_send(DeleteMetricCommand::GaugePair(metric, labels));
	}
}

/// wrapper used to enforce the correct number of labels when interacting with an IntGaugeVec
pub struct IntGaugeVecWrapper<const N: usize> {
	pub metric: IntGaugeVec,
}

impl<const N: usize> IntGaugeVecWrapper<N> {
	fn new(name: &str, help: &str, labels: &[&str; N], registry: &REGISTRY) -> IntGaugeVecWrapper<N> {
		IntGaugeVecWrapper { metric: register_int_gauge_vec_with_registry!(Opts::new(name, help), labels, registry).unwrap() }
	}

	pub fn inc(&self, labels: &[&str; N]) {
		if let Ok(m) = self.metric.get_metric_with_label_values(labels) {
			m.inc();
		};
	}

	pub fn dec(&self, labels: &[&str; N]) {
		if let Ok(m) = self.metric.get_metric_with_label_values(labels) {
			m.dec();
		};
	}

	pub fn set(&self, labels: &[&str; N], val: i64) {
		if let Ok(m) = self.metric.get_metric_with_label_values(labels) {
			m.set(val);
		};
	}
}
/// Wrapper around a counter which doesn't need to be deleted and have some labels that are always
/// the same relative to where the wrapper is used and some that aren't and need to be specified
/// when we interact with the metric
#[derive(Clone)]
pub struct MetricCounterNotToDrop<const N: usize, const C: usize, const T: usize> {
	metric: &'static IntCounterVecWrapper<N>,
	const_labels: [&'static str; C],
}
impl<const N: usize, const C: usize, const T: usize> MetricCounterNotToDrop<N, C, T> {
	pub fn new(
		metric: &'static IntCounterVecWrapper<N>,
		const_labels: [&'static str; C],
	) -> MetricCounterNotToDrop<N, C, T> {
		MetricCounterNotToDrop { metric, const_labels }
	}

	pub fn inc(&self, non_const_labels: &[&str; T]) {
		//TODO: Check best way to concatenate 2 array?
		//Probably the following one is not the best
		let labels: [&str; N] = {
			let mut whole: [&str; N] = [""; N];
			let (one, two) = whole.split_at_mut(self.const_labels.len());
			one.copy_from_slice(&self.const_labels);
			two.copy_from_slice(non_const_labels);
			whole
		};
		self.metric.inc(&labels);
	}
}
/// wrapper around a Counter which needs to be deleted later on
/// labels are defined only at creation, allowing the code to be more clear and less error prone
#[derive(Clone)]
pub struct MetricCounter<const N: usize> {
	metric: &'static IntCounterVecWrapper<N>,
	labels: [String; N],
}

impl<const N: usize> MetricCounter<N> {
	pub fn new(metric: &'static IntCounterVecWrapper<N>, labels: [String; N]) -> MetricCounter<N> {
		MetricCounter { metric, labels }
	}

	pub fn inc(&self) {
		let labels = self.labels.each_ref().map(|s| s.as_str());
		self.metric.inc(&labels);
	}
}

impl<const N: usize> Drop for MetricCounter<N> {
	fn drop(&mut self) {
		let metric = self.metric.metric.clone();
		let labels: Vec<String> = self.labels.iter().map(|s| s.to_string()).collect();

		let _ = DELETE_METRIC_CHANNEL
			.0
			.try_send(DeleteMetricCommand::CounterPair(metric, labels));
	}
}

/// wrapper used to enforce the correct number of labels when interacting with an IntCounterVec
pub struct IntCounterVecWrapper<const N: usize> {
	pub metric: IntCounterVec,
}

impl<const N: usize> IntCounterVecWrapper<N> {
	fn new(name: &str, help: &str, labels: &[&str; N], registry: &REGISTRY) -> IntCounterVecWrapper<N> {
		IntCounterVecWrapper { metric: register_int_counter_vec_with_registry!(Opts::new(name, help), labels, registry).unwrap() }
	}

	pub fn inc(&self, labels: &[&str; N]) {
		if let Ok(m) = self.metric.get_metric_with_label_values(labels) {
			m.inc();
		};
	}
}

/// structure containing the metrics used during a ceremony
/// TODO: expand it with the new metrics measuring ceremony duration
#[derive(Clone)]
pub struct CeremonyMetrics {
	pub processed_messages: MetricCounter<1>,
	pub bad_message: MetricCounterNotToDrop<2, 1, 1>,
}

lazy_static::lazy_static! {
	static ref REGISTRY: Registry = Registry::new();

	pub static ref DELETE_METRIC_CHANNEL: (Sender<DeleteMetricCommand>, Receiver<DeleteMetricCommand>) = unbounded::<DeleteMetricCommand>();

	pub static ref RPC_RETRIER_REQUESTS: IntCounterVecWrapper<2> = IntCounterVecWrapper::new("rpc_requests", "Count the rpc calls made by the engine, it doesn't keep into account the number of retrials", &["client","rpc_method"], &REGISTRY);
	pub static ref RPC_RETRIER_TOTAL_REQUESTS: IntCounterVecWrapper<2> = IntCounterVecWrapper::new("rpc_requests_total", "Count all the rpc calls made by the retrier, it counts every single call even if it is the same made multiple times", &["client", "rpc_method"], &REGISTRY);

	pub static ref P2P_MSG_SENT: IntCounter = register_int_counter_with_registry!(Opts::new("p2p_msg_sent", "Count all the p2p msgs sent by the engine"), REGISTRY).unwrap();
	pub static ref P2P_MSG_RECEIVED: IntCounter = register_int_counter_with_registry!(Opts::new("p2p_msg_received", "Count all the p2p msgs received by the engine (raw before any processing)"), REGISTRY).unwrap();
	pub static ref P2P_RECONNECT_PEERS: IntGauge = register_int_gauge_with_registry!(Opts::new("p2p_reconnect_peers", "Count the number of peers we need to reconnect to"), REGISTRY).unwrap();
	pub static ref P2P_ACTIVE_CONNECTIONS: IntGauge = register_int_gauge_with_registry!(Opts::new("p2p_active_connections", "Count the number of active connections"), REGISTRY).unwrap();
	pub static ref P2P_ALLOWED_PUBKEYS: IntGauge = register_int_gauge_with_registry!(Opts::new("p2p_allowed_pubkeys", "Count the number of allowed pubkeys"), REGISTRY).unwrap();
	pub static ref P2P_DECLINED_CONNECTIONS: IntGauge = register_int_gauge_with_registry!(Opts::new("p2p_declined_connections", "Count the number times we decline a connection"), REGISTRY).unwrap();
	pub static ref P2P_MONITOR_EVENT: IntCounterVecWrapper<1> = IntCounterVecWrapper::new("p2p_monitor_event", "Count the number of events observed by the zmq connection monitor", &["event_type"], &REGISTRY);
	pub static ref P2P_BAD_MSG: IntCounterVecWrapper<1> = IntCounterVecWrapper::new("p2p_bad_msg", "Count all the bad p2p msgs received by the engine and labels them by the reason they got discarded", &["reason"], &REGISTRY);

	pub static ref UNAUTHORIZED_CEREMONY: IntGaugeVecWrapper<2> = IntGaugeVecWrapper::new("unauthorized_ceremony", "Gauge keeping track of the number of unauthorized ceremony currently awaiting authorisation", &["chain", "type"], &REGISTRY);
	pub static ref CEREMONY_BAD_MSG: IntCounterVecWrapper<2> = IntCounterVecWrapper::new("ceremony_bad_msg", "Count all the bad msgs processed during a ceremony", &["reason", "chain"], &REGISTRY);

	pub static ref CEREMONY_PROCESSED_MSG: IntCounterVecWrapper<1> = IntCounterVecWrapper::new("ceremony_msg", "Count all the processed messages for a given ceremony", &["ceremony_id"], &REGISTRY);
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
		let metric = register_int_counter_vec_with_registry!(
			Opts::new("test", "test help"),
			&["label"],
			REGISTRY
		)
		.unwrap();
		metric.with_label_values(&["A"]).inc();
		metric.with_label_values(&["B"]).inc_by(10);
		metric.with_label_values(&["C"]).inc_by(100);

		assert_eq!(metric.with_label_values(&["A"]).get(), 1);
		assert_eq!(metric.with_label_values(&["B"]).get(), 10);
		assert_eq!(metric.with_label_values(&["C"]).get(), 100);

		metric
	}
}
