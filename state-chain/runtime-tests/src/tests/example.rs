use super::*;

#[derive(Debug, Default)]
pub struct Test;

impl RuntimeTest for Test {
	fn run(
		self,
		block_hash: state_chain_runtime::Hash,
		ext: TestExternalities<state_chain_runtime::Runtime>,
	) -> anyhow::Result<()> {
		// Can use the block hash to only run tests against certain blocks.
		if block_hash == state_chain_runtime::Hash::default() {
			return Ok(());
		}

		ext.execute_with(|| {
			assert_eq!(1 + 1, 2);
		});
		Ok(())
	}
}
