use frame_support::sp_io::TestExternalities;
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_finality_grandpa::AuthorityId as GrandpaId;
use sp_runtime::{traits::Zero, BuildStorage};
use state_chain_runtime::{
	constants::common::*, opaque::SessionKeys, AccountId, AccountRolesConfig, EmissionsConfig,
	EthereumVaultConfig, FlipConfig, GovernanceConfig, ReputationConfig, Runtime, SessionConfig,
	StakingConfig, System, ValidatorConfig,
};

pub const CURRENT_AUTHORITY_EMISSION_INFLATION_PERBILL: u32 = 28;
pub const BACKUP_NODE_EMISSION_INFLATION_PERBILL: u32 = 6;
pub const CLAIM_DELAY_BUFFER_SECS: u64 = 10;

use crate::{get_from_seed, network, GENESIS_KEY};
use cf_primitives::{AccountRole, AuthorityCount};
use cf_traits::{BlockNumber, FlipBalance};

pub struct ExtBuilder {
	pub accounts: Vec<(AccountId, FlipBalance)>,
	root: Option<AccountId>,
	blocks_per_epoch: BlockNumber,
	max_authorities: AuthorityCount,
	min_authorities: AuthorityCount,
}

impl Default for ExtBuilder {
	fn default() -> Self {
		Self {
			accounts: vec![],
			root: None,
			blocks_per_epoch: Zero::zero(),
			max_authorities: MAX_AUTHORITIES,
			min_authorities: 1,
		}
	}
}

impl ExtBuilder {
	pub fn accounts(mut self, accounts: Vec<(AccountId, FlipBalance)>) -> Self {
		self.accounts = accounts;
		self
	}

	pub fn root(mut self, root: AccountId) -> Self {
		self.root = Some(root);
		self
	}

	pub fn blocks_per_epoch(mut self, blocks_per_epoch: BlockNumber) -> Self {
		self.blocks_per_epoch = blocks_per_epoch;
		self
	}

	pub fn min_authorities(mut self, min_authorities: AuthorityCount) -> Self {
		self.min_authorities = min_authorities;
		self
	}

	pub fn max_authorities(mut self, max_authorities: AuthorityCount) -> Self {
		self.max_authorities = max_authorities;
		self
	}

	/// Default ext configuration with BlockNumber 1
	pub fn build(&self) -> TestExternalities {
		let mut storage =
			frame_system::GenesisConfig::default().build_storage::<Runtime>().unwrap();

		let (_, public_key, _) = network::ThresholdSigner::generate_keypair(GENESIS_KEY);
		let ethereum_vault_key = public_key.serialize_compressed().to_vec();

		state_chain_runtime::GenesisConfig {
			session: SessionConfig {
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
			},
			flip: FlipConfig { total_issuance: TOTAL_ISSUANCE },
			staking: StakingConfig {
				genesis_stakers: self.accounts.clone(),
				minimum_stake: DEFAULT_MIN_STAKE,
				claim_ttl: core::time::Duration::from_secs(3 * CLAIM_DELAY_SECS),
				claim_delay_buffer_seconds: CLAIM_DELAY_BUFFER_SECS,
			},
			reputation: ReputationConfig {
				accrual_ratio: ACCRUAL_RATIO,
				penalties: PENALTIES.to_vec(),
				genesis_nodes: self.accounts.iter().map(|(id, _)| id.clone()).collect(),
			},
			governance: GovernanceConfig {
				members: self.root.iter().cloned().collect(),
				expiry_span: EXPIRY_SPAN_IN_SECONDS,
			},
			validator: ValidatorConfig {
				genesis_authorities: self.accounts.iter().map(|(id, _)| id.clone()).collect(),
				genesis_backups: Default::default(),
				blocks_per_epoch: self.blocks_per_epoch,
				bond: self.accounts.iter().map(|(_, stake)| *stake).min().unwrap(),
				claim_period_as_percentage: PERCENT_OF_EPOCH_PERIOD_CLAIMABLE,
				backup_reward_node_percentage: 34,
				authority_set_min_size: self.min_authorities,
				min_size: self.min_authorities,
				max_size: self.max_authorities,
				max_expansion: self.max_authorities,
			},
			ethereum_vault: EthereumVaultConfig {
				vault_key: ethereum_vault_key,
				deployment_block: 0,
				keygen_response_timeout: 4,
			},
			emissions: EmissionsConfig {
				current_authority_emission_inflation: CURRENT_AUTHORITY_EMISSION_INFLATION_PERBILL,
				backup_node_emission_inflation: BACKUP_NODE_EMISSION_INFLATION_PERBILL,
				supply_update_interval: SUPPLY_UPDATE_INTERVAL_DEFAULT,
			},
			account_roles: AccountRolesConfig {
				initial_account_roles: self
					.accounts
					.iter()
					.map(|(id, _)| (id.clone(), AccountRole::Validator))
					.collect(),
			},
			..state_chain_runtime::GenesisConfig::default()
		}
		.assimilate_storage(&mut storage)
		.unwrap();

		let mut ext = TestExternalities::from(storage);

		// Ensure we emit the events (no events emitted at block 0)
		ext.execute_with(|| System::set_block_number(1));

		ext
	}
}
