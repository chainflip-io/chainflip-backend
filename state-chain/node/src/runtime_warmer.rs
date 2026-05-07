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

//! Runtime warmer: pre-compiles pending runtime upgrades during the governance
//! approval window so the actual `set_code` doesn't stall block production
//! while wasmtime compiles the new ~5 MB compressed (~30 MB uncompressed) blob.
//!
//! Watches `Governance::Proposals` on every imported best block, decodes any
//! `governance.chainflip_runtime_upgrade { code, .. }` calls it finds, and
//! drives the executor to compile the WASM via `with_instance`. That populates
//! both the executor's in-memory `RuntimeCache` and the on-disk wasmtime
//! artifact cache under the same `(code_hash, heap_alloc_strategy, wasm_method)`
//! key block production looks up later — so when `set_code` finally runs, the
//! lookup is a hashmap hit and there is no compilation work to do.
//!
//! The `code_hash` we set on `RuntimeCode` matches what `BackendRuntimeCode`
//! produces from `storage_hash(:code:)`: blake2-256 of the compressed bytes
//! that get stored under the `:code:` key by `set_code`.

use codec::Decode;
use futures::StreamExt;
use sc_client_api::{BlockchainEvents, StorageProvider};
use sc_executor::{WasmExecutor, DEFAULT_HEAP_ALLOC_STRATEGY};
use sc_service::SpawnTaskHandle;
use sp_blockchain::HeaderBackend;
use sp_core::{
	storage::StorageKey,
	traits::{RuntimeCode, WrappedRuntimeCode},
	twox_128, Blake2Hasher, Hasher,
};
use sp_state_machine::BasicExternalities;
use state_chain_runtime::{opaque::Block, Runtime, RuntimeCall};
use std::{
	borrow::Cow,
	collections::HashSet,
	sync::{Arc, Mutex},
};

const LOG_TARGET: &str = "runtime-warmer";
const TASK_NAME: &str = "runtime-warmer";
const TASK_GROUP: Option<&str> = Some("chainflip");

/// Spawn the warmer task. Idempotent when run on multiple imports — already-warmed
/// code hashes are skipped.
pub fn spawn<C, B>(
	client: Arc<C>,
	executor: WasmExecutor<sp_io::SubstrateHostFunctions>,
	spawn_handle: SpawnTaskHandle,
) where
	C: BlockchainEvents<Block>
		+ StorageProvider<Block, B>
		+ HeaderBackend<Block>
		+ Send
		+ Sync
		+ 'static,
	B: sc_client_api::Backend<Block> + Send + Sync + 'static,
{
	spawn_handle.spawn(TASK_NAME, TASK_GROUP, run(client, executor));
}

async fn run<C, B>(client: Arc<C>, executor: WasmExecutor<sp_io::SubstrateHostFunctions>)
where
	C: BlockchainEvents<Block>
		+ StorageProvider<Block, B>
		+ HeaderBackend<Block>
		+ Send
		+ Sync
		+ 'static,
	B: sc_client_api::Backend<Block> + Send + Sync + 'static,
{
	let prefix = governance_proposals_prefix();
	let mut warmed: HashSet<[u8; 32]> = HashSet::new();
	// Serialise wasmtime compiles to one at a time. A burst of proposals would
	// otherwise spawn parallel cranelift jobs and starve block production.
	let compile_lock: Arc<Mutex<()>> = Arc::new(Mutex::new(()));
	let mut import_stream = client.import_notification_stream();

	log::debug!(target: LOG_TARGET, "Runtime warmer started.");

	while let Some(notification) = import_stream.next().await {
		// Forks revert sometimes; processing non-best blocks would just waste compute.
		if !notification.is_new_best {
			continue;
		}

		let pairs = match client.storage_pairs(notification.hash, Some(&prefix), None) {
			Ok(it) => it,
			Err(e) => {
				log::debug!(
					target: LOG_TARGET,
					"storage_pairs failed at {:?}: {:?}",
					notification.hash, e,
				);
				continue;
			},
		};

		for (_storage_key, value) in pairs {
			// `Proposal { call: Vec<u8>, .. }` — we only need the first field, so decoding a
			// `Vec<u8>` from the leading bytes is sufficient and avoids a dependency on
			// `pallet_cf_governance::Proposal`'s exact layout.
			let mut bytes = value.0.as_slice();
			let opaque_call = match Vec::<u8>::decode(&mut bytes) {
				Ok(c) => c,
				Err(_) => {
					log::warn!(
						target: LOG_TARGET,
						"Failed to decode `Governance::Proposals` entry as opaque call bytes. \
						 The storage layout has likely changed — the runtime warmer needs updating. \
						 Pending runtime upgrades will not be pre-compiled until this is fixed.",
					);
					break;
				},
			};

			let Some(code) = extract_runtime_upgrade_code(&opaque_call) else { continue };

			// `BackendRuntimeCode` keys the runtime cache by `storage_hash(:code:)`,
			// which for a Blake2-256 trie is `blake2_256(stored_value)`. The stored
			// value is the *compressed* WASM, so we hash the compressed bytes here
			// — that makes our cache entry the exact one block production looks up.
			let code_hash = Blake2Hasher::hash(&code).0;
			if !warmed.insert(code_hash) {
				continue;
			}

			let executor = executor.clone();
			let lock = compile_lock.clone();
			let hash_hex = hex::encode(code_hash);
			log::info!(
				target: LOG_TARGET,
				"Detected pending runtime upgrade (hash=0x{}, compressed_size={} bytes); \
				 warming runtime cache.",
				hash_hex, code.len(),
			);

			// Wasmtime AOT compilation is CPU-heavy and would block the async runtime;
			// hand it off to the blocking pool. The mutex inside `warm` serialises
			// compiles so a burst of proposals can't starve block production.
			tokio::task::spawn_blocking(move || warm(&executor, &lock, code, code_hash, &hash_hex));
		}
	}

	log::debug!(target: LOG_TARGET, "Runtime warmer stopped.");
}

/// Compile `code` via `WasmExecutor::with_instance`, populating both the
/// executor's in-memory `RuntimeCache` and the on-disk wasmtime artifact cache
/// under the key block production will look up at `set_code` time.
///
/// `compile_lock` is held for the duration of the compile, serialising concurrent
/// warm-ups so that a burst of proposals doesn't fan out into parallel cranelift
/// jobs.
///
/// The `RuntimeCode.hash` is `blake2_256(compressed_code)` — exactly what
/// `BackendRuntimeCode` produces from `storage_hash(:code:)` after the upgrade
/// lands. `heap_pages` is `None` so the executor falls back to its
/// `default_onchain_heap_alloc_strategy`, matching the path block production
/// takes when there is no `:heap_pages:` storage value (the chainflip runtime
/// doesn't set one).
fn warm(
	executor: &WasmExecutor<sp_io::SubstrateHostFunctions>,
	compile_lock: &Mutex<()>,
	code: Vec<u8>,
	code_hash: [u8; 32],
	hash_hex: &str,
) {
	// Mutex is poisoned only if a previous warmer panicked mid-compile. That would
	// be a wasmtime bug, not a logic bug here — recover and proceed.
	let _guard = compile_lock.lock().unwrap_or_else(|poison| poison.into_inner());
	let started = std::time::Instant::now();

	let wrapped = WrappedRuntimeCode(Cow::Borrowed(code.as_slice()));
	let runtime_code = RuntimeCode {
		code_fetcher: &wrapped,
		// 32-byte blake2-256, encoded just as a fixed-size byte array — matches
		// what `BackendRuntimeCode::runtime_code()` puts here for the active runtime.
		hash: code_hash.to_vec(),
		heap_pages: None,
	};

	let mut ext = BasicExternalities::default();
	let result: sc_executor_common::error::Result<()> = executor.with_instance(
		&runtime_code,
		&mut ext,
		DEFAULT_HEAP_ALLOC_STRATEGY,
		// The compile + cache insert happens before `f` is called. We don't need
		// to do anything with the instance — its existence in the cache is the
		// whole point.
		|_module, _instance, _version, _ext| Ok(Ok(())),
	);

	match result {
		Ok(()) => log::info!(
			target: LOG_TARGET,
			"Runtime warm-up complete (hash=0x{}, took={}ms).",
			hash_hex,
			started.elapsed().as_millis(),
		),
		Err(e) => log::warn!(
			target: LOG_TARGET,
			"Runtime warm-up failed (hash=0x{}, took={}ms): {:?}",
			hash_hex,
			started.elapsed().as_millis(),
			e,
		),
	}
}

/// Decode `opaque` as a `RuntimeCall` and return the inner WASM if it is a
/// `governance.chainflip_runtime_upgrade` call.
///
/// Returns `None` for any other governance call or for an unrecognised pallet.
/// Returns a warning-logging `None` if the bytes don't decode as a `RuntimeCall`
/// at all — that almost certainly means the runtime's storage layout for
/// `Governance::Proposals` has changed and this function needs revisiting.
fn extract_runtime_upgrade_code(opaque: &[u8]) -> Option<Vec<u8>> {
	let call = match RuntimeCall::decode(&mut &opaque[..]) {
		Ok(call) => call,
		Err(e) => {
			log::warn!(
				target: LOG_TARGET,
				"Failed to decode `Governance::Proposals` entry as `RuntimeCall` ({:?}). \
				 The storage layout has likely changed — the runtime warmer needs updating. \
				 Pending runtime upgrades will not be pre-compiled until this is fixed.",
				e,
			);
			return None;
		},
	};
	match call {
		RuntimeCall::Governance(
			pallet_cf_governance::Call::<Runtime>::chainflip_runtime_upgrade { code, .. },
		) => Some(code),
		_ => None,
	}
}

/// Storage prefix for the `Proposals` map in the `Governance` pallet:
/// `Twox128("Governance") || Twox128("Proposals")`.
fn governance_proposals_prefix() -> StorageKey {
	let mut prefix = [0u8; 32];
	prefix[..16].copy_from_slice(&twox_128(b"Governance"));
	prefix[16..].copy_from_slice(&twox_128(b"Proposals"));
	StorageKey(prefix.to_vec())
}

#[cfg(test)]
mod tests {
	use super::*;
	use codec::Encode;

	#[test]
	fn extracts_code_from_runtime_upgrade_call() {
		let wasm = vec![0xDE, 0xAD, 0xBE, 0xEF, 0x01, 0x02, 0x03];
		let call = RuntimeCall::Governance(
			pallet_cf_governance::Call::<Runtime>::chainflip_runtime_upgrade {
				cfe_version_restriction: None,
				code: wasm.clone(),
			},
		);
		let encoded = call.encode();

		assert_eq!(extract_runtime_upgrade_code(&encoded), Some(wasm));
	}

	#[test]
	fn ignores_other_calls() {
		// A non-runtime-upgrade governance call: `approve(0)`.
		let call = RuntimeCall::Governance(pallet_cf_governance::Call::<Runtime>::approve {
			approved_id: 0,
		});
		let encoded = call.encode();

		assert_eq!(extract_runtime_upgrade_code(&encoded), None);
	}

	#[test]
	fn ignores_garbage() {
		assert_eq!(extract_runtime_upgrade_code(&[]), None);
		assert_eq!(extract_runtime_upgrade_code(&[0xFF; 32]), None);
	}

	#[test]
	fn proposals_prefix_matches_known_format() {
		let prefix = governance_proposals_prefix();
		assert_eq!(prefix.0.len(), 32);
		assert_eq!(&prefix.0[..16], &twox_128(b"Governance"));
		assert_eq!(&prefix.0[16..], &twox_128(b"Proposals"));
	}
}
