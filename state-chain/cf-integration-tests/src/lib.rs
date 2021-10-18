#![feature(assert_matches)]
#[cfg(test)]
mod tests {
	use frame_support::sp_io::TestExternalities;
	use frame_support::traits::GenesisBuild;
	use frame_support::traits::OnInitialize;
	use sp_consensus_aura::sr25519::AuthorityId as AuraId;
	use sp_core::crypto::{Pair, Public};
	use sp_finality_grandpa::AuthorityId as GrandpaId;
	use sp_runtime::traits::Zero;
	use sp_runtime::Storage;
	use state_chain_runtime::opaque::SessionKeys;
	use state_chain_runtime::{constants::common::*, AccountId, Runtime, System};
	use state_chain_runtime::{
		Auction, Emissions, Flip, Governance, Online, Reputation, Rewards, Session, Staking,
		Timestamp, Validator, Vaults,
	};

	use cf_traits::{BlockNumber, EpochIndex, FlipBalance};

	pub const ALICE: [u8; 32] = [4u8; 32];
	pub const BOB: [u8; 32] = [5u8; 32];
	pub const CHARLIE: [u8; 32] = [6u8; 32];
	pub const ERIN: [u8; 32] = [7u8; 32];

	pub const BLOCK_TIME: u64 = 1000;

	pub fn get_from_seed<TPublic: Public>(seed: &str) -> <TPublic::Pair as Pair>::Public {
		TPublic::Pair::from_string(&format!("//{}", seed), None)
			.expect("static values are valid; qed")
			.public()
	}
	fn run_to_block(n: u32) {
		pub const INIT_TIMESTAMP: u64 = 30_000;
		while System::block_number() < n {
			System::set_block_number(System::block_number() + 1);
			Timestamp::set_timestamp((System::block_number() as u64 * BLOCK_TIME) + INIT_TIMESTAMP);
			Session::on_initialize(System::block_number());
			Flip::on_initialize(System::block_number());
			Staking::on_initialize(System::block_number());
			Auction::on_initialize(System::block_number());
			Emissions::on_initialize(System::block_number());
			Governance::on_initialize(System::block_number());
			Reputation::on_initialize(System::block_number());
			Vaults::on_initialize(System::block_number());
			Validator::on_initialize(System::block_number());
		}
	}

	pub struct ExtBuilder {
		accounts: Vec<(AccountId, FlipBalance)>,
		winners: Vec<AccountId>,
		root: AccountId,
		blocks_per_epoch: BlockNumber,
	}

	impl Default for ExtBuilder {
		fn default() -> Self {
			Self {
				accounts: vec![],
				winners: vec![],
				root: AccountId::default(),
				blocks_per_epoch: Zero::zero(),
			}
		}
	}

	impl ExtBuilder {
		fn accounts(mut self, accounts: Vec<(AccountId, FlipBalance)>) -> Self {
			self.accounts = accounts;
			self
		}

		fn winners(mut self, winners: Vec<AccountId>) -> Self {
			self.winners = winners;
			self
		}

		fn root(mut self, root: AccountId) -> Self {
			self.root = root;
			self
		}

		fn blocks_per_epoch(mut self, blocks_per_epoch: BlockNumber) -> Self {
			self.blocks_per_epoch = blocks_per_epoch;
			self
		}

		fn configure_storages(&self, storage: &mut Storage) {
			pallet_cf_flip::GenesisConfig::<Runtime> {
				total_issuance: TOTAL_ISSUANCE,
			}
			.assimilate_storage(storage)
			.unwrap();

			pallet_cf_staking::GenesisConfig::<Runtime> {
				genesis_stakers: self.accounts.clone(),
			}
			.assimilate_storage(storage)
			.unwrap();

			pallet_session::GenesisConfig::<Runtime> {
				keys: self
					.accounts
					.iter()
					.map(|x| {
						(
							x.0.clone(),
							x.0.clone(),
							SessionKeys {
								aura: get_from_seed::<AuraId>(&x.0.clone().to_string()),
								grandpa: get_from_seed::<GrandpaId>(&x.0.clone().to_string()),
							},
						)
					})
					.collect::<Vec<_>>(),
			}
			.assimilate_storage(storage)
			.unwrap();

			pallet_cf_auction::GenesisConfig::<Runtime> {
				validator_size_range: (1, MAX_VALIDATORS),
				winners: self.winners.clone(),
				minimum_active_bid: TOTAL_ISSUANCE / 100,
			}
			.assimilate_storage(storage)
			.unwrap();

			GenesisBuild::<Runtime>::assimilate_storage(
				&pallet_cf_emissions::GenesisConfig {
					validator_emission_inflation: VALIDATOR_EMISSION_INFLATION_BPS,
					backup_validator_emission_inflation: BACKUP_VALIDATOR_EMISSION_INFLATION_BPS,
				},
				storage,
			)
			.unwrap();

			pallet_cf_governance::GenesisConfig::<Runtime> {
				members: vec![self.root.clone()],
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

			pallet_cf_validator::GenesisConfig::<Runtime> {
				blocks_per_epoch: self.blocks_per_epoch,
			}
			.assimilate_storage(storage)
			.unwrap();
		}

		/// Default ext configuration with BlockNumber 1
		pub fn build(&self) -> TestExternalities {
			let mut storage = frame_system::GenesisConfig::default()
				.build_storage::<Runtime>()
				.unwrap();

			self.configure_storages(&mut storage);

			let mut ext = TestExternalities::from(storage);
			ext.execute_with(|| System::set_block_number(1));

			ext
		}
	}

	mod genesis {
		use super::*;
		use cf_traits::{AuctionPhase, AuctionResult, Auctioneer, NonceIdentifier, StakeTransfer};

		const GENESIS_BALANCE: FlipBalance = TOTAL_ISSUANCE / 100;

		pub fn default() -> ExtBuilder {
			ExtBuilder::default()
				.accounts(vec![
					(AccountId::from(ALICE), GENESIS_BALANCE),
					(AccountId::from(BOB), GENESIS_BALANCE),
					(AccountId::from(CHARLIE), GENESIS_BALANCE),
				])
				.winners(vec![
					AccountId::from(ALICE),
					AccountId::from(BOB),
					AccountId::from(CHARLIE),
				])
				.root(AccountId::from(ERIN))
		}

		#[test]
		// The following state is to be expected at genesis
		// - Total issuance
		// - The genesis validators are all staked equally
		// - The minimum active bid is set at the stake for a genesis validator
		// - The genesis validators are available via validator_lookup()
		// - The genesis validators are in the session
		// - No auction has been run yet
		// - The genesis validators are considered offline for this heartbeat interval
		// - No emissions have been made
		// - No rewards have been distributed
		// - No vault rotation has occurred
		// - Relevant nonce are at 0
		// - Governance has its member
		// - There have been no proposals
		// - Emission inflation for both validators and backup validators are set
		// - No one has reputation
		fn state_of_genesis_is_as_expected() {
			default().build().execute_with(|| {
				// Confirmation that we have our assumed state at block 1
				assert_eq!(
					Flip::total_issuance(),
					TOTAL_ISSUANCE,
					"we have issued the total issuance"
				);

				let accounts = [
					AccountId::from(ALICE),
					AccountId::from(BOB),
					AccountId::from(CHARLIE),
				];

				for account in accounts.iter() {
					assert_eq!(
						Flip::stakeable_balance(account),
						GENESIS_BALANCE,
						"the account has its stake"
					);
				}

				assert_eq!(
					Auction::current_auction_index(),
					0,
					"we should have had no auction yet"
				);
				assert_matches!(
					Auction::auction_result(),
					Some(AuctionResult {
						minimum_active_bid: GENESIS_BALANCE,
						winners: accounts
					})
				);

				assert_eq!(
					Session::validators(),
					accounts,
					"the validators are those expected at genesis"
				);

				assert_eq!(
					Validator::epoch_number_of_blocks(),
					0,
					"epochs will not rotate automatically from genesis"
				);

				for account in accounts.iter() {
					assert_eq!(
						Validator::validator_lookup(account),
						Some(()),
						"validator is present in lookup"
					);
				}

				for account in accounts.iter() {
					assert_eq!(
						Online::liveness(account),
						Some(pallet_cf_online::liveness::Node {
							is_validator: true,
							liveness: 1,
						}),
						"node should have not sent a heartbeat"
					);
				}

				assert_eq!(Emissions::last_mint_block(), 0, "no emissions");

				assert_eq!(
					Rewards::offchain_funds(pallet_cf_rewards::VALIDATOR_REWARDS),
					0,
					"no rewards"
				);

				assert_eq!(Vaults::current_request(), 0, "no key generation requests");
				assert_eq!(
					Vaults::chain_nonces(NonceIdentifier::Ethereum),
					None,
					"nonce not incremented"
				);

				assert!(
					Governance::members().contains(&AccountId::from(ERIN)),
					"expected governor"
				);
				assert_eq!(
					Governance::number_of_proposals(),
					0,
					"no proposal for governance"
				);

				assert_eq!(
					Emissions::validator_emission_inflation(),
					VALIDATOR_EMISSION_INFLATION_BPS,
					"invalid emission inflation for validators"
				);

				assert_eq!(
					Emissions::backup_validator_emission_inflation(),
					BACKUP_VALIDATOR_EMISSION_INFLATION_BPS,
					"invalid emission inflation for backup validators"
				);

				for account in accounts.iter() {
					assert_eq!(
						Reputation::reputation(account),
						pallet_cf_reputation::Reputation::<BlockNumber>::default(),
						"validator shouldn't have reputation points"
					);
				}
			});
		}
	}
}
