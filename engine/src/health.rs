use async_std::net::TcpListener;
use futures::{AsyncReadExt, AsyncWriteExt, StreamExt};
use slog::o;
use tokio::{select, sync::oneshot::Sender};

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
        let listener = TcpListener::bind(self.bind_address.clone())
            .await
            .expect(format!("Could not bind TCP listener to {}", self.bind_address).as_str());

        let (tx, mut rx) = tokio::sync::oneshot::channel::<()>();
        let logger = self.logger.clone();
        tokio::spawn(async move {
            let mut incoming = listener.incoming();
            loop {
                let stream = select! {
                    Ok(()) = &mut rx => {
                        slog::info!(logger, "Shutting down health check gracefully");
                        break;
                    },
                    Some(stream) = incoming.next() => stream,
                };

                let mut stream = stream.expect("Could not open CFE health check TCP stream");
                let mut buffer = [0; 1024];
                // read the stream into the buffer
                stream
                    .read(&mut buffer)
                    .await
                    .expect("Couldn't read stream into buffer");

                // parse the http request
                let mut headers = [httparse::EMPTY_HEADER; 16];
                let mut req = httparse::Request::new(&mut headers);
                let result = req.parse(&buffer);
                if let Err(e) = result {
                    slog::warn!(
                        logger,
                        "Invalid health check request, could not parse: {}",
                        e,
                    );
                    continue;
                }

                if req.path.eq(&Some("/health")) {
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
                    slog::warn!(logger, "Requested health at invalid path: {:?}", req.path);
                }
            }
        });

        return tx;
    }
}

#[cfg(test)]
mod test {

    use std::time::Duration;

    use slog::{o, Drain};
    use tokio::time;

    use crate::logging;

    use super::*;

    // TODO: Make this a real test, perhaps by using reqwest to ping the health check endpoint
    #[tokio::test]
    #[ignore = "runs for 10 seconds"]
    async fn health_check_test() {
        let drain = slog_json::Json::new(std::io::stdout()).build().fuse();
        let drain = slog_async::Async::new(drain).build().fuse();
        let root = slog::Logger::root(drain, o!());
        let test_settings = settings::test_utils::new_test_settings().unwrap();
        let logger = logging::test_utils::create_test_logger();
        let health_monitor = HealthMonitor::new(&test_settings.health_check, &logger);
        let sender = health_monitor.run().await;
        time::sleep(Duration::from_millis(10000)).await;
        sender.send(()).unwrap();
    }
}
