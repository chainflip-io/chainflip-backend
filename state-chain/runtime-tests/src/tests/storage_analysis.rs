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

//! Storage footprint analysis of a live chain snapshot.
//!
//! Buckets every raw trie key by its 32-byte storage prefix, maps those prefixes back to
//! `(pallet, storage item)` pairs via the runtime metadata, and reports:
//! - which items dominate the state size (hotspots),
//! - prefixes with no corresponding metadata entry (dead storage / leaked prefixes),
//! - key-space statistics for the largest maps (to spot unbounded growth).

use std::collections::BTreeMap;

use codec::Decode;
use frame_support::{
	StorageHasher as _, Twox128,
	__private::metadata::{
		v14::{StorageEntryType, StorageHasher},
		RuntimeMetadata,
	},
};

/// Flat state: storage key -> storage value.
type Pairs = [(Vec<u8>, Vec<u8>)];
/// Trie node database: (path prefix ++ node hash) -> (encoded node, refcount).
type TrieNodes = [(Vec<u8>, (Vec<u8>, i32))];

/// How many of the biggest items get a detailed key-space breakdown.
const DETAILED_ITEMS: usize = 40;
/// How many sample keys to record per orphaned prefix.
const ORPHAN_SAMPLES: usize = 3;
/// Source trees swept for candidate storage item names, relative to the repository root.
const CANDIDATE_SOURCE_DIRS: [&str; 3] = ["state-chain", "engine", "api"];
/// Every FRAME pallet stores its storage version under this postfix. It is not a metadata
/// entry, so it would otherwise be reported as an orphan for every single pallet.
const STORAGE_VERSION_KEY_POSTFIX: &[u8] = b":__STORAGE_VERSION__:";

#[derive(Default, Clone)]
struct Stats {
	count: usize,
	value_bytes: usize,
	key_bytes: usize,
	empty_values: usize,
	max_value: usize,
	min_value: usize,
	/// Suffixes (key bytes after the 32-byte prefix), only retained for the report's
	/// detailed section. Kept unconditionally: the whole snapshot is in memory anyway.
	suffixes: Vec<Vec<u8>>,
	value_sizes: Vec<usize>,
}

impl Stats {
	fn add(&mut self, key: &[u8], value: &[u8]) {
		if self.count == 0 {
			self.min_value = value.len();
		}
		self.count += 1;
		self.value_bytes += value.len();
		self.key_bytes += key.len();
		if value.is_empty() {
			self.empty_values += 1;
		}
		self.max_value = self.max_value.max(value.len());
		self.min_value = self.min_value.min(value.len());
		self.value_sizes.push(value.len());
		if key.len() > 32 {
			self.suffixes.push(key[32..].to_vec());
		}
	}

	fn total_bytes(&self) -> usize {
		self.value_bytes + self.key_bytes
	}

	/// Requires `value_sizes` to have been sorted first.
	fn percentile(&self, p: f64) -> usize {
		if self.value_sizes.is_empty() {
			return 0
		}
		self.value_sizes[(((self.value_sizes.len() - 1) as f64) * p).round() as usize]
	}
}

struct ItemInfo {
	pallet: String,
	item: String,
	hashers: Vec<StorageHasher>,
	key_type: String,
	value_type: String,
}

impl ItemInfo {
	fn full_name(&self) -> String {
		format!("{}::{}", self.pallet, self.item)
	}
}

/// Number of leading bytes a hasher contributes before the (optionally) concatenated raw key.
/// `None` means the raw key is not recoverable from the trie key.
fn hasher_prefix_len(h: &StorageHasher) -> Option<usize> {
	match h {
		StorageHasher::Blake2_128Concat => Some(16),
		StorageHasher::Twox64Concat => Some(8),
		StorageHasher::Identity => Some(0),
		StorageHasher::Blake2_128 | StorageHasher::Twox128 => None,
		StorageHasher::Blake2_256 | StorageHasher::Twox256 => None,
	}
}

fn hasher_name(h: &StorageHasher) -> &'static str {
	match h {
		StorageHasher::Blake2_128 => "Blake2_128",
		StorageHasher::Blake2_256 => "Blake2_256",
		StorageHasher::Blake2_128Concat => "Blake2_128Concat",
		StorageHasher::Twox128 => "Twox128",
		StorageHasher::Twox256 => "Twox256",
		StorageHasher::Twox64Concat => "Twox64Concat",
		StorageHasher::Identity => "Identity",
	}
}

/// Storage item names are Twox128 hashes, so a removed item can only be identified by hashing
/// candidate names and looking for a match. A removed item no longer has a `pub type`
/// declaration, but its name almost always survives somewhere — an error or event variant, a
/// migration's `old` module, an RPC type — so this sweeps every type-shaped identifier in the
/// source tree rather than only declarations. Junk candidates cost nothing: a 128-bit hash
/// makes an accidental match impossible.
fn candidate_names() -> BTreeMap<Vec<u8>, String> {
	fn visit(dir: &std::path::Path, out: &mut BTreeMap<Vec<u8>, String>) {
		let Ok(entries) = std::fs::read_dir(dir) else { return };
		for entry in entries.flatten() {
			let path = entry.path();
			if path.is_dir() {
				if path.file_name().is_some_and(|n| n != "target") {
					visit(&path, out);
				}
			} else if path.extension().is_some_and(|e| e == "rs") {
				let Ok(contents) = std::fs::read_to_string(&path) else { continue };
				for token in contents.split(|c: char| !c.is_ascii_alphanumeric()) {
					if looks_like_a_type_name(token) {
						out.entry(Twox128::hash(token.as_bytes()).to_vec())
							.or_insert_with(|| token.to_string());
					}
				}
			}
		}
	}

	// The repo root, resolved from this crate's location at build time.
	let Some(repo_root) = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2)
	else {
		return Default::default()
	};
	let mut out = BTreeMap::new();
	for dir in CANDIDATE_SOURCE_DIRS {
		visit(&repo_root.join(dir), &mut out);
	}
	add_names_from_git_history(repo_root, &mut out);
	out
}

fn looks_like_a_type_name(token: &str) -> bool {
	token.len() >= 3 &&
		token.len() <= 64 &&
		token.starts_with(|c: char| c.is_ascii_uppercase()) &&
		token.chars().any(|c| c.is_ascii_lowercase()) &&
		// Addresses and hashes from test fixtures are the bulk of the noise.
		!(token.len() >= 8 && token.chars().all(|c| c.is_ascii_hexdigit()))
}

/// An item removed long enough ago leaves nothing in the working tree, but its `pub type`
/// declaration survives in the diff history. Best-effort and additive: yields nothing outside
/// a git checkout, and less on a shallow clone. Takes a few seconds; the diff is streamed
/// rather than buffered because `git log -p` over a deep clone is arbitrarily large.
fn add_names_from_git_history(repo_root: &std::path::Path, out: &mut BTreeMap<Vec<u8>, String>) {
	use std::io::BufRead;

	let Ok(mut child) = std::process::Command::new("git")
		.args(["log", "--all", "-p", "-U0", "--", "*.rs"])
		.current_dir(repo_root)
		.stdout(std::process::Stdio::piped())
		.stderr(std::process::Stdio::null())
		.spawn()
	else {
		return
	};
	if let Some(stdout) = child.stdout.take() {
		for line in std::io::BufReader::new(stdout).lines().map_while(Result::ok) {
			// Added or removed declaration lines, i.e. `+pub type Foo<T> = ...`.
			let Some(rest) = line.strip_prefix(['+', '-']) else { continue };
			let Some(rest) = rest.trim_start().strip_prefix("pub type ") else { continue };
			let name = rest
				.chars()
				.take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
				.collect::<String>();
			if looks_like_a_type_name(&name) {
				out.entry(Twox128::hash(name.as_bytes()).to_vec()).or_insert(name);
			}
		}
	}
	let _ = child.wait();
}

fn human(bytes: usize) -> String {
	const UNITS: [&str; 4] = ["B", "KiB", "MiB", "GiB"];
	let mut v = bytes as f64;
	let mut u = 0;
	while v >= 1024.0 && u < UNITS.len() - 1 {
		v /= 1024.0;
		u += 1;
	}
	if u == 0 {
		format!("{bytes} B")
	} else {
		format!("{v:.1} {}", UNITS[u])
	}
}

/// Best-effort interpretation of the first key component of a map, used to spot
/// stale ranges (e.g. block numbers or epochs that should have been cleaned up) and
/// lopsided fan-out (one account/id owning a disproportionate share of a double map).
fn summarise_first_key_component(suffixes: &[Vec<u8>], hashers: &[StorageHasher]) -> Vec<String> {
	let mut out = Vec::new();
	let Some(hash_len) = hashers.first().and_then(hasher_prefix_len) else { return out };

	// A concat hasher emits `hash(key) ++ key`, so equal leading `hash_len` bytes means
	// equal first key, whatever the key type is. Works for maps of any arity.
	if hash_len > 0 {
		let mut fan_out = BTreeMap::<&[u8], usize>::new();
		for s in suffixes.iter().filter(|s| s.len() >= hash_len) {
			*fan_out.entry(&s[..hash_len]).or_default() += 1;
		}
		if !fan_out.is_empty() && hashers.len() > 1 {
			let max = fan_out.values().copied().max().unwrap_or(0);
			out.push(format!(
				"{} distinct first keys, avg fan-out {:.1}, max fan-out {}",
				fan_out.len(),
				suffixes.len() as f64 / fan_out.len() as f64,
				max,
			));
			// Few enough groups to enumerate: the raw key follows the concat hash, so show
			// it decoded as a u32 (epoch/block-number keys, the usual stale-data suspects).
			if fan_out.len() <= 16 {
				let mut groups = suffixes
					.iter()
					.filter(|s| s.len() >= hash_len + 4)
					.fold(BTreeMap::<u32, usize>::new(), |mut acc, s| {
						let mut buf = [0u8; 4];
						buf.copy_from_slice(&s[hash_len..hash_len + 4]);
						*acc.entry(u32::from_le_bytes(buf)).or_default() += 1;
						acc
					})
					.into_iter()
					.collect::<Vec<_>>();
				groups.sort_by_key(|(_, c)| std::cmp::Reverse(*c));
				out.push(format!(
					"first key as u32 -> entries: {}",
					groups.iter().map(|(k, c)| format!("{k}: {c}")).collect::<Vec<_>>().join(", ")
				));
			}
		} else if !fan_out.is_empty() {
			out.push(format!("{} distinct keys", fan_out.len()));
		}
	}

	// Only single-key maps let us pin down the raw key bytes unambiguously.
	if hashers.len() != 1 {
		return out
	}
	let raws: Vec<&[u8]> =
		suffixes.iter().filter(|s| s.len() > hash_len).map(|s| &s[hash_len..]).collect();
	let Some(first) = raws.first() else { return out };
	if !raws.iter().all(|r| r.len() == first.len()) {
		out.push("variable-length raw key".to_string());
		return out
	}
	let width = first.len();

	match width {
		1 | 2 | 4 | 8 | 16 => {
			let mut values = raws
				.iter()
				.map(|r| {
					let mut buf = [0u8; 16];
					buf[..width].copy_from_slice(&r[..width]);
					u128::from_le_bytes(buf)
				})
				.collect::<Vec<_>>();
			values.sort_unstable();
			out.push(format!(
				"as u{}: min={} max={} span={}",
				width * 8,
				values[0],
				values[values.len() - 1],
				values[values.len() - 1].saturating_sub(values[0]),
			));
		},
		32 => out.push("32-byte key (AccountId / hash)".to_string()),
		n => out.push(format!("uniform raw key of {n} bytes")),
	}
	out
}

pub fn analyse(
	block_hash: state_chain_runtime::Hash,
	pairs: &Pairs,
	trie_nodes: &TrieNodes,
) -> String {
	let metadata = state_chain_runtime::Runtime::metadata();
	let RuntimeMetadata::V14(v14) = &metadata.1 else {
		panic!("expected V14 metadata");
	};

	let type_name = |id: u32| -> String {
		v14.types
			.resolve(id)
			.map(|t| {
				if t.path.segments.is_empty() {
					format!("{:?}", t.type_def).split_whitespace().next().unwrap_or("?").to_string()
				} else {
					t.path.segments.join("::")
				}
			})
			.unwrap_or_else(|| "?".to_string())
	};

	// 32-byte storage prefix -> declared storage item.
	let mut declared = BTreeMap::<Vec<u8>, ItemInfo>::new();
	// 16-byte pallet prefix -> pallet name, so orphans can at least be attributed to a pallet.
	let mut pallet_prefixes = BTreeMap::<Vec<u8>, String>::new();

	for pallet in &v14.pallets {
		let Some(storage) = &pallet.storage else { continue };
		let pallet_prefix = Twox128::hash(storage.prefix.as_bytes()).to_vec();
		pallet_prefixes.insert(pallet_prefix.clone(), storage.prefix.clone());
		for entry in &storage.entries {
			let mut key = pallet_prefix.clone();
			key.extend_from_slice(&Twox128::hash(entry.name.as_bytes()));
			let (hashers, key_type, value_type) = match &entry.ty {
				StorageEntryType::Plain(v) => (vec![], "-".to_string(), type_name(v.id)),
				StorageEntryType::Map { hashers, key, value } =>
					(hashers.clone(), type_name(key.id), type_name(value.id)),
			};
			declared.insert(
				key,
				ItemInfo {
					pallet: storage.prefix.clone(),
					item: entry.name.clone(),
					hashers,
					key_type,
					value_type,
				},
			);
		}
	}

	// Pallets registered in the runtime but with no `Storage` section still own a prefix.
	for info in <state_chain_runtime::AllPalletsWithSystem as frame_support::traits::PalletsInfoAccess>::infos() {
		pallet_prefixes
			.entry(Twox128::hash(info.name.as_bytes()).to_vec())
			.or_insert_with(|| info.name.to_string());
	}

	// If the source tree isn't reachable this comes back empty, and orphans are still
	// reported, just by hash rather than by name.
	let candidates = candidate_names();
	let name_item = |hash: &[u8]| -> String {
		if hash == Twox128::hash(STORAGE_VERSION_KEY_POSTFIX) {
			return ":__STORAGE_VERSION__: (FRAME pallet storage version, not a metadata entry)"
				.to_string()
		}
		match candidates.get(hash) {
			Some(name) => format!("{name} (matched by hash)"),
			None => format!("0x{}", hex::encode(hash)),
		}
	};

	let mut by_prefix = BTreeMap::<Vec<u8>, Stats>::new();
	let mut well_known = BTreeMap::<Vec<u8>, Stats>::new();
	let mut total_keys = 0usize;
	let mut total_bytes = 0usize;

	for (k, v) in pairs.iter() {
		total_keys += 1;
		total_bytes += k.len() + v.len();
		if k.first() == Some(&b':') || k.len() < 32 {
			well_known.entry(k.clone()).or_default().add(k, v);
		} else {
			by_prefix.entry(k[..32].to_vec()).or_default().add(k, v);
		}
	}

	let mut report = String::new();
	let mut w = |s: String| {
		report.push_str(&s);
		report.push('\n');
	};

	// The metadata we map against comes from the *local* runtime. If the snapshot predates it,
	// items removed since then show up as orphans, so surface both versions.
	let onchain_spec_version = {
		let mut key = Twox128::hash(b"System").to_vec();
		key.extend_from_slice(&Twox128::hash(b"LastRuntimeUpgrade"));
		pairs
			.iter()
			.find(|(k, _)| *k == key)
			.and_then(|(_, v)| {
				<(codec::Compact<u32>, String)>::decode(&mut &v[..]).ok().map(|(v, _)| v.0)
			})
			.map(|v| v.to_string())
			.unwrap_or_else(|| "unknown".to_string())
	};

	w("# State storage analysis".to_string());
	w(String::new());
	w(format!("Block hash: `{block_hash:?}`"));
	w(format!("On-chain spec version (snapshot): {onchain_spec_version}"));
	w(format!(
		"Local runtime spec version (source of metadata): {}",
		state_chain_runtime::VERSION.spec_version
	));
	w(format!(
		"Flat state: {total_keys} entries / {} of key+value bytes ({total_bytes} B)",
		human(total_bytes)
	));
	w(format!(
		"Trie node database: {} nodes / {} (what a node actually stores on disk)",
		trie_nodes.len(),
		human(trie_nodes.iter().map(|(k, (v, _))| k.len() + v.len()).sum::<usize>()),
	));
	w(format!("Distinct 32-byte storage prefixes present: {}", by_prefix.len()));
	w(format!("Storage items declared in metadata: {}", declared.len()));
	w(String::new());

	// ---------------------------------------------------------------------------------------
	// Per-pallet rollup.
	// ---------------------------------------------------------------------------------------
	let mut per_pallet = BTreeMap::<String, (usize, usize, usize)>::new(); // (items, keys, bytes)
	for (prefix, stats) in &by_prefix {
		let name = declared
			.get(prefix)
			.map(|i| i.pallet.clone())
			.or_else(|| pallet_prefixes.get(&prefix[..16]).cloned())
			.unwrap_or_else(|| format!("<unknown pallet 0x{}>", hex::encode(&prefix[..16])));
		let e = per_pallet.entry(name).or_default();
		e.0 += 1;
		e.1 += stats.count;
		e.2 += stats.total_bytes();
	}
	let mut per_pallet = per_pallet.into_iter().collect::<Vec<_>>();
	per_pallet.sort_by_key(|(_, (_, _, bytes))| std::cmp::Reverse(*bytes));

	w("## Per-pallet totals".to_string());
	w(String::new());
	w("| Pallet | Items | Keys | Bytes | % of state |".to_string());
	w("|---|---:|---:|---:|---:|".to_string());
	for (name, (items, keys, bytes)) in &per_pallet {
		w(format!(
			"| {name} | {items} | {keys} | {} | {:.2}% |",
			human(*bytes),
			100.0 * *bytes as f64 / total_bytes as f64
		));
	}
	w(String::new());

	// ---------------------------------------------------------------------------------------
	// Well-known / non-prefixed keys (`:code`, `:heappages`, ...).
	// ---------------------------------------------------------------------------------------
	w("## Well-known keys".to_string());
	w(String::new());
	w("| Key | Bytes |".to_string());
	w("|---|---:|".to_string());
	let mut wk = well_known.into_iter().collect::<Vec<_>>();
	wk.sort_by_key(|(_, s)| std::cmp::Reverse(s.total_bytes()));
	for (k, s) in wk {
		let name = String::from_utf8(k.clone())
			.ok()
			.filter(|s| s.chars().all(|c| c.is_ascii_graphic()))
			.unwrap_or_else(|| format!("0x{}", hex::encode(&k)));
		w(format!("| `{name}` | {} |", human(s.total_bytes())));
	}
	w(String::new());

	// ---------------------------------------------------------------------------------------
	// Every present storage item, largest first.
	// ---------------------------------------------------------------------------------------
	for stats in by_prefix.values_mut() {
		stats.value_sizes.sort_unstable();
	}
	let declared_prefixes = declared.keys().cloned().collect::<std::collections::BTreeSet<_>>();
	let present_prefixes = by_prefix.keys().cloned().collect::<std::collections::BTreeSet<_>>();

	let mut items = by_prefix.iter().collect::<Vec<_>>();
	items.sort_by_key(|(_, s)| std::cmp::Reverse(s.total_bytes()));

	w("## Storage items by total size".to_string());
	w(String::new());
	w("| # | Item | Keys | Total | Values | Keys(bytes) | avg val | p50 | p99 | max | empty |"
		.to_string());
	w("|---:|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|".to_string());
	for (i, (prefix, stats)) in items.iter().enumerate() {
		let name = match declared.get(*prefix) {
			Some(info) => info.full_name(),
			None => format!(
				"⚠️ ORPHAN {}::{}",
				pallet_prefixes
					.get(&prefix[..16])
					.cloned()
					.unwrap_or_else(|| format!("0x{}", hex::encode(&prefix[..16]))),
				name_item(&prefix[16..])
			),
		};
		w(format!(
			"| {} | {name} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
			i + 1,
			stats.count,
			human(stats.total_bytes()),
			human(stats.value_bytes),
			human(stats.key_bytes),
			stats.value_bytes / stats.count.max(1),
			stats.percentile(0.5),
			stats.percentile(0.99),
			stats.max_value,
			stats.empty_values,
		));
	}
	w(String::new());

	// ---------------------------------------------------------------------------------------
	// Orphaned prefixes: present in state but absent from the current metadata.
	// ---------------------------------------------------------------------------------------
	let storage_version_hash = Twox128::hash(STORAGE_VERSION_KEY_POSTFIX);
	let (version_keys, orphans): (Vec<_>, Vec<_>) = items
		.iter()
		.filter(|(prefix, _)| !declared_prefixes.contains(*prefix))
		.partition(|(prefix, _)| prefix[16..] == storage_version_hash);

	w("## Orphaned prefixes (state with no matching metadata entry)".to_string());
	w(String::new());
	w(format!(
		"Excluding {} pallet `:__STORAGE_VERSION__:` keys ({}), which are expected.",
		version_keys.len(),
		human(version_keys.iter().map(|(_, s)| s.total_bytes()).sum::<usize>()),
	));
	w(String::new());
	if orphans.is_empty() {
		w("_None. Every prefix in state maps to a declared storage item._".to_string());
	} else {
		w(format!(
			"{} orphaned prefixes, {} keys, {} total.",
			orphans.len(),
			orphans.iter().map(|(_, s)| s.count).sum::<usize>(),
			human(orphans.iter().map(|(_, s)| s.total_bytes()).sum::<usize>()),
		));
		w(String::new());
		w("| Pallet | Item | Keys | Bytes | Sample key suffixes |".to_string());
		w("|---|---|---:|---:|---|".to_string());
		for (prefix, stats) in &orphans {
			let owner = pallet_prefixes
				.get(&prefix[..16])
				.cloned()
				.unwrap_or_else(|| format!("<removed pallet 0x{}>", hex::encode(&prefix[..16])));
			let samples = stats
				.suffixes
				.iter()
				.take(ORPHAN_SAMPLES)
				.map(|s| format!("0x{}", hex::encode(s)))
				.collect::<Vec<_>>()
				.join(", ");
			w(format!(
				"| {owner} | {} | {} | {} | {} |",
				name_item(&prefix[16..]),
				stats.count,
				human(stats.total_bytes()),
				if samples.is_empty() { "(plain value)".to_string() } else { samples },
			));
		}
	}
	w(String::new());

	// ---------------------------------------------------------------------------------------
	// Physical footprint. Trie node keys are `path prefix ++ node hash`, so a node can only be
	// attributed to a storage item once the path is deep enough to cover the 32-byte prefix.
	// Shallow nodes (near the root) are unattributable, hence the residual bucket.
	// ---------------------------------------------------------------------------------------
	let mut trie_by_pallet = BTreeMap::<String, (usize, usize)>::new();
	let mut unattributed = (0usize, 0usize);
	for (k, (v, _)) in trie_nodes.iter() {
		let size = k.len() + v.len();
		let hash_len = std::mem::size_of::<state_chain_runtime::Hash>();
		let path_len = k.len().saturating_sub(hash_len);
		let name = (path_len >= 32)
			.then(|| declared.get(&k[..32]).map(|i| i.pallet.clone()))
			.flatten()
			.or_else(|| (path_len >= 16).then(|| pallet_prefixes.get(&k[..16]).cloned()).flatten());
		match name {
			Some(pallet) => {
				let e = trie_by_pallet.entry(pallet).or_default();
				e.0 += 1;
				e.1 += size;
			},
			None => {
				unattributed.0 += 1;
				unattributed.1 += size;
			},
		}
	}
	let trie_total = trie_nodes.iter().map(|(k, (v, _))| k.len() + v.len()).sum::<usize>();
	let mut trie_by_pallet = trie_by_pallet.into_iter().collect::<Vec<_>>();
	trie_by_pallet.sort_by_key(|(_, (_, bytes))| std::cmp::Reverse(*bytes));

	w("## Physical trie footprint by pallet".to_string());
	w(String::new());
	w("| Pallet | Nodes | Bytes | % of trie |".to_string());
	w("|---|---:|---:|---:|".to_string());
	for (name, (nodes, bytes)) in trie_by_pallet.iter().take(20) {
		w(format!(
			"| {name} | {nodes} | {} | {:.2}% |",
			human(*bytes),
			100.0 * *bytes as f64 / trie_total as f64
		));
	}
	w(format!(
		"| _unattributed (shallow nodes)_ | {} | {} | {:.2}% |",
		unattributed.0,
		human(unattributed.1),
		100.0 * unattributed.1 as f64 / trie_total as f64
	));
	w(String::new());

	// ---------------------------------------------------------------------------------------
	// Declared but empty: candidates for removal, and a sanity check on the mapping.
	// ---------------------------------------------------------------------------------------
	let unused = declared
		.iter()
		.filter(|(prefix, _)| !present_prefixes.contains(*prefix))
		.map(|(_, info)| info.full_name())
		.collect::<Vec<_>>();
	w(format!("## Declared but absent from state ({} items)", unused.len()));
	w(String::new());
	w(unused.join(", "));
	w(String::new());

	// ---------------------------------------------------------------------------------------
	// Key-space detail for the biggest items: growth shape and stale-range detection.
	// ---------------------------------------------------------------------------------------
	w(format!("## Key-space detail for the {DETAILED_ITEMS} largest items"));
	w(String::new());
	for (prefix, stats) in items.iter().take(DETAILED_ITEMS) {
		let Some(info) = declared.get(*prefix) else { continue };
		let shape = if info.hashers.is_empty() {
			"Plain value".to_string()
		} else {
			format!(
				"Map<{}> keyed by {}",
				info.hashers.iter().map(hasher_name).collect::<Vec<_>>().join(" + "),
				info.key_type
			)
		};
		w(format!("### {}", info.full_name()));
		w(String::new());
		w(format!("- {shape}"));
		w(format!("- Value type: `{}`", info.value_type));
		w(format!(
			"- {} keys, {} total ({} values / {} key overhead)",
			stats.count,
			human(stats.total_bytes()),
			human(stats.value_bytes),
			human(stats.key_bytes)
		));
		if !info.hashers.is_empty() {
			let summary = summarise_first_key_component(&stats.suffixes, &info.hashers);
			if summary.is_empty() {
				w("- First key component: not recoverable from trie key".to_string());
			}
			for line in summary {
				w(format!("- First key component: {line}"));
			}
		}
		w(String::new());
	}

	report
}

/// Print a short summary and write the full report in this crate's directory.
pub fn run(
	block_hash: state_chain_runtime::Hash,
	pairs: &Pairs,
	trie_nodes: &TrieNodes,
) -> anyhow::Result<()> {
	let report = analyse(block_hash, pairs, trie_nodes);
	let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
		.join(format!("storage-report-{block_hash:?}.md"));
	std::fs::write(&path, &report)?;
	println!("Storage report written to {}", path.display());
	// Echo the header + per-pallet table so the terminal is useful on its own.
	for line in report.lines().take_while(|l| !l.starts_with("## Well-known")) {
		println!("{line}");
	}
	Ok(())
}
