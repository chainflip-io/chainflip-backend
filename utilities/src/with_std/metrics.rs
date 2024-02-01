//! Metric monitoring for the CFE
//! allowing prometheus server to query metrics from the CFE
//! Returns the metrics encoded in a prometheus format
//! Method returns a Sender, allowing graceful termination of the infinite loop
use super::{super::Port, task_scope};
use crate::ArrayCollect;
use lazy_static;
use prometheus::{
	register_histogram_vec_with_registry, register_int_counter_vec_with_registry,
	register_int_counter_with_registry, register_int_gauge_vec_with_registry,
	register_int_gauge_with_registry, HistogramVec, IntCounter, IntCounterVec, IntGauge,
	IntGaugeVec, Opts, Registry,
};
use serde::Deserialize;
use std::{net::IpAddr, time::Duration};
use tracing::info;
use warp::Filter;

#[derive(Debug, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct Prometheus {
	pub hostname: String,
	pub port: Port,
}

/// wrapper around histogram to enforce correct conversion to f64 when observing a value
pub struct HistogramVecWrapper<const N: usize> {
	pub prom_metric: HistogramVec,
}

impl<const N: usize> HistogramVecWrapper<N> {
	fn new(
		name: &str,
		help: &str,
		labels: &[&str; N],
		buckets: Vec<f64>,
		registry: &REGISTRY,
	) -> HistogramVecWrapper<N> {
		HistogramVecWrapper {
			prom_metric: register_histogram_vec_with_registry!(
				name, help, labels, buckets, registry
			)
			.expect("A duplicate metric collector has already been registered."),
		}
	}

	pub fn observe(&self, labels: &[&str; N], val: Duration) {
		let sample_value: f64 = val.as_secs_f64();
		self.prom_metric.with_label_values(labels).observe(sample_value);
	}
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

macro_rules! build_histogram_vec {
	($metric_ident:ident, $name:literal, $help:literal, $labels:tt, $buckets:tt) => {
		lazy_static::lazy_static!{
			pub static ref $metric_ident: HistogramVecWrapper<{ $labels.len() }> = HistogramVecWrapper::new($name, $help, &$labels, $buckets, &REGISTRY);
		}
	}
}

macro_rules! build_histogram_vec_struct {
	($metric_ident:ident, $struct_ident:ident, $name:literal, $help:literal, $labels:tt, $buckets:tt) => {
		build_histogram_vec!($metric_ident, $name, $help, $labels, $buckets);

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

			pub fn observe(&self, val: Duration) {
				let labels = self.labels.each_ref().map(|s| s.as_str());
				self.metric.observe(&labels, val);
			}
		}
	};
	($metric_ident:ident, $struct_ident:ident, $name:literal, $help:literal, $labels:tt, $const_labels:tt, $buckets:tt) => {
		build_histogram_vec!($metric_ident, $name, $help, $labels, $buckets);

		#[derive(Clone)]
		pub struct $struct_ident {
			metric: &'static $metric_ident,
			const_labels: [String; { $const_labels.len() }],
		}
		impl $struct_ident {
			pub fn new(
				metric: &'static $metric_ident,
				const_labels: [String; { $const_labels.len() }],
			) -> $struct_ident {
				$struct_ident { metric, const_labels }
			}

			pub fn observe(
				&self,
				non_const_labels: &[&str; { $labels.len() - $const_labels.len() }],
				val: Duration,
			) {
				let labels: [&str; { $labels.len() }] = self
					.const_labels
					.iter()
					.map(|s| s.as_str())
					.chain(*non_const_labels)
					.collect_array();
				self.metric.observe(&labels, val);
			}
		}
	};
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

			pub fn dec(&self) {
				let labels = self.labels.each_ref().map(|s| s.as_str());
				self.metric.dec(&labels);
			}

			pub fn set<T: TryInto<i64>>(&self, val: T)
			where
				<T as TryInto<i64>>::Error: std::fmt::Debug,
			{
				let labels = self.labels.each_ref().map(|s| s.as_str());
				self.metric.set(&labels, val);
			}
		}
	};
	($metric_ident:ident, $struct_ident:ident, $name:literal, $help:literal, $labels:tt, $const_labels:tt) => {
		build_gauge_vec!($metric_ident, $name, $help, $labels);

		#[derive(Clone)]
		pub struct $struct_ident {
			metric: &'static $metric_ident,
			const_labels: [String; { $const_labels.len() }],
		}
		impl $struct_ident {
			pub fn new(
				metric: &'static $metric_ident,
				const_labels: [String; { $const_labels.len() }],
			) -> $struct_ident {
				$struct_ident { metric, const_labels }
			}

			pub fn inc(
				&mut self,
				non_const_labels: &[&str; { $labels.len() - $const_labels.len() }],
			) {
				let labels: [&str; { $labels.len() }] = self
					.const_labels
					.iter()
					.map(|s| s.as_str())
					.chain(*non_const_labels)
					.collect_array();
				self.metric.inc(&labels);
			}

			pub fn dec(
				&mut self,
				non_const_labels: &[&str; { $labels.len() - $const_labels.len() }],
			) {
				let labels: [&str; { $labels.len() }] = self
					.const_labels
					.iter()
					.map(|s| s.as_str())
					.chain(*non_const_labels)
					.collect_array();
				self.metric.dec(&labels);
			}

			pub fn set<T: TryInto<i64>>(
				&mut self,
				non_const_labels: &[&str; { $labels.len() - $const_labels.len() }],
				val: T,
			) where
				<T as TryInto<i64>>::Error: std::fmt::Debug,
			{
				let labels: [&str; { $labels.len() }] = self
					.const_labels
					.iter()
					.map(|s| s.as_str())
					.chain(*non_const_labels)
					.collect_array();
				self.metric.set(&labels, val);
			}
		}
	};
}

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
	};
	($metric_ident:ident, $struct_ident:ident, $name:literal, $help:literal, $labels:tt, $const_labels:tt) => {
		build_counter_vec!($metric_ident, $name, $help, $labels);

		#[derive(Clone)]
		pub struct $struct_ident {
			metric: &'static $metric_ident,
			const_labels: [String; { $const_labels.len() }],
		}
		impl $struct_ident {
			pub fn new(
				metric: &'static $metric_ident,
				const_labels: [String; { $const_labels.len() }],
			) -> $struct_ident {
				$struct_ident { metric, const_labels }
			}

			pub fn inc(
				&mut self,
				non_const_labels: &[&str; { $labels.len() - $const_labels.len() }],
			) {
				let labels: [&str; { $labels.len() }] = self
					.const_labels
					.iter()
					.map(|s| s.as_str())
					.chain(*non_const_labels)
					.collect_array();
				self.metric.inc(&labels);
			}
		}
	};
}

lazy_static::lazy_static! {
	static ref REGISTRY: Registry = Registry::new();

	pub static ref P2P_MSG_SENT: IntCounter = register_int_counter_with_registry!(Opts::new("cfe_p2p_msg_sent", "Count all the p2p msgs sent by the engine"), REGISTRY).expect("A duplicate metric collector has already been registered.");
	pub static ref P2P_MSG_RECEIVED: IntCounter = register_int_counter_with_registry!(Opts::new("cfe_p2p_msg_received", "Count all the p2p msgs received by the engine (raw before any processing)"), REGISTRY).expect("A duplicate metric collector has already been registered.");
	pub static ref P2P_RECONNECT_PEERS: IntGaugeWrapper = IntGaugeWrapper::new("cfe_p2p_reconnect_peers", "Count the number of peers we need to reconnect to", &REGISTRY);
	pub static ref P2P_ACTIVE_CONNECTIONS: IntGaugeWrapper = IntGaugeWrapper::new("cfe_p2p_active_connections", "Count the number of active connections", &REGISTRY);
	pub static ref P2P_ALLOWED_PUBKEYS: IntGaugeWrapper = IntGaugeWrapper::new("cfe_p2p_allowed_pubkeys", "Count the number of allowed pubkeys", &REGISTRY);
	pub static ref P2P_DECLINED_CONNECTIONS: IntCounter = register_int_counter_with_registry!(Opts::new("cfe_p2p_declined_connections", "Count the number times we decline a connection"), &REGISTRY).expect("A duplicate metric collector has already been registered.");
}

build_gauge_vec!(
	UNAUTHORIZED_CEREMONIES,
	"cfe_unauthorized_ceremonies",
	"Gauge keeping track of the number of unauthorized ceremony currently awaiting authorisation",
	["chain", "type"]
);
build_gauge_vec!(
	CHAIN_TRACKING,
	"cfe_chain_tracking",
	"Gauge keeping track of the latest block number the engine reported to the state chain",
	["chain"]
);
build_gauge_vec!(
	AUTHORIZED_CEREMONIES,
	"cfe_authorized_ceremonies",
	"Gauge keeping track of the number of ceremonies currently running",
	["chain", "type"]
);
build_counter_vec!(
	RPC_RETRIER_REQUESTS,
	"cfe_rpc_requests",
	"Count the rpc calls made by the engine, it doesn't keep into account the number of retrials",
	["client", "rpc_method"]
);
build_counter_vec!(
	RPC_RETRIER_TOTAL_REQUESTS,
	"cfe_rpc_requests_total",
	"Count all the rpc calls made by the retrier, it counts every single call even if it is the same made multiple times",
	["client","rpc_method"]
);
build_counter_vec!(
	P2P_MONITOR_EVENT,
	"cfe_p2p_monitor_event",
	"Count the number of events observed by the zmq connection monitor",
	["event_type"]
);
build_counter_vec!(
	P2P_BAD_MSG,
	"cfe_p2p_bad_msg",
	"Count all the bad p2p msgs received by the engine and labels them by the reason they got discarded",
	["reason"]
);
build_counter_vec_struct!(
	CEREMONY_PROCESSED_MSG,
	CeremonyProcessedMsg,
	"cfe_ceremony_msg",
	"Count all the processed messages for a given ceremony",
	["chain", "ceremony_type"]
);
build_counter_vec_struct!(
	CEREMONY_BAD_MSG,
	CeremonyBadMsg,
	"cfe_ceremony_bad_msg",
	"Count all the bad msgs processed during a ceremony",
	["chain", "reason"],
	["chain"] //const labels
);
build_histogram_vec_struct!(
	CEREMONY_DURATION,
	CeremonyDuration,
	"cfe_ceremony_duration",
	"Measure the duration of a ceremony in seconds",
	["chain", "ceremony_type"],
	(vec![2.0, 4.0, 8.0, 16.0, 32.0, 64.0, 128.0, 256.0, 512.0, 1024.0])
);
build_gauge_vec_struct!(
	CEREMONY_TIMEOUT_MISSING_MSG,
	CeremonyTimeoutMissingMsg,
	"cfe_ceremony_timeout_missing_msg",
	"Measure the number of missing messages when reaching timeout",
	["chain", "ceremony_type", "stage"],
	["chain", "ceremony_type"]
);
build_histogram_vec_struct!(
	STAGE_DURATION,
	StageDuration,
	"cfe_stage_duration",
	"Measure the duration of a stage in seconds",
	["chain", "stage", "phase"], //phase can be either receiving or processing
	["chain"],
	(vec![2.0, 3.0, 5.0, 8.0, 10.0, 15.0, 20.0, 25.0, 30.0])
);
build_counter_vec_struct!(
	STAGE_FAILING,
	StageFailing,
	"cfe_stage_failing",
	"Count the number of stages which are failing with the cause of the failure attached",
	["chain", "stage", "reason"],
	["chain"]
);
build_counter_vec_struct!(
	STAGE_COMPLETING,
	StageCompleting,
	"cfe_stage_completing",
	"Count the number of stages which are completing successfully",
	["chain", "stage"],
	["chain"]
);

/// structure containing the metrics used during a ceremony
#[derive(Clone)]
pub struct CeremonyMetrics {
	pub processed_messages: CeremonyProcessedMsg,
	pub bad_message: CeremonyBadMsg,
	pub ceremony_duration: CeremonyDuration,
	pub missing_messages: CeremonyTimeoutMissingMsg,
	pub stage_duration: StageDuration,
	pub stage_failing: StageFailing,
	pub stage_completing: StageCompleting,
}
impl CeremonyMetrics {
	pub fn new(chain_name: &str, ceremony_type: &str) -> Self {
		let chain_name = chain_name.to_string();
		let ceremony_type = ceremony_type.to_string();
		CeremonyMetrics {
			processed_messages: CeremonyProcessedMsg::new(
				&CEREMONY_PROCESSED_MSG,
				[chain_name.clone(), ceremony_type.clone()],
			),
			bad_message: CeremonyBadMsg::new(&CEREMONY_BAD_MSG, [chain_name.clone()]),
			ceremony_duration: CeremonyDuration::new(
				&CEREMONY_DURATION,
				[chain_name.clone(), ceremony_type.clone()],
			),
			missing_messages: CeremonyTimeoutMissingMsg::new(
				&CEREMONY_TIMEOUT_MISSING_MSG,
				[chain_name.clone(), ceremony_type],
			),
			stage_duration: StageDuration::new(&STAGE_DURATION, [chain_name.clone()]),
			stage_failing: StageFailing::new(&STAGE_FAILING, [chain_name.clone()]),
			stage_completing: StageCompleting::new(&STAGE_COMPLETING, [chain_name]),
		}
	}
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
	use super::*;
	use futures::FutureExt;

	#[tokio::test]
	async fn prometheus_test() {
		let prometheus_settings = Prometheus { hostname: "0.0.0.0".to_string(), port: 5567 };
		let metric = create_and_register_metric();

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
				request_test("metrics", reqwest::StatusCode::OK, "# HELP test test help\n# TYPE test counter\ntest{label=\"A\"} 1\ntest{label=\"B\"} 10\ntest{label=\"C\"} 100\n").await;

				REGISTRY.unregister(Box::new(metric)).unwrap();
				request_test("metrics", reqwest::StatusCode::OK, "").await;


				//test CeremonyMetrics correct deletion

				//we create the ceremony struct and put some metrics in it
				{
					let mut metrics = CeremonyMetrics::new("Chain1", "Keygen");
					metrics.bad_message.inc(&["AA"]);
					metrics.ceremony_duration.observe(Duration::new(999, 0));
					metrics.missing_messages.set(&["stage1",], 5);
					metrics.processed_messages.inc();
					metrics.processed_messages.inc();
					metrics.stage_completing.inc(&["stage1"]);
					metrics.stage_completing.inc(&["stage1"]);
					metrics.stage_completing.inc(&["stage2"]);
					metrics.stage_duration.observe(&["stage1", "receiving"], Duration::new(780, 0));
					metrics.stage_duration.observe(&["stage1", "processing"], Duration::new(78, 0));
					metrics.stage_failing.inc(&["stage3", "NotEnoughMessages"]);

					//This request does nothing, the ceremony is still ongoning so there is no deletion
					request_test("metrics", reqwest::StatusCode::OK, 
r#"# HELP cfe_ceremony_bad_msg Count all the bad msgs processed during a ceremony
# TYPE cfe_ceremony_bad_msg counter
cfe_ceremony_bad_msg{chain="Chain1",reason="AA"} 1
# HELP cfe_ceremony_duration Measure the duration of a ceremony in seconds
# TYPE cfe_ceremony_duration histogram
cfe_ceremony_duration_bucket{ceremony_type="Keygen",chain="Chain1",le="2"} 0
cfe_ceremony_duration_bucket{ceremony_type="Keygen",chain="Chain1",le="4"} 0
cfe_ceremony_duration_bucket{ceremony_type="Keygen",chain="Chain1",le="8"} 0
cfe_ceremony_duration_bucket{ceremony_type="Keygen",chain="Chain1",le="16"} 0
cfe_ceremony_duration_bucket{ceremony_type="Keygen",chain="Chain1",le="32"} 0
cfe_ceremony_duration_bucket{ceremony_type="Keygen",chain="Chain1",le="64"} 0
cfe_ceremony_duration_bucket{ceremony_type="Keygen",chain="Chain1",le="128"} 0
cfe_ceremony_duration_bucket{ceremony_type="Keygen",chain="Chain1",le="256"} 0
cfe_ceremony_duration_bucket{ceremony_type="Keygen",chain="Chain1",le="512"} 0
cfe_ceremony_duration_bucket{ceremony_type="Keygen",chain="Chain1",le="1024"} 1
cfe_ceremony_duration_bucket{ceremony_type="Keygen",chain="Chain1",le="+Inf"} 1
cfe_ceremony_duration_sum{ceremony_type="Keygen",chain="Chain1"} 999
cfe_ceremony_duration_count{ceremony_type="Keygen",chain="Chain1"} 1
# HELP cfe_ceremony_msg Count all the processed messages for a given ceremony
# TYPE cfe_ceremony_msg counter
cfe_ceremony_msg{ceremony_type="Keygen",chain="Chain1"} 2
# HELP cfe_ceremony_timeout_missing_msg Measure the number of missing messages when reaching timeout
# TYPE cfe_ceremony_timeout_missing_msg gauge
cfe_ceremony_timeout_missing_msg{ceremony_type="Keygen",chain="Chain1",stage="stage1"} 5
# HELP cfe_stage_completing Count the number of stages which are completing successfully
# TYPE cfe_stage_completing counter
cfe_stage_completing{chain="Chain1",stage="stage1"} 2
cfe_stage_completing{chain="Chain1",stage="stage2"} 1
# HELP cfe_stage_duration Measure the duration of a stage in seconds
# TYPE cfe_stage_duration histogram
cfe_stage_duration_bucket{chain="Chain1",phase="processing",stage="stage1",le="2"} 0
cfe_stage_duration_bucket{chain="Chain1",phase="processing",stage="stage1",le="3"} 0
cfe_stage_duration_bucket{chain="Chain1",phase="processing",stage="stage1",le="5"} 0
cfe_stage_duration_bucket{chain="Chain1",phase="processing",stage="stage1",le="8"} 0
cfe_stage_duration_bucket{chain="Chain1",phase="processing",stage="stage1",le="10"} 0
cfe_stage_duration_bucket{chain="Chain1",phase="processing",stage="stage1",le="15"} 0
cfe_stage_duration_bucket{chain="Chain1",phase="processing",stage="stage1",le="20"} 0
cfe_stage_duration_bucket{chain="Chain1",phase="processing",stage="stage1",le="25"} 0
cfe_stage_duration_bucket{chain="Chain1",phase="processing",stage="stage1",le="30"} 0
cfe_stage_duration_bucket{chain="Chain1",phase="processing",stage="stage1",le="+Inf"} 1
cfe_stage_duration_sum{chain="Chain1",phase="processing",stage="stage1"} 78
cfe_stage_duration_count{chain="Chain1",phase="processing",stage="stage1"} 1
cfe_stage_duration_bucket{chain="Chain1",phase="receiving",stage="stage1",le="2"} 0
cfe_stage_duration_bucket{chain="Chain1",phase="receiving",stage="stage1",le="3"} 0
cfe_stage_duration_bucket{chain="Chain1",phase="receiving",stage="stage1",le="5"} 0
cfe_stage_duration_bucket{chain="Chain1",phase="receiving",stage="stage1",le="8"} 0
cfe_stage_duration_bucket{chain="Chain1",phase="receiving",stage="stage1",le="10"} 0
cfe_stage_duration_bucket{chain="Chain1",phase="receiving",stage="stage1",le="15"} 0
cfe_stage_duration_bucket{chain="Chain1",phase="receiving",stage="stage1",le="20"} 0
cfe_stage_duration_bucket{chain="Chain1",phase="receiving",stage="stage1",le="25"} 0
cfe_stage_duration_bucket{chain="Chain1",phase="receiving",stage="stage1",le="30"} 0
cfe_stage_duration_bucket{chain="Chain1",phase="receiving",stage="stage1",le="+Inf"} 1
cfe_stage_duration_sum{chain="Chain1",phase="receiving",stage="stage1"} 780
cfe_stage_duration_count{chain="Chain1",phase="receiving",stage="stage1"} 1
# HELP cfe_stage_failing Count the number of stages which are failing with the cause of the failure attached
# TYPE cfe_stage_failing counter
cfe_stage_failing{chain="Chain1",reason="NotEnoughMessages",stage="stage3"} 1
"#).await;

					//End of ceremony
				}

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
