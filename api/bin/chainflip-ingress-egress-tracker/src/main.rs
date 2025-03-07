// Copyright 2025 Chainflip Labs GmbH
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

use crate::store::RedisStore;
use cf_utilities::task_scope;
use chainflip_engine::settings::CfSettings;
use clap::Parser;
use futures::FutureExt;
use settings::{DepositTrackerSettings, TrackerOptions};

mod settings;
mod store;
mod utils;
mod witnessing;

async fn start(
	scope: &task_scope::Scope<'_, anyhow::Error>,
	settings: DepositTrackerSettings,
) -> anyhow::Result<()> {
	tracing_subscriber::FmtSubscriber::builder()
		.with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
		.try_init()
		.expect("setting default subscriber failed");

	let client = redis::Client::open(settings.redis_url.clone()).unwrap();
	let store = RedisStore::new(client.get_multiplexed_tokio_connection().await?);

	witnessing::start(scope, settings.clone(), store.clone()).await?;

	Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	chainflip_api::use_chainflip_account_id_encoding();
	let settings = DepositTrackerSettings::load_settings_from_all_sources(
		// Not using the config root or settings dir.
		"".to_string(),
		"",
		TrackerOptions::parse(),
	)?;

	task_scope::task_scope(|scope| async move { start(scope, settings).await }.boxed()).await
}
