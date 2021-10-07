#![feature(assert_matches)]
#[cfg(test)]
mod tests {
	use frame_support::sp_io::TestExternalities;
	use frame_support::traits::GenesisBuild;
	use sp_consensus_aura::sr25519::AuthorityId as AuraId;
	use sp_core::crypto::{Pair, Public};
	use sp_finality_grandpa::AuthorityId as GrandpaId;
	use sp_runtime::Storage;
	use state_chain_runtime::opaque::SessionKeys;
	use state_chain_runtime::{constants::common::*, AccountId, Runtime, System};

	pub const ALICE: [u8; 32] = [4u8; 32];
	pub const BOB: [u8; 32] = [5u8; 32];
	pub const CHARLIE: [u8; 32] = [6u8; 32];
	pub const ERIN: [u8; 32] = [7u8; 32];

	pub fn get_from_seed<TPublic: Public>(seed: &str) -> <TPublic::Pair as Pair>::Public {
		TPublic::Pair::from_string(&format!("//{}", seed), None)
			.expect("static values are valid; qed")
			.public()
	}

	pub struct ExtBuilder {
		accounts: Vec<(AccountId, FlipBalance)>,
		winners: Vec<AccountId>,
		root: AccountId,
	}

	impl Default for ExtBuilder {
		fn default() -> Self {
			Self {
				accounts: vec![],
				winners: vec![],
				root: AccountId::default(),
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
				auction_size_range: (1, MAX_VALIDATORS),
				winners: self.winners.clone(),
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

			<pallet_cf_validator::GenesisConfig as GenesisBuild<Runtime>>::assimilate_storage(
				&pallet_cf_validator::GenesisConfig {},
				storage,
			)
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
		use cf_traits::{AuctionPhase, Auctioneer, NonceIdentifier, StakeTransfer};
		use state_chain_runtime::{
			Auction, Emissions, Flip, Governance, Reputation, Rewards, Session, Validator, Vaults,
		};

		#[test]
		// The following state is to be expected at genesis
		// 1. Total issuance
		// 2. The genesis validators are all staked equally
		// 3. The minimum active bid is set at the stake for a genesis validator
		// 3. The genesis validators are available via validator_lookup()
		// 4. The genesis validators are in the session
		// 5. No auction has been run yet
		// 6. The genesis validators are considered offline for this heartbeat interval
		// 7. No emissions have been made
		// 8. No rewards have been distributed
		// 9. No vault rotation has occurred
		// 10. Relevant nonce are at 0
		// 11. Governance has its member
		// 12. There have been no proposals
		fn state_of_genesis_is_as_expected() {
			const GENESIS_BALANCE: FlipBalance = TOTAL_ISSUANCE / 100;
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
				.build()
				.execute_with(|| {
					// Confirmation that we have our assumed state at block 1
					assert_eq!(Flip::total_issuance(), TOTAL_ISSUANCE);

					let accounts = [
						AccountId::from(ALICE),
						AccountId::from(BOB),
						AccountId::from(CHARLIE),
					];

					for account in accounts.iter() {
						assert_eq!(Flip::stakeable_balance(account), GENESIS_BALANCE);
					}

					assert_eq!(Auction::current_auction_index(), 0);
					assert_matches!(Auction::phase(), AuctionPhase::WaitingForBids(winners, minimum_active_bid)
						if winners == accounts && minimum_active_bid == GENESIS_BALANCE
					);

					assert_eq!(Session::validators(), accounts);

					for account in accounts.iter() {
						assert_eq!(Validator::validator_lookup(account), Some(()));
					}

					for account in accounts.iter() {
						assert_eq!(Reputation::validator_liveness(account), Some(0));
					}

					assert_eq!(Emissions::last_mint_block(), 0);

					assert_eq!(
						Rewards::offchain_funds(pallet_cf_rewards::VALIDATOR_REWARDS),
						0
					);

					assert_eq!(Vaults::current_request(), 0);
					assert_eq!(Vaults::chain_nonces(NonceIdentifier::Ethereum), None);

					assert!(Governance::members().contains(&AccountId::from(ERIN)));
					assert_eq!(Governance::number_of_proposals(), 0);
				});
		}
	}
}
