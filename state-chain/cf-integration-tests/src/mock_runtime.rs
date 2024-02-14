use chainflip_node::{
	chain_spec::testnet::{EXPIRY_SPAN_IN_SECONDS, REDEMPTION_TTL_SECS},
	test_account_from_seed,
};
use pallet_cf_validator::SetSizeParameters;
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_consensus_grandpa::AuthorityId as GrandpaId;
use sp_runtime::{Percent, Permill};
use state_chain_runtime::{
	chainflip::Offence, constants::common::*, opaque::SessionKeys, test_runner::*, AccountId,
	AccountRolesConfig, EmissionsConfig, EthereumThresholdSignerConfig, EthereumVaultConfig,
	FlipConfig, FundingConfig, GovernanceConfig, ReputationConfig, SessionConfig, ValidatorConfig,
};

use cf_chains::{
	btc::{BitcoinFeeInfo, BitcoinTrackedData},
	dot::{PolkadotTrackedData, RuntimeVersion},
	eth::EthereumTrackedData,
	Bitcoin, ChainState, Ethereum, Polkadot,
};
use state_chain_runtime::{
	BitcoinChainTrackingConfig, EthereumChainTrackingConfig, PolkadotChainTrackingConfig,
};

pub const CURRENT_AUTHORITY_EMISSION_INFLATION_PERBILL: u32 = 28;
pub const BACKUP_NODE_EMISSION_INFLATION_PERBILL: u32 = 6;
pub const SUPPLY_UPDATE_INTERVAL_DEFAULT: u32 = 14_400;
pub const MIN_FUNDING: FlipBalance = 10 * FLIPPERINOS_PER_FLIP;

pub const ACCRUAL_RATIO: (i32, u32) = (1, 1);

/// The offences committable within the protocol and their respective reputation penalty and
/// suspension durations.
pub const PENALTIES: &[(Offence, (i32, BlockNumber))] = &[
	(Offence::ParticipateKeygenFailed, (15, HEARTBEAT_BLOCK_INTERVAL)),
	(Offence::ParticipateSigningFailed, (15, HEARTBEAT_BLOCK_INTERVAL)),
	(Offence::MissedAuthorshipSlot, (15, HEARTBEAT_BLOCK_INTERVAL)),
	(Offence::MissedHeartbeat, (15, HEARTBEAT_BLOCK_INTERVAL)),
	// We exclude them from the nomination pool of the next attempt,
	// so there is no need to suspend them further.
	(Offence::FailedToBroadcastTransaction, (10, 0)),
	(Offence::GrandpaEquivocation, (50, HEARTBEAT_BLOCK_INTERVAL * 5)),
];

use crate::{
	threshold_signing::{EthKeyComponents, KeyUtils},
	GENESIS_KEY_SEED,
};
use cf_primitives::{
	AccountRole, AuthorityCount, BlockNumber, FlipBalance, DEFAULT_MAX_AUTHORITY_SET_CONTRACTION,
	GENESIS_EPOCH,
};

pub struct ExtBuilder {
	pub genesis_accounts: Vec<(AccountId, AccountRole, FlipBalance)>,
	root: Option<AccountId>,
	blocks_per_epoch: BlockNumber,
	max_authorities: AuthorityCount,
	min_authorities: AuthorityCount,
}

impl Default for ExtBuilder {
	fn default() -> Self {
		Self {
			max_authorities: MAX_AUTHORITIES,
			min_authorities: 1,
			genesis_accounts: Default::default(),
			root: Default::default(),
			blocks_per_epoch: Default::default(),
		}
	}
}

impl ExtBuilder {
	pub fn accounts(mut self, accounts: Vec<(AccountId, AccountRole, FlipBalance)>) -> Self {
		self.genesis_accounts = accounts;
		self
	}

	pub fn with_additional_accounts(
		mut self,
		accounts: &[(AccountId, AccountRole, FlipBalance)],
	) -> Self {
		self.genesis_accounts.extend_from_slice(accounts);
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
	pub fn build(&self) -> TestRunner<()> {
		let key_components = EthKeyComponents::generate(GENESIS_KEY_SEED, GENESIS_EPOCH);
		let ethereum_vault_key = key_components.agg_key();

		TestRunner::<()>::new(state_chain_runtime::RuntimeGenesisConfig {
			// These are set indirectly via the session pallet.
			aura: Default::default(),
			// These are set indirectly via the session pallet.
			grandpa: Default::default(),
			session: SessionConfig {
				keys: self
					.genesis_accounts
					.iter()
					.map(|x| {
						(
							x.0.clone(),
							x.0.clone(),
							SessionKeys {
								aura: test_account_from_seed::<AuraId>(&x.0.clone().to_string()),
								grandpa: test_account_from_seed::<GrandpaId>(
									&x.0.clone().to_string(),
								),
							},
						)
					})
					.collect::<Vec<_>>(),
			},
			flip: FlipConfig {
				total_issuance: TOTAL_ISSUANCE,
				daily_slashing_rate: Permill::from_perthousand(1),
			},
			funding: FundingConfig {
				genesis_accounts: self.genesis_accounts.clone(),
				redemption_tax: MIN_FUNDING / 2,
				minimum_funding: MIN_FUNDING,
				redemption_ttl: core::time::Duration::from_secs(REDEMPTION_TTL_SECS),
			},
			reputation: ReputationConfig {
				accrual_ratio: ACCRUAL_RATIO,
				penalties: PENALTIES.to_vec(),
				genesis_validators: self
					.genesis_accounts
					.iter()
					.filter_map(|(id, role, ..)| {
						matches!(role, AccountRole::Validator).then_some(id.clone())
					})
					.collect(),
			},
			governance: GovernanceConfig {
				members: self.root.iter().cloned().collect(),
				expiry_span: EXPIRY_SPAN_IN_SECONDS,
			},
			validator: ValidatorConfig {
				genesis_authorities: self
					.genesis_accounts
					.iter()
					.filter_map(|(id, role, ..)| {
						matches!(role, AccountRole::Validator).then_some(id.clone())
					})
					.collect(),
				genesis_backups: Default::default(),
				genesis_vanity_names: Default::default(),
				blocks_per_epoch: self.blocks_per_epoch,
				bond: self
					.genesis_accounts
					.iter()
					.filter_map(|(.., role, amount)| {
						matches!(role, AccountRole::Validator).then_some(*amount)
					})
					.min()
					.unwrap(),
				redemption_period_as_percentage: Percent::from_percent(
					REDEMPTION_PERIOD_AS_PERCENTAGE,
				),
				backup_reward_node_percentage: Percent::from_percent(34),
				authority_set_min_size: self.min_authorities,
				auction_parameters: SetSizeParameters {
					min_size: self.min_authorities,
					max_size: self.max_authorities,
					max_expansion: self.max_authorities,
				},
				auction_bid_cutoff_percentage: Percent::from_percent(0),
				max_authority_set_contraction_percentage: DEFAULT_MAX_AUTHORITY_SET_CONTRACTION,
			},
			ethereum_vault: EthereumVaultConfig { deployment_block: Some(0) },
			emissions: EmissionsConfig {
				current_authority_emission_inflation: CURRENT_AUTHORITY_EMISSION_INFLATION_PERBILL,
				backup_node_emission_inflation: BACKUP_NODE_EMISSION_INFLATION_PERBILL,
				supply_update_interval: SUPPLY_UPDATE_INTERVAL_DEFAULT,
				..Default::default()
			},
			account_roles: AccountRolesConfig {
				initial_account_roles: self
					.genesis_accounts
					.iter()
					.map(|(id, role, _)| (id.clone(), *role))
					.collect(),
			},
			ethereum_chain_tracking: EthereumChainTrackingConfig {
				init_chain_state: ChainState::<Ethereum> {
					block_height: 0,
					tracked_data: EthereumTrackedData {
						base_fee: 0u32.into(),
						priority_fee: 0u32.into(),
					},
				},
			},
			polkadot_chain_tracking: PolkadotChainTrackingConfig {
				init_chain_state: ChainState::<Polkadot> {
					block_height: 0,
					tracked_data: PolkadotTrackedData {
						median_tip: 0,
						runtime_version: RuntimeVersion {
							spec_version: 17,
							transaction_version: 17,
						},
					},
				},
			},
			bitcoin_chain_tracking: BitcoinChainTrackingConfig {
				init_chain_state: ChainState::<Bitcoin> {
					block_height: 0,
					tracked_data: BitcoinTrackedData { btc_fee_info: BitcoinFeeInfo::new(0) },
				},
			},
			bitcoin_threshold_signer: Default::default(),
			ethereum_threshold_signer: EthereumThresholdSignerConfig {
				key: Some(ethereum_vault_key),
				keygen_response_timeout: 4,
				threshold_signature_response_timeout: 4,
				amount_to_slash: FLIPPERINOS_PER_FLIP,
				_instance: std::marker::PhantomData,
			},
			polkadot_threshold_signer: Default::default(),
			bitcoin_vault: Default::default(),
			polkadot_vault: Default::default(),
			environment: Default::default(),
			liquidity_pools: Default::default(),
			system: Default::default(),
			transaction_payment: Default::default(),
			bitcoin_ingress_egress: Default::default(),
			polkadot_ingress_egress: Default::default(),
			ethereum_ingress_egress: Default::default(),
		})
	}
}
