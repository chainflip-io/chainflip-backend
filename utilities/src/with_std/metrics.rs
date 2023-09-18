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

/// wrapper used to enforce the correct conversion to i64 when setting a specific value for a gauge
pub struct IntGaugeWrapper {
	pub prom_metric: IntGauge,
}

impl IntGaugeWrapper {
	fn new(name: &str, help: &str, registry: &REGISTRY) -> IntGaugeWrapper {
		IntGaugeWrapper {
			prom_metric: register_int_gauge_with_registry!(Opts::new(name, help), registry)
				.expect("A duplicate metric collector has already been registered."),
		}
	}

	pub fn inc(&self) {
		self.prom_metric.inc();
	}

	pub fn dec(&self) {
		self.prom_metric.dec();
	}

	pub fn set<T: TryInto<i64>>(&self, val: T)
	where
		<T as TryInto<i64>>::Error: std::fmt::Debug,
	{
		match val.try_into() {
			Ok(val) => self.prom_metric.set(val),
			Err(e) => tracing::error!("Conversion to i64 failed: {:?}", e),
		}
	}
}

/// wrapper used to enforce the correct number of labels when interacting with an IntGaugeVec
pub struct IntGaugeVecWrapper<const N: usize> {
	pub prom_metric: IntGaugeVec,
}

impl<const N: usize> IntGaugeVecWrapper<N> {
	fn new(
		name: &str,
		help: &str,
		labels: &[&str; N],
		registry: &REGISTRY,
	) -> IntGaugeVecWrapper<N> {
		IntGaugeVecWrapper {
			prom_metric: register_int_gauge_vec_with_registry!(
				Opts::new(name, help),
				labels,
				registry
			)
			.expect("A duplicate metric collector has already been registered."),
		}
	}

	pub fn inc(&self, labels: &[&str; N]) {
		match self.prom_metric.get_metric_with_label_values(labels) {
			Ok(m) => m.inc(),
			Err(e) => tracing::error!("Failed to get the metric: {}", e),
		}
	}

	pub fn dec(&self, labels: &[&str; N]) {
		match self.prom_metric.get_metric_with_label_values(labels) {
			Ok(m) => m.dec(),
			Err(e) => tracing::error!("Failed to get the metric: {}", e),
		}
	}

	pub fn set<T: TryInto<i64>>(&self, labels: &[&str; N], val: T)
	where
		<T as TryInto<i64>>::Error: std::fmt::Debug,
	{
		match val.try_into() {
			Ok(val) => match self.prom_metric.get_metric_with_label_values(labels) {
				Ok(m) => m.set(val),
				Err(e) => tracing::error!("Failed to get the metric: {}", e),
			},
			Err(e) => tracing::error!("Conversion to i64 failed: {:?}", e),
		}
	}
}

#[derive(Clone)]
/// wrapper used to enforce the correct number of labels when interacting with an IntCounterVec
pub struct IntCounterVecWrapper<const N: usize> {
	pub prom_metric: IntCounterVec,
}

impl<const N: usize> IntCounterVecWrapper<N> {
	fn new(
		name: &str,
		help: &str,
		labels: &[&str; N],
		registry: &REGISTRY,
	) -> IntCounterVecWrapper<N> {
		IntCounterVecWrapper {
			prom_metric: register_int_counter_vec_with_registry!(
				Opts::new(name, help),
				labels,
				registry
			)
			.expect("A duplicate metric collector has already been registered."),
		}
	}

	pub fn inc(&self, labels: &[&str; N]) {
		match self.prom_metric.get_metric_with_label_values(labels) {
			Ok(m) => m.inc(),
			Err(e) => tracing::error!("Failed to get the metric: {}", e),
		}
	}
}
macro_rules! build_gauge_vec {
	($metric_ident:ident, $name:literal, $help:literal, $labels:tt) => {
		lazy_static::lazy_static!{
			pub static ref $metric_ident: IntGaugeVecWrapper<{ $labels.len() }> = IntGaugeVecWrapper::new($name, $help, &$labels, &REGISTRY);
		}
	}
}

macro_rules! build_counter_vec {
	($metric_ident:ident, $name:literal, $help:literal, $labels:tt) => {
		lazy_static::lazy_static!{
			pub static ref $metric_ident: IntCounterVecWrapper<{ $labels.len() }> = IntCounterVecWrapper::new($name, $help, &$labels, &REGISTRY);
		}
	}
}

macro_rules! build_gauge_vec_struct {
	($metric_ident:ident, $struct_ident:ident, $name:literal, $help:literal, $labels:tt) => {
		build_gauge_vec!($metric_ident, $name, $help, $labels);
   
		#[derive(Clone)]
		pub struct $struct_ident {
			metric: &'static $metric_ident,
			labels: [String; { $labels.len() }],
		}
		impl $struct_ident {
			pub fn new(metric: &'static $metric_ident, labels: [String; { $labels.len() }]) -> $struct_ident {
				$struct_ident { metric, labels }
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
		impl Drop for $struct_ident {
			fn drop(&mut self) {
				let metric = self.metric.metric.clone();
				let labels: Vec<String> = self.labels.iter().map(|s| s.to_string()).collect();
   
				DELETE_METRIC_CHANNEL
					.0
					.try_send(DeleteMetricCommand::GaugePair(metric, labels))
					.expect("DELETE_METRIC_CHANNEL should never be closed!");
			}
		}
	};
}	
/// The idea behind this macro is to help to create the wrapper for the metrics at compile time,
/// without having to specify number of labels etc, but still enforcing the correct use of the
/// metric there are 2 possibilities here:
/// - a metric with some const labels value -> these metrics get created specifying the const label
///   values and when used we need to specify the value for the other labels these metrics are kept
///   around even after being dropped, these type of metrics are used because
/// it simplify referring some values at runtime which wouldn't be available otherwise
/// - a metric with no const values -> these metrics are created with all the necessary labels
///   supplied, when interacting with them we don't have to specify any labels anymore when these
///   metrics go out of scope and get dropped the label combination is also deleted (we
/// won't refer to that specific combination ever again)
macro_rules! build_counter_vec_struct {
	($metric_ident:ident, $struct_ident:ident, $name:literal, $help:literal, $labels:tt) => {
		build_counter_vec!($metric_ident, $name, $help, $labels);

		#[derive(Clone)]
		pub struct $struct_ident {
			metric: &'static $metric_ident,
			labels: [String; { $labels.len() }],
		}
		impl $struct_ident {
			pub fn new(
				metric: &'static $metric_ident,
				labels: [String; { $labels.len() }],
			) -> $struct_ident {
				$struct_ident { metric, labels }
			}

			pub fn inc(&self) {
				let labels = self.labels.each_ref().map(|s| s.as_str());
				self.metric.inc(&labels);
			}
		}
		impl Drop for $struct_ident {
			fn drop(&mut self) {
				let metric = self.metric.prom_metric.clone();
				let labels: Vec<String> = self.labels.iter().map(|s| s.to_string()).collect();

				DELETE_METRIC_CHANNEL
					.0
					.try_send(DeleteMetricCommand::CounterPair(metric, labels))
					.expect("DELETE_METRIC_CHANNEL should never be closed!");
			}
		}
	};
	($metric_ident:ident, $structNotDrop:ident, $name:literal, $help:literal, $labels:tt, $const_labels:tt) => {
		build_counter_vec!($metric_ident, $name, $help, $labels);

		#[derive(Clone)]
		pub struct $structNotDrop {
			metric: &'static $metric_ident,
			const_labels: [&'static str; { $const_labels.len() }],
		}
		impl $structNotDrop {
			pub fn new(
				metric: &'static $metric_ident,
				const_labels: [&'static str; { $const_labels.len() }],
			) -> $structNotDrop {
				$structNotDrop { metric, const_labels }
			}

			pub fn inc(&self, non_const_labels: &[&str; { $labels.len() - $const_labels.len() }]) {
				let labels: [&str; { $labels.len() }] = {
					let mut whole: [&str; { $labels.len() }] = [""; { $labels.len() }];
					let (one, two) = whole.split_at_mut(self.const_labels.len());
					one.copy_from_slice(&self.const_labels);
					two.copy_from_slice(non_const_labels);
					whole
				};
				self.metric.inc(&labels);
			}
		}
	};
}

lazy_static::lazy_static! {
	static ref REGISTRY: Registry = Registry::new();
	pub static ref DELETE_METRIC_CHANNEL: (Sender<DeleteMetricCommand>, Receiver<DeleteMetricCommand>) = unbounded::<DeleteMetricCommand>();

	pub static ref P2P_MSG_SENT: IntCounter = register_int_counter_with_registry!(Opts::new("p2p_msg_sent", "Count all the p2p msgs sent by the engine"), REGISTRY).expect("A duplicate metric collector has already been registered.");
	pub static ref P2P_MSG_RECEIVED: IntCounter = register_int_counter_with_registry!(Opts::new("p2p_msg_received", "Count all the p2p msgs received by the engine (raw before any processing)"), REGISTRY).expect("A duplicate metric collector has already been registered.");
	pub static ref P2P_RECONNECT_PEERS: IntGaugeWrapper = IntGaugeWrapper::new("p2p_reconnect_peers", "Count the number of peers we need to reconnect to", &REGISTRY);
	pub static ref P2P_ACTIVE_CONNECTIONS: IntGaugeWrapper = IntGaugeWrapper::new("p2p_active_connections", "Count the number of active connections", &REGISTRY);
	pub static ref P2P_ALLOWED_PUBKEYS: IntGaugeWrapper = IntGaugeWrapper::new("p2p_allowed_pubkeys", "Count the number of allowed pubkeys", &REGISTRY);
	pub static ref P2P_DECLINED_CONNECTIONS: IntCounter = register_int_counter_with_registry!(Opts::new("p2p_declined_connections", "Count the number times we decline a connection"), &REGISTRY).expect("A duplicate metric collector has already been registered.");
}

build_gauge_vec!(
	UNAUTHORIZED_CEREMONY,
	"unauthorized_ceremony",
	"Gauge keeping track of the number of unauthorized ceremony currently awaiting authorisation",
	["chain", "type"]
);
build_counter_vec!(
	RPC_RETRIER_REQUESTS,
	"rpc_requests",
	"Count the rpc calls made by the engine, it doesn't keep into account the number of retrials",
	["client", "rpc_method"]
);
build_counter_vec!(
	RPC_RETRIER_TOTAL_REQUESTS,
	"rpc_requests_total",
	"Count all the rpc calls made by the retrier, it counts every single call even if it is the same made multiple times",
	["client","rpc_method"]
);
build_counter_vec!(
	P2P_MONITOR_EVENT,
	"p2p_monitor_event",
	"Count the number of events observed by the zmq connection monitor",
	["event_type"]
);
build_counter_vec!(
	P2P_BAD_MSG,
	"p2p_bad_msg",
	"Count all the bad p2p msgs received by the engine and labels them by the reason they got discarded",
	["reason"]
);
build_counter_vec_struct!(
	CEREMONY_PROCESSED_MSG,
	CeremonyProcessedMsgDrop,
	"ceremony_msg",
	"Count all the processed messages for a given ceremony",
	["ceremony_id"]
);
build_counter_vec_struct!(
	CEREMONY_BAD_MSG,
	CeremonyBadMsgNotDrop,
	"ceremony_bad_msg",
	"Count all the bad msgs processed during a ceremony",
	["chain", "reason"],
	["chain"] //const labels
);

/// structure containing the metrics used during a ceremony
#[derive(Clone)]
pub struct CeremonyMetrics {
	pub processed_messages: CeremonyProcessedMsgDrop,
	pub bad_message: CeremonyBadMsgNotDrop,
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
