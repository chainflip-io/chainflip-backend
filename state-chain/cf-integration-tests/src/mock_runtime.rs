// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

use cf_chains::{
	arb::ArbitrumTrackedData,
	btc::{BitcoinFeeInfo, BitcoinTrackedData},
	dot::{PolkadotTrackedData, RuntimeVersion},
	eth::EthereumTrackedData,
	hub::AssethubTrackedData,
	sol::{sol_tx_core::sol_test_values, SolTrackedData},
	Arbitrum, Assethub, Bitcoin, ChainState, Ethereum, Polkadot, Solana,
};
use chainflip_node::{
	chain_spec::testnet::{EXPIRY_SPAN_IN_SECONDS, REDEMPTION_TTL_SECS},
	test_account_from_seed,
};
use pallet_cf_elections::{
	electoral_systems::blockchain::delta_based_ingress::BackoffSettings, InitialState,
};
use pallet_cf_validator::SetSizeParameters;
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_consensus_grandpa::AuthorityId as GrandpaId;
use sp_core::H160;
use sp_runtime::{Percent, Permill};
use state_chain_runtime::{
	chainflip::{
		solana_elections::{SolanaIngressSettings, SolanaVaultSwapsSettings},
		Offence,
	},
	constants::common::*,
	opaque::SessionKeys,
	test_runner::*,
	AccountId, AccountRolesConfig, ArbitrumChainTrackingConfig, AssethubChainTrackingConfig,
	BitcoinChainTrackingConfig, BitcoinElectionsConfig, EmissionsConfig, EnvironmentConfig,
	EthereumChainTrackingConfig, EthereumVaultConfig, EvmThresholdSignerConfig, FlipConfig,
	FundingConfig, GenericElectionsConfig, GovernanceConfig, PolkadotChainTrackingConfig,
	ReputationConfig, SessionConfig, SolanaChainTrackingConfig, SolanaElectionsConfig,
	ValidatorConfig,
};

pub const CURRENT_AUTHORITY_EMISSION_INFLATION_PERBILL: u32 = 28;
pub const BACKUP_NODE_EMISSION_INFLATION_PERBILL: u32 = 6;
pub const SUPPLY_UPDATE_INTERVAL_DEFAULT: u32 = 14_400;
pub const MIN_FUNDING: FlipBalance = 10 * FLIPPERINOS_PER_FLIP;

pub const ACCRUAL_RATIO: (i32, u32) = (1, 1);

const COMPUTE_PRICE: u64 = 1_000u64;

const BLOCKS_BETWEEN_LIVENESS_CHECKS: u32 = 10;

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
	(Offence::FailedLivenessCheck(cf_chains::ForeignChain::Solana), (4, 0)),
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
	epoch_duration: BlockNumber,
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
			epoch_duration: Default::default(),
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

	pub fn epoch_duration(mut self, epoch_duration: BlockNumber) -> Self {
		self.epoch_duration = epoch_duration;
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
				genesis_accounts: self
					.genesis_accounts
					.iter()
					.map(|(id, _role, amount)| (id.clone(), *amount))
					.collect::<Vec<_>>(),
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
				epoch_duration: self.epoch_duration,
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
			ethereum_vault: EthereumVaultConfig {
				deployment_block: Some(0),
				chain_initialized: true,
			},

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
				genesis_vanity_names: Default::default(),
			},
			ethereum_chain_tracking: EthereumChainTrackingConfig {
				init_chain_state: ChainState::<Ethereum> {
					block_height: 0,
					tracked_data: EthereumTrackedData {
						base_fee: 100000u32.into(),
						priority_fee: 1u32.into(),
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
			assethub_chain_tracking: AssethubChainTrackingConfig {
				init_chain_state: ChainState::<Assethub> {
					block_height: 0,
					tracked_data: AssethubTrackedData {
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
			arbitrum_chain_tracking: ArbitrumChainTrackingConfig {
				init_chain_state: ChainState::<Arbitrum> {
					block_height: 0,
					tracked_data: ArbitrumTrackedData {
						base_fee: 100000u32.into(),
						l1_base_fee_estimate: 1u128,
					},
				},
			},
			solana_chain_tracking: SolanaChainTrackingConfig {
				init_chain_state: ChainState::<Solana> {
					block_height: 0,
					tracked_data: SolTrackedData { priority_fee: COMPUTE_PRICE },
				},
			},
			bitcoin_threshold_signer: Default::default(),
			evm_threshold_signer: EvmThresholdSignerConfig {
				key: Some(ethereum_vault_key),
				keygen_response_timeout: 4,
				threshold_signature_response_timeout: 4,
				amount_to_slash: FLIPPERINOS_PER_FLIP,
				_instance: std::marker::PhantomData,
			},
			environment: EnvironmentConfig {
				sol_durable_nonces_and_accounts: vec![
					(Default::default(), Default::default()),
					(Default::default(), Default::default()),
					(Default::default(), Default::default()),
					(Default::default(), Default::default()),
				],
				// Exact values not important, but should be different from each other.
				eth_key_manager_address: H160::repeat_byte(0x01),
				eth_vault_address: H160::repeat_byte(0x02),
				state_chain_gateway_address: H160::repeat_byte(0x03),
				flip_token_address: H160::repeat_byte(0x04),
				..Default::default()
			},
			polkadot_threshold_signer: Default::default(),
			solana_threshold_signer: Default::default(),
			bitcoin_vault: Default::default(),
			polkadot_vault: Default::default(),
			assethub_vault: Default::default(),
			arbitrum_vault: Default::default(),
			solana_vault: Default::default(),
			swapping: Default::default(),
			system: Default::default(),
			transaction_payment: Default::default(),
			bitcoin_ingress_egress: Default::default(),
			polkadot_ingress_egress: Default::default(),
			assethub_ingress_egress: Default::default(),
			ethereum_ingress_egress: Default::default(),
			arbitrum_ingress_egress: Default::default(),
			solana_ingress_egress: Default::default(),
			solana_elections: SolanaElectionsConfig {
				option_initial_state: Some(InitialState {
					unsynchronised_state: (
						/* chain tracking */ Default::default(),
						(),
						(),
						(),
						(),
						Default::default(),
						(),
					),
					unsynchronised_settings: ((), (), (), (), (), (), ()),
					settings: (
						(),
						(
							SolanaIngressSettings {
								vault_program: sol_test_values::VAULT_PROGRAM,
								usdc_token_mint_pubkey: sol_test_values::USDC_TOKEN_MINT_PUB_KEY,
							},
							BackoffSettings { backoff_after_blocks: 600, backoff_frequency: 100 },
						),
						(),
						(),
						BLOCKS_BETWEEN_LIVENESS_CHECKS,
						SolanaVaultSwapsSettings {
							swap_endpoint_data_account_address:
								sol_test_values::SWAP_ENDPOINT_DATA_ACCOUNT_ADDRESS,
							usdc_token_mint_pubkey: sol_test_values::USDC_TOKEN_MINT_PUB_KEY,
						},
						(),
					),
					shared_data_reference_lifetime: Default::default(),
				}),
			},
			bitcoin_elections: BitcoinElectionsConfig { option_initial_state: None },
			generic_elections: GenericElectionsConfig { option_initial_state: None },
			ethereum_broadcaster: state_chain_runtime::EthereumBroadcasterConfig {
				broadcast_timeout: 5 * BLOCKS_PER_MINUTE_ETHEREUM,
			},
			polkadot_broadcaster: state_chain_runtime::PolkadotBroadcasterConfig {
				broadcast_timeout: 4 * BLOCKS_PER_MINUTE_POLKADOT,
			},
			assethub_broadcaster: state_chain_runtime::AssethubBroadcasterConfig {
				broadcast_timeout: 4 * BLOCKS_PER_MINUTE_ASSETHUB,
			},
			bitcoin_broadcaster: state_chain_runtime::BitcoinBroadcasterConfig {
				broadcast_timeout: 9, // = 90 minutes
			},
			arbitrum_broadcaster: state_chain_runtime::ArbitrumBroadcasterConfig {
				broadcast_timeout: 2 * BLOCKS_PER_MINUTE_ARBITRUM,
			},
			solana_broadcaster: state_chain_runtime::SolanaBroadcasterConfig {
				broadcast_timeout: 4 * BLOCKS_PER_MINUTE_SOLANA,
			},
		})
	}
}
