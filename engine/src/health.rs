//! Health monitor for the CFE
//! allowing external services to query, ensuring it's online
//! Returns a HTTP 200 response to any request on {hostname}:{port}/health
//! Method returns a Sender, allowing graceful termination of the infinite loop

use std::{net::IpAddr, sync::Arc};

use tracing::info;
use utilities::task_scope;
use warp::Filter;

use crate::settings;

const INITIALISING: &'static str = "INITIALISING";
const RUNNING: &'static str = "RUNNING";

#[tracing::instrument(name = "health-check", skip_all)]
pub async fn start<'a, 'env>(
	scope: &'a task_scope::Scope<'env, anyhow::Error>,
	health_check_settings: &'a settings::HealthCheck,
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

	use crate::settings::Settings;

	use super::*;

	#[tokio::test]
	async fn health_check_test() {
		let health_check = Settings::new_test().unwrap().health_check.unwrap();

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
