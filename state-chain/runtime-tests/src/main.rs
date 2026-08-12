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
use std::{
	path::{Path, PathBuf},
	str::FromStr,
};

use anyhow::anyhow;
use frame_remote_externalities::{Mode, OfflineConfig, OnlineConfig, SnapshotConfig, Transport};
use tracing_subscriber::filter::LevelFilter;

mod tests;

type StateChainBlock = state_chain_runtime::Block;

fn help() -> String {
	// Resolved rather than created: printing the help shouldn't leave a directory behind.
	let snapshot_dir = snapshot_dir().map_or_else(
		|_| format!("<none: no cache directory found, set {SNAPSHOT_DIR_VAR}>"),
		|dir| dir.display().to_string(),
	);

	format!(
		"\
Runs the state chain runtime tests against real chain state, either fetched from a network
or loaded from a local snapshot.

USAGE:
    cargo run -- [NETWORK] <TARGET>...
    cargo run -- help

    This crate is excluded from the workspace, so cargo must be run from within
    `state-chain/runtime-tests`.

NETWORK (optional):
    local                       http://localhost:9944
    sisyphos | s                https://archive.sisyphos.chainflip.io:443
    perseverance | p            https://archive.perseverance.chainflip.io:443
    berghain | mainnet | b | m  https://mainnet-archive.chainflip.io:443
    <url>                       any other archive node RPC endpoint, with an explicit
                                scheme (http://, https://, ws:// or wss://)

    The leading argument is taken as the network only if it matches one of the above.
    With no network, state is loaded from local snapshots and nothing is fetched.

TARGET (one or more):
    latest                      the network's latest finalised block (needs a NETWORK)
    0x<hash>                    a specific block, from the snapshot cache or the network
    <path>.snap                 a snapshot file at an explicit path, always read offline

    Fetched state is cached by block hash, so a block that has been run before is re-used
    offline instead of being fetched again. The cache lives outside the repository, so it
    survives `git clean` and is shared between checkouts:

        {snapshot_dir}

ENVIRONMENT:
    CF_RUNTIME_TESTS_SNAPSHOT_DIR   where to cache snapshots (see above)
    STORAGE_ANALYSIS_ONLY=1     run the storage analysis only, skipping the tests
    RUST_LOG=debug              log level (default: info)

EXAMPLES:
    cargo run -- berghain latest             # fetch and test mainnet's latest block
    cargo run -- b 0xabc123... 0xdef456...   # fetch and test two specific blocks
    cargo run -- 0xabc123...                 # re-run against an existing snapshot

A storage report is written to `state-chain/runtime-tests/storage-report-<hash>.md` for
every block that is processed.\
"
	)
}

#[derive(Debug, Clone)]
pub enum Network {
	Local,
	Sisyphos,
	Perseverance,
	Berghain,
	Custom(String),
}

impl Network {
	fn url(self) -> String {
		match self {
			Self::Local => "http://localhost:9944".to_string(),
			Self::Sisyphos => "https://archive.sisyphos.chainflip.io:443".to_string(),
			Self::Perseverance => "https://archive.perseverance.chainflip.io:443".to_string(),
			Self::Berghain => "https://mainnet-archive.chainflip.io:443".to_string(),
			Self::Custom(url) => url,
		}
	}
}

impl FromStr for Network {
	/// `Err` means "not a network", not "malformed": the argument is then tried as a target.
	type Err = ();

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		const SCHEMES: [&str; 4] = ["http://", "https://", "ws://", "wss://"];

		match s {
			"local" => Ok(Self::Local),
			"sisyphos" | "s" => Ok(Self::Sisyphos),
			"perseverance" | "p" => Ok(Self::Perseverance),
			"berghain" | "mainnet" | "b" | "m" => Ok(Self::Berghain),
			// A custom endpoint must carry an explicit scheme. Accepting bare strings would
			// swallow any mistyped target as a URL, and fail much later at connection time.
			s if SCHEMES.iter().any(|scheme| s.starts_with(scheme)) =>
				Ok(Self::Custom(s.to_string())),
			_ => Err(()),
		}
	}
}

/// A block to run the tests against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
	/// The network's latest finalised block.
	Latest,
	/// A specific block, cached in [`snapshot_dir`] under its hash.
	Block(state_chain_runtime::Hash),
	/// A snapshot file at an explicit path, read as given.
	Snapshot(PathBuf),
}

impl FromStr for Target {
	type Err = anyhow::Error;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		if s == "latest" {
			return Ok(Self::Latest)
		}

		// A bare hash is looked up in (and cached to) the snapshot directory; anything that
		// names a file is used verbatim, so that snapshots kept elsewhere can be replayed.
		if let Ok(hash) = s.parse() {
			return Ok(Self::Block(hash))
		}

		if s.ends_with(".snap") {
			return Ok(Self::Snapshot(PathBuf::from(s)))
		}

		Err(anyhow!(
			"Invalid argument `{s}`: expected a network name or url, `latest`, a block hash, or \
			 a path to a `.snap` file. Run with `help` for details."
		))
	}
}

/// Overrides where snapshots are cached.
const SNAPSHOT_DIR_VAR: &str = "CF_RUNTIME_TESTS_SNAPSHOT_DIR";

/// Where fetched chain state is cached.
///
/// Deliberately outside the repository: snapshots are large, are keyed by an immutable block
/// hash, and so are worth sharing between checkouts rather than losing to a `git clean`.
fn snapshot_dir() -> anyhow::Result<PathBuf> {
	Ok(match std::env::var_os(SNAPSHOT_DIR_VAR) {
		Some(dir) => PathBuf::from(dir),
		None => dirs::cache_dir()
			.ok_or_else(|| {
				anyhow!("Unable to locate a cache directory. Set {SNAPSHOT_DIR_VAR} to choose one.")
			})?
			.join("chainflip")
			.join("runtime-tests"),
	})
}

/// [`snapshot_dir`], created if it doesn't exist yet.
fn create_snapshot_dir() -> anyhow::Result<PathBuf> {
	let dir = snapshot_dir()?;

	std::fs::create_dir_all(&dir)
		.map_err(|e| anyhow!("Unable to create snapshot directory {}: {e}", dir.display()))?;

	Ok(dir)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	let args: Vec<String> = std::env::args().skip(1).collect();

	if args.iter().any(|arg| matches!(arg.as_str(), "help" | "--help" | "-h")) {
		println!("{}", help());
		return Ok(())
	}

	let Some((first, rest)) = args.split_first() else {
		eprintln!("{}", help());
		anyhow::bail!("No arguments provided.");
	};

	tracing_subscriber::FmtSubscriber::builder()
		.with_env_filter(
			tracing_subscriber::EnvFilter::builder()
				.with_default_directive(LevelFilter::INFO.into())
				.from_env()?,
		)
		.try_init()
		.expect("setting default subscriber failed");

	// The leading argument is the network if it names one; otherwise every argument is a
	// target and the run is offline.
	let (network, target_args) = match first.parse::<Network>() {
		Ok(network) => (Some(network), rest),
		Err(()) => (None, args.as_slice()),
	};

	let targets = target_args
		.iter()
		.map(|arg| arg.parse::<Target>())
		.collect::<anyhow::Result<Vec<_>>>()?;

	if targets.is_empty() {
		eprintln!("{}", help());
		anyhow::bail!("No targets provided. Provide one or more snapshots, hashes or `latest`.");
	}

	// Resolving `latest` requires querying the network, and the snapshot it would otherwise be
	// read from is renamed to its block hash as soon as it has been fetched.
	if network.is_none() && targets.contains(&Target::Latest) {
		anyhow::bail!("`latest` requires a network, e.g. `berghain latest`.");
	}

	for target in &targets {
		if let Target::Snapshot(path) = target {
			if !path.is_file() {
				anyhow::bail!("Snapshot not found: {}", path.display());
			}
		}
	}

	let network = network.map(Network::url);
	let snapshot_dir = create_snapshot_dir()?;
	log::info!("Caching snapshots in {}", snapshot_dir.display());

	let modes: Vec<_> = targets
		.into_iter()
		.map(|target| {
			// An explicitly-pathed snapshot is read as given: there is no hash to fetch it by.
			let hash = match target {
				Target::Block(hash) => Some(hash),
				Target::Latest => None,
				Target::Snapshot(path) =>
					return Mode::Offline(OfflineConfig {
						state_snapshot: SnapshotConfig::new(path),
					}),
			};

			let state_snapshot = snapshot_file_for_hash(&snapshot_dir, hash);

			match &network {
				None => Mode::Offline(OfflineConfig { state_snapshot }),
				Some(network) => Mode::OfflineOrElseOnline(
					OfflineConfig { state_snapshot: state_snapshot.clone() },
					OnlineConfig {
						at: hash,
						state_snapshot: Some(state_snapshot),
						transport: Transport::Uri(network.clone()),
						..Default::default()
					},
				),
			}
		})
		.collect();

	for mode in modes {
		let snapshot_config = match &mode {
			Mode::Offline(OfflineConfig { ref state_snapshot }) => Some(state_snapshot),
			Mode::OfflineOrElseOnline(OfflineConfig { ref state_snapshot, .. }, _) =>
				Some(state_snapshot),
			_ => None,
		}
		.cloned();

		let remote_externalities = frame_remote_externalities::Builder::<StateChainBlock>::new()
			.mode(mode)
			.build()
			.await
			.map_err(|e| anyhow!(e))?;

		// If the snapshot was for "latest", rename it to the actual hash.
		if let Some(snapshot) = snapshot_config {
			if snapshot.path == snapshot_file_for_hash(&snapshot_dir, None).path {
				std::fs::rename(
					snapshot.path,
					snapshot_file_for_hash(&snapshot_dir, Some(remote_externalities.header.hash()))
						.path,
				)?;
			}
		}

		tests::run_all(remote_externalities)?;
	}

	Ok(())
}

fn snapshot_file_for_hash(dir: &Path, hash: Option<state_chain_runtime::Hash>) -> SnapshotConfig {
	SnapshotConfig::new(dir.join(match hash {
		Some(hash) => format!("{hash:?}.snap"),
		None => "latest.snap".to_string(),
	}))
}
