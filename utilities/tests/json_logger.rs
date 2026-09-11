// Copyright 2026 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;
use utilities::logging::{init_json_logger, LoggingSettings};

fn plain_thread_has_subscriber() -> bool {
	std::thread::spawn(|| {
		!tracing::dispatcher::get_default(|d| d.is::<tracing::subscriber::NoSubscriber>())
	})
	.join()
	.unwrap()
}

fn free_port() -> u16 {
	std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
		.unwrap()
		.local_addr()
		.unwrap()
		.port()
}

/// The command server binds in a spawned task, so it may not be listening yet.
async fn wait_until_listening(client: &reqwest::Client, url: &str) {
	for _ in 0..100 {
		if client.get(url).send().await.is_ok() {
			return
		}
		tokio::time::sleep(Duration::from_millis(20)).await;
	}
	panic!("command server at {url} never started listening");
}

/// The engine runner can call the same engine's entrypoint twice in one process (before and after
/// a runtime upgrade), each time in a fresh tokio runtime. The second call must not try to install
/// another global subscriber, and its command server must control the one already installed.
#[test]
fn logger_survives_engine_reentry() {
	assert!(!plain_thread_has_subscriber());

	for filter in ["debug", "warn"] {
		tokio::runtime::Runtime::new().unwrap().block_on(async {
			let port = free_port();
			init_json_logger(LoggingSettings { command_server_port: port, ..Default::default() })
				.await;

			let client = reqwest::Client::new();
			let url = format!("http://127.0.0.1:{port}/tracing");
			wait_until_listening(&client, &url).await;

			let response = client
				.post(&url)
				.header("Content-Type", "application/json")
				.body(format!("\"{filter}\""))
				.send()
				.await
				.unwrap();
			assert!(response.status().is_success(), "{}", response.text().await.unwrap());
			assert!(client.get(&url).send().await.unwrap().text().await.unwrap().contains(filter));
		});

		assert!(plain_thread_has_subscriber());
	}
}
