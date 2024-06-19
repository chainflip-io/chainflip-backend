use crate::StateChainBlock;
use cf_test_utilities::TestExternalities;
use frame_remote_externalities::RemoteExternalities;

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
