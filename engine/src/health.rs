use slog::o;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    select,
    sync::oneshot::Sender,
};

use crate::{logging::COMPONENT_KEY, settings};

/// Health monitor for the CFE
/// allowing external services to query, ensuring it's online
/// Returns a HTTP 200 response to any request on {hostname}:{port}/health
/// Method returns a Sender, allowing graceful termination of the infinite loop
pub struct HealthMonitor {
    bind_address: String,
    logger: slog::Logger,
}

impl HealthMonitor {
    pub fn new(health_check_settings: &settings::HealthCheck, logger: &slog::Logger) -> Self {
        let bind_address = format!(
            "{}:{}",
            health_check_settings.hostname, health_check_settings.port
        );
        Self {
            logger: logger
                .new(o!(COMPONENT_KEY => "health-check", "bind-address" => bind_address.clone())),
            bind_address,
        }
    }

    pub async fn run(&self) -> Sender<()> {
        slog::info!(self.logger, "Starting");
        let listener = TcpListener::bind(self.bind_address.clone())
            .await
            .expect(format!("Could not bind TCP listener to {}", self.bind_address).as_str());

        let (shutdown_sender, mut shutdown_receiver) = tokio::sync::oneshot::channel::<()>();
        let logger = self.logger.clone();
        tokio::spawn(async move {
            loop {
                select! {
                    Ok(()) = &mut shutdown_receiver => {
                        slog::info!(logger, "Shutting down health check gracefully");
                        break;
                    },
                    result = listener.accept() => match result {
                        Ok((mut stream, _address)) => {
                            let mut buffer = [0; 1024];
                            stream
                                .read(&mut buffer)
                                .await
                                .expect("Couldn't read stream into buffer");

                            let mut headers = [httparse::EMPTY_HEADER; 16];
                            let mut request = httparse::Request::new(&mut headers);
                            match request.parse(&buffer) /* Iff returns Ok, fills request with the parsed request */ {
                                Ok(_) => {
                                    if request.path.eq(&Some("/health")) {
                                        let http_200_response = "HTTP/1.1 200 OK\r\n\r\n";
                                        stream
                                            .write(http_200_response.as_bytes())
                                            .await
                                            .expect("Could not write to health check stream");
                                        slog::trace!(logger, "Responded to health check: CFE is healthy :heart: ");
                                        stream
                                            .flush()
                                            .await
                                            .expect("Could not flush health check TCP stream");
                                    } else {
                                        slog::warn!(logger, "Requested health at invalid path: {:?}", request.path);
                                    }
                                },
                                Err(error) => {
                                    slog::warn!(
                                        logger,
                                        "Invalid health check request, could not parse: {}",
                                        error,
                                    );
                                }
                            }
                        },
                        Err(error) => {
                            slog::warn!(logger, "Could not open CFE health check TCP stream: {}", error);
                        }
                    },
                };
            }
        });

        return shutdown_sender;
    }
}

#[cfg(test)]
mod test {

    use crate::logging;
    use crate::testing::assert_ok;
    use tokio::process::Command;

    use super::*;

    #[tokio::test]
    async fn health_check_test() {
        let health_check = settings::test_utils::new_test_settings()
            .unwrap()
            .health_check;
        let logger = logging::test_utils::create_test_logger();
        let health_monitor = HealthMonitor::new(&health_check, &logger);
        let sender = health_monitor.run().await;

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

        sender.send(()).unwrap();
    }
}
