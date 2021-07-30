use async_std::net::TcpListener;
use futures::{AsyncReadExt, AsyncWriteExt, StreamExt};
use tokio::{select, sync::oneshot::Sender};

use crate::settings;

/// Health check function for the CFE
/// allowing external services to query, ensuring it's online
/// Returns a HTTP 200 response to any request on {hostname}:{port}/health
/// Method returns a Sender, allowing graceful termination of the infinite loop
pub async fn spawn_health_check(health_check_settings: settings::HealthCheck) -> Sender<()> {
    let bind_address = format!(
        "{}:{}",
        health_check_settings.hostname, health_check_settings.port
    );
    let listener = TcpListener::bind(bind_address.clone())
        .await
        .expect(format!("Could not bind TCP listener to {}", bind_address).as_str());

    let (tx, mut rx) = tokio::sync::oneshot::channel::<()>();

    tokio::spawn(async move {
        let mut incoming = listener.incoming();
        loop {
            let stream = select! {
                Ok(()) = &mut rx => {
                    log::info!("Shutting down health check gracefully");
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
                log::warn!("Invalid health check request, could not parse: {}", e);
                continue;
            }

            if req.path.eq(&Some("/health")) {
                let http_200_response = "HTTP/1.1 200 OK\r\n\r\n";
                stream
                    .write(http_200_response.as_bytes())
                    .await
                    .expect("Could not write to health check stream");
                log::trace!("Responded to health check: CFE is healthy :heart: ");
                stream
                    .flush()
                    .await
                    .expect("Could not flush health check TCP stream");
            } else {
                log::warn!("Requested health at invalid path: {:?}", req.path);
            }
        }
    });

    return tx;
}

#[cfg(test)]
mod test {

    use std::time::Duration;

    use tokio::time;

    use super::*;

    // TODO: Make this a real test, perhaps by using reqwest to ping the health check endpoint
    #[tokio::test]
    #[ignore = "runs for 10 seconds"]
    async fn health_check_test() {
        let test_settings = settings::test_utils::new_test_settings().unwrap();
        let sender = spawn_health_check(test_settings.health_check).await;
        time::sleep(Duration::from_millis(10000)).await;
        sender.send(()).unwrap();
    }
}
