use sp_std::collections::btree_set::BTreeSet;

use crate::mock_runtime::{
	ExtBuilder, BACKUP_NODE_EMISSION_INFLATION_PERBILL,
	CURRENT_AUTHORITY_EMISSION_INFLATION_PERBILL,
};

use super::*;
use cf_primitives::AccountRole;
use cf_traits::{AccountInfo, EpochInfo, QualifyNode};
use state_chain_runtime::{
	BitcoinThresholdSigner, EthereumThresholdSigner, PolkadotThresholdSigner,
};
pub const GENESIS_BALANCE: FlipBalance = TOTAL_ISSUANCE / 100;

const BLOCKS_PER_EPOCH: u32 = 1000;

pub fn with_test_defaults() -> ExtBuilder {
	ExtBuilder::default()
		.accounts(vec![
			(AccountId::from(ALICE), AccountRole::Validator, GENESIS_BALANCE),
			(AccountId::from(BOB), AccountRole::Validator, GENESIS_BALANCE),
			(AccountId::from(CHARLIE), AccountRole::Validator, GENESIS_BALANCE),
			(AccountId::from(BROKER), AccountRole::Broker, GENESIS_BALANCE),
			(AccountId::from(LIQUIDITY_PROVIDER), AccountRole::LiquidityProvider, GENESIS_BALANCE),
		])
		.root(AccountId::from(ERIN))
		.blocks_per_epoch(BLOCKS_PER_EPOCH)
}

#[test]
fn state_of_genesis_is_as_expected() {
	with_test_defaults().build().execute_with(|| {
		// Confirmation that we have our assumed state at block 1
		assert_eq!(Flip::total_issuance(), TOTAL_ISSUANCE, "we have issued the total issuance");

		let accounts = [AccountId::from(CHARLIE), AccountId::from(BOB), AccountId::from(ALICE)];

		for account in accounts.iter() {
			assert_eq!(<Flip as AccountInfo<_>>::balance(account), GENESIS_BALANCE,);
		}

		assert_eq!(Validator::bond(), GENESIS_BALANCE);
		assert_eq!(
			Validator::current_authorities().iter().collect::<BTreeSet<_>>(),
			accounts.iter().collect::<BTreeSet<_>>(),
			"the validators are those expected at genesis"
		);

		assert_eq!(
			Validator::blocks_per_epoch(),
			BLOCKS_PER_EPOCH,
			"epochs will not rotate automatically from genesis"
		);

		let current_epoch = Validator::current_epoch();

		for account in accounts.iter() {
			assert!(
				Validator::authority_index(current_epoch, account).is_some(),
				"authority is present in lookup"
			);
		}

		for account in accounts.iter() {
			assert!(Reputation::is_qualified(account), "Genesis nodes start online");
		}

		assert_eq!(Emissions::last_supply_update_block(), 0, "no emissions");

		assert_eq!(EthereumThresholdSigner::ceremony_id_counter(), 0, "no key generation requests");
		assert_eq!(PolkadotThresholdSigner::ceremony_id_counter(), 0, "no key generation requests");
		assert_eq!(BitcoinThresholdSigner::ceremony_id_counter(), 0, "no key generation requests");

		assert_eq!(
			pallet_cf_environment::EthereumSignatureNonce::<Runtime>::get(),
			0,
			"Global signature nonce should be 0"
		);

		assert!(Governance::members().contains(&AccountId::from(ERIN)), "expected governor");
		assert_eq!(Governance::proposal_id_counter(), 0, "no proposal for governance");

		assert_eq!(
			Emissions::current_authority_emission_inflation(),
			CURRENT_AUTHORITY_EMISSION_INFLATION_PERBILL,
			"invalid emission inflation for authorities"
		);

		assert_eq!(
			Emissions::backup_node_emission_inflation(),
			BACKUP_NODE_EMISSION_INFLATION_PERBILL,
			"invalid emission inflation for backup authorities"
		);

		for account in &accounts {
			assert_eq!(
				Reputation::reputation(account),
				pallet_cf_reputation::ReputationTracker::<Runtime>::default(),
				"authority shouldn't have reputation points"
			);
		}

		for account in &accounts {
			assert_eq!(
				pallet_cf_account_roles::AccountRoles::<Runtime>::get(account).unwrap(),
				AccountRole::Validator
			);
		}

		for account in accounts.iter() {
			// TODO: Check historical epochs
			assert_eq!(ChainflipAccountState::CurrentAuthority, get_validator_state(account));
		}
	});
}
