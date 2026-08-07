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
use crate::StateChainBlock;
use frame_remote_externalities::RemoteExternalities;

pub type Ext = sp_state_machine::TestExternalities<sp_runtime::traits::BlakeTwo256>;

pub trait RuntimeTest: Default {
	fn setup() -> Self {
		Default::default()
	}

	fn run(self, block_hash: state_chain_runtime::Hash, ext: Ext) -> anyhow::Result<()>;
}

pub mod auction_resolution;
pub mod rotation_breakdown;
pub mod rotation_on_initialize;
pub mod storage_analysis;
pub mod swap_rate;
pub mod witnesser_cull;

/// Set to skip the runtime tests and only produce the storage report.
fn storage_analysis_only() -> bool {
	std::env::var("STORAGE_ANALYSIS_ONLY").is_ok_and(|v| v != "0")
}

pub fn run_all(mut ext: RemoteExternalities<StateChainBlock>) -> anyhow::Result<()> {
	let block_hash = ext.header.hash();
	let state_version = ext.state_version;

	// The flat key/value state. Note this is not the same as the raw snapshot below, which is
	// the trie node database (path prefix + node hash), and so cannot be attributed to storage
	// items directly.
	let pairs = ext.inner_ext.execute_with(|| {
		let mut pairs = Vec::new();
		let mut key = Vec::new();
		while let Some(next) = sp_io::storage::next_key(&key) {
			if let Some(value) = sp_io::storage::get(&next) {
				pairs.push((next.clone(), value.to_vec()));
			}
			key = next;
		}
		pairs
	});

	let (raw_storage, storage_root) = ext.inner_ext.into_raw_snapshot();

	storage_analysis::run(block_hash, &pairs, &raw_storage)?;

	if storage_analysis_only() {
		return Ok(())
	}

	log::info!("Running tests for block hash: {:?}", block_hash);

	let mk_ext = || {
		sp_state_machine::TestExternalities::from_raw_snapshot(
			raw_storage.clone(),
			storage_root,
			state_version,
		)
	};

	swap_rate::Test::setup().run(block_hash, mk_ext())?;
	auction_resolution::Test::setup().run(block_hash, mk_ext())?;
	rotation_on_initialize::Test::setup().run(block_hash, mk_ext())?;
	rotation_breakdown::PerPallet::setup().run(block_hash, mk_ext())?;
	rotation_breakdown::Full::setup().run(block_hash, mk_ext())?;
	witnesser_cull::Test::setup().run(block_hash, mk_ext())?;
	witnesser_cull::CullCost::setup().run(block_hash, mk_ext())?;

	log::info!("All tests passed for block hash: {:?}", block_hash);

	Ok(())
}
