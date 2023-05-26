//! Health monitor for the CFE
//! allowing external services to query, ensuring it's online
//! Returns a HTTP 200 response to any request on {hostname}:{port}/health
//! Method returns a Sender, allowing graceful termination of the infinite loop

use std::net::IpAddr;

use tracing::info;
use utilities::task_scope;
use warp::Filter;

use crate::settings;

#[tracing::instrument(name = "health-check", skip_all)]
pub async fn start<'a, 'env>(
	scope: &'a task_scope::Scope<'env, anyhow::Error>,
	health_check_settings: &'a settings::HealthCheck,
) -> Result<(), anyhow::Error> {
	info!("Starting");

	const PATH: &str = "health";

	let future =
		warp::serve(warp::any().and(warp::path(PATH)).and(warp::path::end()).map(warp::reply))
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
				start(scope, &health_check).await.unwrap();

				let request_test = |path: &'static str, expected_status: reqwest::StatusCode| {
					let health_check = health_check.clone();
					async move {
						assert_eq!(
							expected_status,
							reqwest::get(&format!(
								"http://{}:{}/{}",
								&health_check.hostname, &health_check.port, path
							))
							.await
							.unwrap()
							.status(),
						);
					}
				};

				request_test("health", reqwest::StatusCode::OK).await;
				request_test("invalid", reqwest::StatusCode::NOT_FOUND).await;

				Ok(())
			}
			.boxed()
		})
		.await
		.unwrap();
	}
}
