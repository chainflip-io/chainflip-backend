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

#[derive(Debug, clap::Parser)]
pub struct Cli {
	#[command(subcommand)]
	pub subcommand: Option<Subcommand>,

	#[clap(flatten)]
	pub run: sc_cli::RunCmd,

	/// UNSAFE: Warp sync to this block instead of to the chain tip.
	///
	/// Requires `--sync warp`, `--warp-sync-target-rpc` and an empty database. The target header
	/// is fetched from `--warp-sync-target-rpc` and trusted unconditionally: only use this with a
	/// node you control. Intended for building a database that holds the state at a specific
	/// block, for example to run `benchmark block` against it.
	#[arg(long, value_name = "BLOCK_NUMBER", requires = "warp_sync_target_rpc")]
	pub unsafe_warp_sync_target_block: Option<cf_primitives::BlockNumber>,

	/// RPC endpoint of a *trusted* node used to resolve `--unsafe-warp-sync-target-block`.
	///
	/// The node must be synced past the target block, and must be an archive node if the target
	/// block is older than the endpoint's pruning window.
	#[arg(long, value_name = "URL")]
	pub warp_sync_target_rpc: Option<String>,
}

#[derive(Debug, clap::Subcommand)]
#[expect(clippy::large_enum_variant)]
pub enum Subcommand {
	/// Key management cli utilities
	#[command(subcommand)]
	Key(sc_cli::KeySubcommand),

	/// Build a chain specification.
	/// DEPRECATED: Use `export-chain-spec` command instead.
	#[deprecated(
		note = "build-spec command will be removed after 1/04/2026. Use export-chain-spec command instead"
	)]
	BuildSpec(sc_cli::BuildSpecCmd),

	/// Export the chain specification.
	ExportChainSpec(sc_cli::ExportChainSpecCmd),

	/// Validate blocks.
	CheckBlock(sc_cli::CheckBlockCmd),

	/// Export blocks.
	ExportBlocks(sc_cli::ExportBlocksCmd),

	/// Export the state of a given block into a chain spec.
	ExportState(sc_cli::ExportStateCmd),

	/// Import blocks.
	ImportBlocks(sc_cli::ImportBlocksCmd),

	/// Remove the whole chain.
	PurgeChain(sc_cli::PurgeChainCmd),

	/// Revert the chain to a previous state.
	Revert(sc_cli::RevertCmd),

	/// Sub-commands concerned with benchmarking.
	#[command(subcommand)]
	Benchmark(frame_benchmarking_cli::BenchmarkCmd),

	/// Db meta columns information.
	ChainInfo(sc_cli::ChainInfoCmd),
}

#[cfg(test)]
mod tests {
	use super::*;
	use clap::Parser;

	#[test]
	fn warp_sync_target_flags() {
		// The target block is meaningless without somewhere to resolve it from.
		assert!(Cli::try_parse_from(["chainflip-node", "--unsafe-warp-sync-target-block", "123"])
			.is_err());

		let cli = Cli::try_parse_from([
			"chainflip-node",
			"--unsafe-warp-sync-target-block",
			"123",
			"--warp-sync-target-rpc",
			"ws://localhost:9944",
		])
		.unwrap();
		assert_eq!(cli.unsafe_warp_sync_target_block, Some(123));
		assert_eq!(cli.warp_sync_target_rpc.as_deref(), Some("ws://localhost:9944"));

		// Both flags are optional.
		let cli = Cli::try_parse_from(["chainflip-node"]).unwrap();
		assert_eq!(cli.unsafe_warp_sync_target_block, None);
		assert_eq!(cli.warp_sync_target_rpc, None);
	}
}
