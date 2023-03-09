//! Health monitor for the CFE
//! allowing external services to query, ensuring it's online
//! Returns a HTTP 200 response to any request on {hostname}:{port}/health
//! Method returns a Sender, allowing graceful termination of the infinite loop

use anyhow::{Context, Result};
use futures::Future;
use tokio::{
	io::{AsyncReadExt, AsyncWriteExt},
	net::TcpListener,
};
use tracing::{error, info, info_span, warn, Instrument};

use crate::settings;

pub struct HealthChecker {
	listener: TcpListener,
}

// Returns the health checker run future so we can make sure TcpListener is active before
// proceeding in tests
impl HealthChecker {
	pub async fn start(
		health_check_settings: &settings::HealthCheck,
	) -> Result<impl Future<Output = Result<(), anyhow::Error>>> {
		let bind_address =
			format!("{}:{}", health_check_settings.hostname, health_check_settings.port);

		info!("Starting health-checker");

		let health_checker = Self {
			listener: TcpListener::bind(&bind_address)
				.await
				.with_context(|| format!("Could not bind TCP listener to {bind_address}"))?,
		};

		Ok(health_checker
			.run()
			.instrument(info_span!("health-check", bind_address = bind_address)))
	}

	async fn run(self) -> Result<()> {
		loop {
			match self.listener.accept().await {
				Ok((mut stream, _address)) => {
					let mut buffer = [0; 1024];
					stream.read(&mut buffer).await.context("Couldn't read stream into buffer")?;

					let mut headers = [httparse::EMPTY_HEADER; 16];
					let mut request = httparse::Request::new(&mut headers);
					match request.parse(&buffer) /* Iff returns Ok, fills request with the parsed request */ {
                        Ok(_) => {
                            if request.path.eq(&Some("/health")) {
                                let http_200_response = "HTTP/1.1 200 OK\r\n\r\n";
                                stream
                                    .write(http_200_response.as_bytes())
                                    .await
                                    .context("Could not write to health check stream")?;
                                stream
                                    .flush()
                                    .await
                                    .context("Could not flush health check TCP stream")?;
                            } else {
                                warn!("Requested health at invalid path: {:?}", request.path);
                            }
                        },
                        Err(e) => {
                            warn!("Invalid health check request, could not parse: {e}");
                        }
                    }
				},
				Err(e) => {
					error!("Could not open CFE health check TCP stream: {e}");
				},
			}
		}
	}
}

#[cfg(test)]
mod tests {

	use crate::settings::Settings;

	use super::*;

	#[tokio::test]
	async fn health_check_test() {
		let health_check = Settings::new_test().unwrap().health_check.unwrap();

		tokio::spawn(HealthChecker::start(&health_check).await.unwrap());

		let request_test = |path: &'static str, expected_status: Option<reqwest::StatusCode>| {
			let health_check = health_check.clone();
			async move {
				assert_eq!(
					expected_status,
					reqwest::get(&format!(
						"http://{}:{}/{}",
						&health_check.hostname, &health_check.port, path
					))
					.await
					.ok()
					.map(|x| x.status()),
				);
			}
		};

		request_test("health", Some(reqwest::StatusCode::from_u16(200).unwrap())).await;
		request_test("invalid", None).await;
	}
}
