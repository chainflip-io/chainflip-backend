//! Health monitor for the CFE and apis
//! allowing external services to query, ensuring it's online
//! Returns a HTTP 200 response to any request on {hostname}:{port}/health
//! Method returns a Sender, allowing graceful termination of the infinite loop

use std::{net::IpAddr, sync::Arc};

use crate::{task_scope, Port};
use clap::Args;
use serde::Deserialize;
use tracing::info;
use warp::Filter;

#[derive(Args, Debug, Clone, Default)]
pub struct HealthCheckOptions {
	#[clap(
		id = "HEALTH_CHECK_HOSTNAME",
		long = "health_check.hostname",
		help = "Hostname for this server's healthcheck. Requires the <HEALTH_CHECK_PORT> parameter to be given as well.",
		requires("HEALTH_CHECK_PORT")
	)]
	pub health_check_hostname: Option<String>,
	#[clap(
		id = "HEALTH_CHECK_PORT",
		long = "health_check.port",
		help = "Port for this server's healthcheck. Requires the <HEALTH_CHECK_HOSTNAME> parameter to be given as well.",
		requires("HEALTH_CHECK_HOSTNAME")
	)]
	pub health_check_port: Option<u16>,
}

#[derive(Debug, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct HealthCheck {
	pub hostname: String,
	pub port: Port,
}

const INITIALISING: &str = "INITIALISING";
const RUNNING: &str = "RUNNING";

pub async fn start_if_configured<'a, 'env>(
	scope: &'a task_scope::Scope<'env, anyhow::Error>,
	opts: &'a HealthCheckOptions,
	has_completed_initialising: Arc<std::sync::atomic::AtomicBool>,
) -> Result<(), anyhow::Error> {
	if opts.health_check_hostname.is_some() || opts.health_check_port.is_some() {
		let error_msg =
			"Clap enforces that both health_check.hostname and health_check.port are present.";
		start(
			scope,
			&HealthCheck {
				hostname: opts.health_check_hostname.clone().expect(error_msg),
				port: opts.health_check_port.expect(error_msg),
			},
			has_completed_initialising,
		)
		.await
	} else {
		Ok(())
	}
}

#[tracing::instrument(name = "health-check", skip_all)]
pub async fn start<'a, 'env>(
	scope: &'a task_scope::Scope<'env, anyhow::Error>,
	health_check_settings: &'a HealthCheck,
	has_completed_initialising: Arc<std::sync::atomic::AtomicBool>,
) -> Result<(), anyhow::Error> {
	info!("Starting");

	const PATH: &str = "health";

	let future =
		warp::serve(warp::any().and(warp::path(PATH)).and(warp::path::end()).map(move || {
			warp::reply::with_status(
				if has_completed_initialising.load(std::sync::atomic::Ordering::Relaxed) {
					RUNNING
				} else {
					INITIALISING
				},
				warp::http::StatusCode::OK,
			)
		}))
		.bind((health_check_settings.hostname.parse::<IpAddr>()?, health_check_settings.port));

	scope.spawn_weak(async move {
		future.await;
		Ok(())
	});

	Ok(())
}

#[cfg(test)]
mod tests {

	use futures_util::FutureExt;

	use super::*;

	#[tokio::test]
	async fn health_check_test() {
		let health_check = HealthCheck { hostname: "127.0.0.1".to_string(), port: 5555 };

		task_scope::task_scope(|scope| {
			async {
				let has_completed_initialising =
					Arc::new(std::sync::atomic::AtomicBool::new(false));
				start(scope, &health_check, has_completed_initialising.clone()).await.unwrap();

				let request_test = |path: &'static str,
				                    expected_status: reqwest::StatusCode,
				                    expected_text: &'static str| {
					let health_check = health_check.clone();

					async move {
						let resp = reqwest::get(&format!(
							"http://{}:{}/{}",
							&health_check.hostname, &health_check.port, path
						))
						.await
						.unwrap();

						assert_eq!(expected_status, resp.status());
						assert_eq!(resp.text().await.unwrap(), expected_text);
					}
				};

				// starts with `has_completed_initialising` set to false
				request_test("health", reqwest::StatusCode::OK, INITIALISING).await;
				request_test("invalid", reqwest::StatusCode::NOT_FOUND, "").await;

				has_completed_initialising.store(true, std::sync::atomic::Ordering::Relaxed);

				request_test("health", reqwest::StatusCode::OK, RUNNING).await;

				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap();
	}
}
