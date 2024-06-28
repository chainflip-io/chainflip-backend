use std::collections::BTreeMap;

use crate::StateChainBlock;
use cf_test_utilities::TestExternalities;
use frame_remote_externalities::RemoteExternalities;
use frame_support::StorageHasher;

pub trait RuntimeTest: Default {
	fn setup() -> Self {
		Default::default()
	}

	fn run(
		self,
		block_hash: state_chain_runtime::Hash,
		ext: TestExternalities<state_chain_runtime::Runtime>,
	) -> anyhow::Result<()>;
}

pub mod example;

pub fn run_all(ext: RemoteExternalities<StateChainBlock>) -> anyhow::Result<()> {
	let block_hash = ext.block_hash;
	let state_version = ext.state_version;
	let (raw_storage, storage_root) = ext.inner_ext.into_raw_snapshot();

	let mut size_by_prefix = BTreeMap::<Vec<u8>, (usize, usize)>::new();
	for (k, (v, _)) in raw_storage.iter() {
		size_by_prefix
			// 16 bytes for the prefix, 16 bytes for the key
			.entry(k.iter().take(16 + 16).copied().collect::<Vec<_>>())
			.and_modify(|(c, s)| {
				*c += 1;
				*s += v.len();
			})
			.or_insert((1, v.len()));
	}
	let mut size_by_prefix = size_by_prefix.into_iter().collect::<Vec<_>>();
	size_by_prefix.sort_by_key(|(_, (_, size))| std::cmp::Reverse(*size));

	let pallets_by_prefix =
		<state_chain_runtime::AllPalletsWithSystem as frame_support::traits::PalletsInfoAccess>::infos()
			.into_iter()
			.map(|info| {
				let prefix = frame_support::Twox128::hash(info.name.as_bytes());
				(prefix, info.name)
			})
			.collect::<BTreeMap<_, _>>();

	println!("=== Storage usage top 10:");
	for (prefix, (count, size)) in size_by_prefix.into_iter().take(10) {
		let decription = if let Some(name) = pallets_by_prefix.get(&prefix[..16]) {
			format!("Pallet {}[{}]", name, hex::encode(&prefix[16..]),)
		} else if hex::encode(&prefix[..5]) == "3a636f6465" {
			format!("Runtime WASM")
		} else {
			format!("Prefix 0x{}", hex::encode(prefix),)
		};

		println!(
			"{:<64}: num_items: {:>7} / total_size: ~{:>3}{}",
			decription,
			count,
			if size < 1_000_000 {
				size / 1_000
			} else {
				size / 1_000_000
			},
			if size < 1_000_000 { "Kb" } else { "Mb" },
		);
	}
	println!("========================");

	log::info!("Running tests for block hash: {:?}", block_hash);

	for test in [example::Test::setup()] {
		test.run(
			block_hash,
			TestExternalities::from_raw_snapshot(
				raw_storage.clone(),
				storage_root.clone(),
				state_version,
			),
		)?;
	}

	log::info!("All tests passed for block hash: {:?}", block_hash);

	Ok(())
}
