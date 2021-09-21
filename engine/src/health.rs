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

        let (tx, mut rx) = tokio::sync::oneshot::channel::<()>();
        let logger = self.logger.clone();
        tokio::spawn(async move {
            loop {
                select! {
                    Ok(()) = &mut rx => {
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

        return tx;
    }
}

#[cfg(test)]
mod test {

    use std::time::Duration;

    use tokio::time;

    use crate::logging;

    use super::*;

    // TODO: Make this a real test, perhaps by using reqwest to ping the health check endpoint
    #[tokio::test]
    #[ignore = "runs for 10 seconds"]
    async fn health_check_test() {
        let test_settings = settings::test_utils::new_test_settings().unwrap();
        let logger = logging::test_utils::create_test_logger();
        let health_monitor = HealthMonitor::new(&test_settings.health_check, &logger);
        let sender = health_monitor.run().await;
        time::sleep(Duration::from_millis(10000)).await;
        sender.send(()).unwrap();
    }
}
