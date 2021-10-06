#[cfg(test)]
mod tests {
	use frame_support::sp_io::TestExternalities;
	use frame_support::traits::GenesisBuild;
	use sp_runtime::Storage;
	use state_chain_runtime::{constants::common::*, AccountId, Runtime, System};

	pub struct ExtBuilder;
	impl ExtBuilder {
		fn configure_storages(storage: &mut Storage) {
			let bashful_sr25519 = hex_literal::hex![
				"36c0078af3894b8202b541ece6c5d8fb4a091f7e5812b688e703549040473911"
			];
			let doc_sr25519 = hex_literal::hex![
				"8898758bf88855615d459f552e36bfd14e8566c8b368f6a6448942759d5c7f04"
			];
			let dopey_sr25519 = hex_literal::hex![
				"ca58f2f4ae713dbb3b4db106640a3db150e38007940dfe29e6ebb870c4ccd47e"
			];
			let snow_white = hex_literal::hex![
				"ced2e4db6ce71779ac40ccec60bf670f38abbf9e27a718b4412060688a9ad212"
			];

			let endowed_accounts: Vec<AccountId> = vec![
				bashful_sr25519.into(),
				doc_sr25519.into(),
				dopey_sr25519.into(),
				snow_white.into(),
			];
			let winners: Vec<AccountId> = vec![
				bashful_sr25519.into(),
				doc_sr25519.into(),
				dopey_sr25519.into(),
			];
			let root_key = snow_white;

			pallet_cf_flip::GenesisConfig::<Runtime> {
				total_issuance: TOTAL_ISSUANCE,
			}
			.assimilate_storage(storage)
			.unwrap();

			pallet_cf_staking::GenesisConfig::<Runtime> {
				genesis_stakers: endowed_accounts
					.iter()
					.map(|acct| (acct.clone(), TOTAL_ISSUANCE / 100))
					.collect::<Vec<(AccountId, FlipBalance)>>(),
			}
			.assimilate_storage(storage)
			.unwrap();

			pallet_cf_auction::GenesisConfig::<Runtime> {
				auction_size_range: (1, MAX_VALIDATORS),
				winners,
				minimum_active_bid: TOTAL_ISSUANCE / 100,
			}
			.assimilate_storage(storage)
			.unwrap();

			pallet_cf_emissions::GenesisConfig::<Runtime> {
				emission_per_block: BLOCK_EMISSIONS,
				..Default::default()
			}
			.assimilate_storage(storage)
			.unwrap();

			pallet_cf_governance::GenesisConfig::<Runtime> {
				members: vec![root_key.into()],
				expiry_span: EXPIRY_SPAN_IN_SECONDS,
			}
			.assimilate_storage(storage)
			.unwrap();

			pallet_cf_reputation::GenesisConfig::<Runtime> {
				accrual_ratio: (ACCRUAL_POINTS, ACCRUAL_BLOCKS),
			}
			.assimilate_storage(storage)
			.unwrap();

			pallet_cf_vaults::GenesisConfig::<Runtime> {
				ethereum_vault_key: hex_literal::hex![
					"03035e49e5db75c1008f33f7368a87ffb13f0d845dc3f9c89723e4e07a066f2667"
				]
				.to_vec(),
			}
			.assimilate_storage(storage)
			.unwrap();
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
