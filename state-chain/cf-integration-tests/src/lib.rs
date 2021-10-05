#[cfg(test)]
mod tests {
	use state_chain_runtime::{System, Runtime};
	use sp_runtime::Storage;
	use frame_support::sp_io::TestExternalities;

	pub struct ExtBuilder;
	impl ExtBuilder {
		fn configure_storages(storage: &mut Storage) {
			// let mut accounts = Vec::new();
			// for account in ACCOUNT1..=ACCOUNT3 {
			// 	accounts.push(account);
			// }
			//
			// let _ = pallet_balances::GenesisConfig::<ReferenceRuntime> {
			// 	balances: accounts.iter().cloned().map(|k|(k, 100)).collect()
			// }.assimilate_storage(storage);
		}

		/// Default ext configuration with BlockNumber 1
		pub fn build() -> TestExternalities {
			let mut storage = frame_system::GenesisConfig::default()
				.build_storage::<Runtime>()
				.unwrap();

			Self::configure_storages(&mut storage);

			let mut ext = TestExternalities::from(storage);
			ext.execute_with(|| System::set_block_number(1));

			ext
		}
	}

	#[test]
	fn should_run() {}
}
