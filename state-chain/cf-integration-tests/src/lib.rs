#![cfg(test)]

mod network;

use frame_support::{assert_noop, assert_ok, sp_io::TestExternalities, traits::OnInitialize};
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_core::crypto::{Pair, Public};
use sp_finality_grandpa::AuthorityId as GrandpaId;
use sp_runtime::{traits::Zero, BuildStorage};
use state_chain_runtime::{
	chainflip::Offence, constants::common::*, opaque::SessionKeys, AccountId, AuctionConfig,
	Emissions, EmissionsConfig, EthereumVaultConfig, Flip, FlipConfig, Governance,
	GovernanceConfig, Origin, Reputation, ReputationConfig, Runtime, SessionConfig, Staking,
	StakingConfig, System, Timestamp, Validator, ValidatorConfig, Witnesser,
};

use cf_traits::{AuthorityCount, BlockNumber, EpochIndex, FlipBalance};
use libsecp256k1::SecretKey;
use pallet_cf_staking::{EthTransactionHash, EthereumAddress};
use rand::{prelude::*, SeedableRng};
use sp_runtime::AccountId32;

type NodeId = AccountId32;
const ETH_DUMMY_ADDR: EthereumAddress = [42u8; 20];
const ETH_ZERO_ADDRESS: EthereumAddress = [0xff; 20];
const TX_HASH: EthTransactionHash = [211u8; 32];

pub const GENESIS_KEY: u64 = 42;

// TODO - remove collision of account numbers
pub const ALICE: [u8; 32] = [0xaa; 32];
pub const BOB: [u8; 32] = [0xbb; 32];
pub const CHARLIE: [u8; 32] = [0xcc; 32];
// Root and Gov member
pub const ERIN: [u8; 32] = [0xee; 32];

const GENESIS_EPOCH: EpochIndex = 1;

pub fn get_from_seed<TPublic: Public>(seed: &str) -> <TPublic::Pair as Pair>::Public {
	TPublic::Pair::from_string(&format!("//{}", seed), None)
		.expect("static values are valid; qed")
		.public()
}

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
	fn accounts(mut self, accounts: Vec<(AccountId, FlipBalance)>) -> Self {
		self.accounts = accounts;
		self
	}

	fn root(mut self, root: AccountId) -> Self {
		self.root = Some(root);
		self
	}

	fn blocks_per_epoch(mut self, blocks_per_epoch: BlockNumber) -> Self {
		self.blocks_per_epoch = blocks_per_epoch;
		self
	}

	fn min_authorities(mut self, min_authorities: AuthorityCount) -> Self {
		self.min_authorities = min_authorities;
		self
	}

	fn max_authorities(mut self, max_authorities: AuthorityCount) -> Self {
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
				claim_ttl: core::time::Duration::from_secs(3 * CLAIM_DELAY),
			},
			auction: AuctionConfig {
				min_size: self.min_authorities,
				max_size: self.max_authorities,
				max_expansion: self.max_authorities,
			},
			reputation: ReputationConfig {
				accrual_ratio: ACCRUAL_RATIO,
				penalties: vec![(Offence::MissedHeartbeat, (15, 150))],
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
				authority_set_min_size: self.min_authorities as u8,
			},
			ethereum_vault: EthereumVaultConfig {
				vault_key: ethereum_vault_key,
				deployment_block: 0,
				keygen_response_timeout: 4,
			},
			emissions: EmissionsConfig {
				current_authority_emission_inflation: CURRENT_AUTHORITY_EMISSION_INFLATION_BPS,
				backup_node_emission_inflation: BACKUP_NODE_EMISSION_INFLATION_BPS,
				supply_update_interval: SUPPLY_UPDATE_INTERVAL_DEFAULT,
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

mod genesis {
	use sp_std::collections::btree_set::BTreeSet;

	use super::*;
	use cf_traits::{
		ChainflipAccount, ChainflipAccountState, ChainflipAccountStore, EpochInfo, QualifyNode,
		StakeTransfer,
	};
	pub const GENESIS_BALANCE: FlipBalance = TOTAL_ISSUANCE / 100;

	pub fn default() -> ExtBuilder {
		ExtBuilder::default()
			.accounts(vec![
				(AccountId::from(ALICE), GENESIS_BALANCE),
				(AccountId::from(BOB), GENESIS_BALANCE),
				(AccountId::from(CHARLIE), GENESIS_BALANCE),
			])
			.root(AccountId::from(ERIN))
	}

	#[test]
	fn state_of_genesis_is_as_expected() {
		default().build().execute_with(|| {
			// Confirmation that we have our assumed state at block 1
			assert_eq!(Flip::total_issuance(), TOTAL_ISSUANCE, "we have issued the total issuance");

			let accounts = [AccountId::from(CHARLIE), AccountId::from(BOB), AccountId::from(ALICE)];

			for account in accounts.iter() {
				assert_eq!(
					Flip::staked_balance(account),
					GENESIS_BALANCE,
					"the account has its stake"
				);
			}

			assert_eq!(Validator::bond(), GENESIS_BALANCE);
			assert_eq!(
				Validator::current_authorities().iter().collect::<BTreeSet<_>>(),
				accounts.iter().collect::<BTreeSet<_>>(),
				"the validators are those expected at genesis"
			);

			assert_eq!(
				Validator::blocks_per_epoch(),
				0,
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

			assert_eq!(Validator::ceremony_id_counter(), 0, "no key generation requests");

			assert_eq!(
				pallet_cf_environment::GlobalSignatureNonce::<Runtime>::get(),
				0,
				"Global signature nonce should be 0"
			);

			assert!(Governance::members().contains(&AccountId::from(ERIN)), "expected governor");
			assert_eq!(Governance::proposal_id_counter(), 0, "no proposal for governance");

			assert_eq!(
				Emissions::current_authority_emission_inflation(),
				CURRENT_AUTHORITY_EMISSION_INFLATION_BPS,
				"invalid emission inflation for authorities"
			);

			assert_eq!(
				Emissions::backup_node_emission_inflation(),
				BACKUP_NODE_EMISSION_INFLATION_BPS,
				"invalid emission inflation for backup authorities"
			);

			for account in accounts.iter() {
				assert_eq!(
					Reputation::reputation(account),
					pallet_cf_reputation::ReputationTracker::<Runtime>::default(),
					"authority shouldn't have reputation points"
				);
			}

			for account in accounts.iter() {
				let account_data = ChainflipAccountStore::<Runtime>::get(account);
				// TODO: Check historical epochs
				assert_eq!(ChainflipAccountState::CurrentAuthority, account_data.state);
			}
		});
	}
}

// The minimum number of blocks a vault rotation should last
const VAULT_ROTATION_BLOCKS: BlockNumber = 6;

mod epoch {
	use std::collections::BTreeSet;

	use super::*;
	use crate::{genesis::GENESIS_BALANCE, network::Network};
	use cf_traits::{
		BidderProvider, ChainflipAccount, ChainflipAccountState, ChainflipAccountStore, EpochInfo,
	};
	use pallet_cf_validator::RotationPhase;
	use state_chain_runtime::Validator;

	#[test]
	fn auction_repeats_after_failure_because_of_liveness() {
		const EPOCH_BLOCKS: BlockNumber = 1000;
		super::genesis::default()
			.blocks_per_epoch(EPOCH_BLOCKS)
			// As we run a rotation at genesis we will need accounts to support
			// having 5 authorities as the default is 3 (Alice, Bob and Charlie)
			.accounts(vec![
				(AccountId::from(ALICE), GENESIS_BALANCE),
				(AccountId::from(BOB), GENESIS_BALANCE),
				(AccountId::from(CHARLIE), GENESIS_BALANCE),
				(AccountId::from([0xfc; 32]), GENESIS_BALANCE),
				(AccountId::from([0xfb; 32]), GENESIS_BALANCE),
			])
			.min_authorities(5)
			.build()
			.execute_with(|| {
				let mut nodes = Validator::current_authorities();
				let (mut testnet, mut backup_nodes) = network::Network::create(3, &nodes);

				nodes.append(&mut backup_nodes);

				// All nodes stake to be included in the next epoch which are witnessed on the
				// state chain
				for node in &nodes {
					testnet.stake_manager_contract.stake(
						node.clone(),
						genesis::GENESIS_BALANCE + 1,
						GENESIS_EPOCH,
					);
				}

				// Set the first 4 nodes offline
				let offline_nodes: Vec<_> = nodes.iter().take(4).cloned().collect();

				for node in &offline_nodes {
					testnet.set_active(node, false);
					pallet_cf_reputation::LastHeartbeat::<Runtime>::remove(node);
				}

				// Run to the next epoch to start the auction
				testnet.move_to_next_epoch();

				assert!(
					matches!(Validator::current_rotation_phase(), RotationPhase::Idle),
					"Expected RotationPhase::Idle, got: {:?}.",
					Validator::current_rotation_phase(),
				);

				// Next block, no progress.
				testnet.move_forward_blocks(1);

				assert!(
					matches!(Validator::current_rotation_phase(), RotationPhase::Idle),
					"Expected RotationPhase::Idle, got: {:?}.",
					Validator::current_rotation_phase(),
				);

				for node in &offline_nodes {
					testnet.set_active(node, true);
				}

				// Submit a heartbeat, for all the nodes. Given we were waiting for the nodes to
				// come online to start the rotation, the rotation ought to start on the next
				// block
				testnet.submit_heartbeat_all_engines();
				testnet.move_forward_blocks(1);

				assert_eq!(GENESIS_EPOCH, Validator::epoch_index());

				// We are still rotating, we have not completed a rotation
				assert!(
					matches!(
						Validator::current_rotation_phase(),
						RotationPhase::VaultsRotating { .. }
					),
					"Expected RotationPhase::VaultsRotating, got: {:?}.",
					Validator::current_rotation_phase(),
				);
			});
	}

	#[test]
	// An epoch has completed.  We have a genesis where the blocks per epoch are
	// set to 100
	// - When the epoch is reached an auction is started and completed
	// - All nodes stake above the MAB
	// - We have two nodes that haven't registered their session keys
	// - New authorities have the state of Validator with the last active epoch stored
	// - Nodes without keys state remain unqualified as a backup with `None` as their last active
	//   epoch
	fn epoch_rotates() {
		const EPOCH_BLOCKS: BlockNumber = 1000;
		const MAX_SET_SIZE: AuthorityCount = 5;
		super::genesis::default()
			.blocks_per_epoch(EPOCH_BLOCKS)
			.min_authorities(MAX_SET_SIZE)
			.build()
			.execute_with(|| {
				// Genesis nodes
				let genesis_nodes = Validator::current_authorities();

				let number_of_backup_nodes = MAX_SET_SIZE
					.checked_sub(genesis_nodes.len() as AuthorityCount)
					.expect("Max set size must be at least the number of genesis authorities");

				let (mut testnet, backup_nodes) =
					network::Network::create(number_of_backup_nodes as u8, &genesis_nodes);

				assert_eq!(testnet.live_nodes().len() as AuthorityCount, MAX_SET_SIZE);
				// All nodes stake to be included in the next epoch which are witnessed on the
				// state chain
				let stake_amount = genesis::GENESIS_BALANCE + 1;
				for node in &testnet.live_nodes() {
					testnet.stake_manager_contract.stake(node.clone(), stake_amount, GENESIS_EPOCH);
				}

				// Add two nodes which don't have session keys
				let keyless_nodes = vec![testnet.create_engine(), testnet.create_engine()];
				// Our keyless nodes also stake
				for keyless_node in &keyless_nodes {
					testnet.stake_manager_contract.stake(
						keyless_node.clone(),
						stake_amount,
						GENESIS_EPOCH,
					);
				}

				// A late staker which we will use after the auction.  They are yet to stake
				// and will do after the auction with the intention of being a backup node
				let late_staker = testnet.create_engine();
				testnet.set_active(&late_staker, true);

				// Move forward one block to register the stakes on-chain.
				testnet.move_forward_blocks(1);

				for node in &backup_nodes {
					network::setup_account_and_peer_mapping(node);
					network::Cli::activate_account(node);
				}
				for node in &keyless_nodes {
					network::setup_peer_mapping(node);
					network::Cli::activate_account(node);
				}

				testnet.move_to_next_epoch();
				testnet.submit_heartbeat_all_engines();
				testnet.move_forward_blocks(1);

				assert!(matches!(
					Validator::current_rotation_phase(),
					RotationPhase::VaultsRotating(..)
				));

				testnet.move_forward_blocks(VAULT_ROTATION_BLOCKS);

				assert_eq!(
					GENESIS_EPOCH + 1,
					Validator::epoch_index(),
					"We should be in the next epoch. {:?}",
					Staking::get_bidders()
				);

				assert_eq!(
					Validator::bond(),
					stake_amount,
					"minimum active bid should be that of the new stake"
				);

				assert_eq!(
						Validator::current_authorities().iter().collect::<BTreeSet<_>>(),
						[genesis_nodes, backup_nodes].concat().iter().collect::<BTreeSet<_>>(),
						"the new winners should be those genesis authorities and the backup nodes that have keys set"
					);

				for account in keyless_nodes.iter() {
					// TODO: Check historical epochs
					assert_eq!(
						ChainflipAccountState::Backup,
						ChainflipAccountStore::<Runtime>::get(account).state,
						"should be a backup node"
					);
				}

				for account in &Validator::current_authorities() {
					// TODO: Check historical epochs
					assert_eq!(
						ChainflipAccountState::CurrentAuthority,
						ChainflipAccountStore::<Runtime>::get(account).state,
						"should be CurrentAuthority"
					);
				}

				// A late staker comes along, they should become a backup node as long as they
				// are sufficiently staked and have
				testnet.stake_manager_contract.stake(
					late_staker.clone(),
					stake_amount,
					GENESIS_EPOCH + 1,
				);

				// Register the stake.
				testnet.move_forward_blocks(1);

				assert_eq!(
					ChainflipAccountState::Backup,
					ChainflipAccountStore::<Runtime>::get(&late_staker).state,
					"late staker should be a backup node"
				);
			});
	}

	#[test]
	// When an epoch expires, purge stale storages in the Witnesser pallet.
	// This is done through ChainflipEpochTransitions.
	fn new_epoch_will_purges_stale_witnesser_storage() {
		const EPOCH_BLOCKS: BlockNumber = 100;
		const MAX_AUTHORITIES: AuthorityCount = 3;
		super::genesis::default()
			.blocks_per_epoch(EPOCH_BLOCKS)
			.min_authorities(MAX_AUTHORITIES)
			.build()
			.execute_with(|| {
				// Get the list of nodes
				let mut nodes = Validator::current_authorities();
				let (mut testnet, mut backup_nodes) = network::Network::create(0, &nodes);

				for backup_node in backup_nodes.clone() {
					network::Cli::activate_account(&backup_node);
				}

				nodes.append(&mut backup_nodes);
				let stake_amount = FLIPPERINOS_PER_FLIP;

				// Moving into epoch 1
				for node in &nodes {
					testnet.stake_manager_contract.stake(node.clone(), stake_amount, 1);
				}

				testnet.move_forward_blocks(VAULT_ROTATION_BLOCKS + 1);
				assert_eq!(Validator::epoch_index(), 1);

				// Move forward a few more epochs
				let move_forward_by_epochs = |epochs: u32, testnet: &mut Network| {
					let start = Validator::epoch_index();
					let finish = start + epochs;
					for epoch in start..finish {
						testnet.move_forward_blocks(EPOCH_BLOCKS + VAULT_ROTATION_BLOCKS + 1);
						for node in &nodes {
							testnet.stake_manager_contract.stake(node.clone(), stake_amount, epoch + 1);
						}
						testnet.submit_heartbeat_all_engines();
					}
				};

				move_forward_by_epochs(3, &mut testnet);
				

				assert_eq!(Validator::epoch_index(), 4);
				assert_eq!(Validator::last_expired_epoch(), 2);

				// Create dummy call and call hash
				let call =
					Box::new(state_chain_runtime::Call::System(frame_system::Call::remark {
						remark: vec![],
					}));
				let call_hash =
					pallet_cf_witnesser::CallHash(frame_support::Hashable::blake2_256(&*call));
				
				// Add the dummy call into storage	 
				let validators = Validator::current_authorities();
				for node in &validators {
					assert_ok!(Witnesser::witness_at_epoch(
						Origin::signed(node.clone()),
						call.clone(),
						4
					));
				}
				pallet_cf_witnesser::ExtraCallData::<Runtime>::insert(
					4,
					&call_hash,
					vec![vec![0u8]],
				);

				// Execute the call after voting has passed.
				testnet.move_forward_blocks(1);

				// Votes are registered in storage.
				assert_eq!(
					pallet_cf_witnesser::Votes::<Runtime>::get(4, &call_hash),
					Some(vec![224])
				);
				assert_eq!(
					pallet_cf_witnesser::ExtraCallData::<Runtime>::get(4, &call_hash),
					Some(vec![vec![0u8]])
				);
				assert_eq!(
					pallet_cf_witnesser::CallHashExecuted::<Runtime>::get(4, &call_hash),
					Some(())
				);

				// Move forward in time until Epoch 4 is expired.
				move_forward_by_epochs(2, &mut testnet);

				assert_eq!(Validator::epoch_index(), 6);
				assert_eq!(Validator::last_expired_epoch(), 4);

				// Test that the storage has been purged.
				assert_eq!(pallet_cf_witnesser::Votes::<Runtime>::get(4, &call_hash), None);
				assert_eq!(pallet_cf_witnesser::ExtraCallData::<Runtime>::get(4, &call_hash), None);
				assert_eq!(
					pallet_cf_witnesser::CallHashExecuted::<Runtime>::get(4, &call_hash),
					None
				);
			});
	}
}

mod staking {
	use crate::{
		genesis::GENESIS_BALANCE,
		network::{create_testnet_with_new_staker, NEW_STAKE_AMOUNT},
	};

	use super::{genesis, network, *};
	use cf_traits::EpochInfo;
	use pallet_cf_staking::pallet::Error;
	use pallet_cf_validator::Backups;
	#[test]
	// Stakers cannot claim when we are out of the claiming period (50% of the epoch)
	// We have a set of nodes that are staked and can claim in the claiming period and
	// not claim when out of the period
	fn cannot_claim_stake_out_of_claim_period() {
		const EPOCH_BLOCKS: u32 = 100;
		const MAX_AUTHORITIES: AuthorityCount = 3;
		super::genesis::default()
			.blocks_per_epoch(EPOCH_BLOCKS)
			.max_authorities(MAX_AUTHORITIES)
			.build()
			.execute_with(|| {
				let mut nodes = Validator::current_authorities();
				let (mut testnet, mut backup_nodes) = network::Network::create(0, &nodes);

				for backup_node in backup_nodes.clone() {
					network::Cli::activate_account(&backup_node);
				}

				nodes.append(&mut backup_nodes);

				// Stake these nodes so that they are included in the next epoch
				let stake_amount = genesis::GENESIS_BALANCE;
				for node in &nodes {
					testnet.stake_manager_contract.stake(node.clone(), stake_amount, GENESIS_EPOCH);
				}

				// Move forward one block to process events
				testnet.move_forward_blocks(1);

				assert_eq!(
					GENESIS_EPOCH,
					Validator::epoch_index(),
					"We should be in the genesis epoch"
				);

				// We should be able to claim stake out of an auction
				for node in &nodes {
					assert_ok!(Staking::claim(
						Origin::signed(node.clone()),
						1.into(),
						ETH_DUMMY_ADDR
					));
				}

				let end_of_claim_period =
					EPOCH_BLOCKS * PERCENT_OF_EPOCH_PERIOD_CLAIMABLE as u32 / 100;
				// Move to end of the claim period
				System::set_block_number(end_of_claim_period + 1);
				// We will try to claim some stake
				for node in &nodes {
					assert_noop!(
						Staking::claim(
							Origin::signed(node.clone()),
							stake_amount.into(),
							ETH_DUMMY_ADDR
						),
						Error::<Runtime>::AuctionPhase
					);
				}

				assert_eq!(1, Validator::epoch_index(), "We should still be in the first epoch");

				// Move to new epoch
				testnet.move_to_next_epoch();
				// Run things to a successful vault rotation
				testnet.move_forward_blocks(VAULT_ROTATION_BLOCKS);

				assert_eq!(2, Validator::epoch_index(), "We are in a new epoch");

				// We should be able to claim again outside of the auction
				// At the moment we have a pending claim so we would expect an error here for
				// this.
				// TODO implement Claims in Contract/Network
				for node in &nodes {
					assert_noop!(
						Staking::claim(Origin::signed(node.clone()), 1.into(), ETH_DUMMY_ADDR),
						Error::<Runtime>::PendingClaim
					);
				}
			});
	}

	#[test]
	fn staked_node_is_added_to_backups() {
		const EPOCH_BLOCKS: u32 = 10_000_000;
		super::genesis::default()
			.blocks_per_epoch(EPOCH_BLOCKS)
			// As we run a rotation at genesis we will need accounts to support
			// having 5 authorities as the default is 3 (Alice, Bob and Charlie)
			.accounts(vec![
				(AccountId::from(ALICE), GENESIS_BALANCE),
				(AccountId::from(BOB), GENESIS_BALANCE),
				(AccountId::from(CHARLIE), GENESIS_BALANCE),
			])
			.min_authorities(3)
			.build()
			.execute_with(|| {
				let (_, new_backup) = create_testnet_with_new_staker();
				let backups_map = Backups::<Runtime>::get();
				assert_eq!(backups_map.len(), 1);
				assert_eq!(backups_map.get(&new_backup).unwrap(), &NEW_STAKE_AMOUNT);
			});
	}
}

mod runtime {
	use super::*;
	use frame_support::dispatch::GetDispatchInfo;
	use pallet_cf_flip::FlipTransactionPayment;
	use pallet_transaction_payment::OnChargeTransaction;

	#[test]
	// We have two types of accounts. One set of accounts which is part
	// of the governance and is allowed to make free calls to governance extrinsic.
	// All other accounts are normally charged and can call any extrinsic.
	fn restriction_handling() {
		super::genesis::default().build().execute_with(|| {
			let call: state_chain_runtime::Call =
				frame_system::Call::remark { remark: vec![] }.into();
			let gov_call: state_chain_runtime::Call =
				pallet_cf_governance::Call::approve { id: 1 }.into();
			// Expect a successful normal call to work
			let ordinary = FlipTransactionPayment::<Runtime>::withdraw_fee(
				&ALICE.into(),
				&call,
				&call.get_dispatch_info(),
				5,
				0,
			);
			assert!(ordinary.expect("we have a result").is_some(), "expected Some(Surplus)");
			// Expect a successful gov call to work
			let gov = FlipTransactionPayment::<Runtime>::withdraw_fee(
				&ERIN.into(),
				&gov_call,
				&gov_call.get_dispatch_info(),
				5000,
				0,
			);
			assert!(gov.expect("we have a result").is_none(), "expected None");
			// Expect a non gov call to fail when it's executed by gov member
			let gov_err = FlipTransactionPayment::<Runtime>::withdraw_fee(
				&ERIN.into(),
				&call,
				&call.get_dispatch_info(),
				5000,
				0,
			);
			assert!(gov_err.is_err(), "expected an error");
		});
	}
}

mod authorities {
	use crate::{
		genesis, network, NodeId, GENESIS_EPOCH, HEARTBEAT_BLOCK_INTERVAL, VAULT_ROTATION_BLOCKS,
	};
	use cf_traits::{
		AuthorityCount, ChainflipAccount, ChainflipAccountState, ChainflipAccountStore, EpochInfo,
		FlipBalance, StakeTransfer,
	};
	use sp_runtime::AccountId32;
	use state_chain_runtime::{Flip, Runtime, Validator};
	use std::collections::HashMap;

	#[test]
	fn authorities_earn_rewards_for_authoring_blocks() {
		// We want to have at least one heartbeat within our reduced epoch
		const EPOCH_BLOCKS: u32 = 1000;
		// Reduce our validating set and hence the number of nodes we need to have a backup
		// set
		const MAX_AUTHORITIES: AuthorityCount = 3;
		super::genesis::default()
			.blocks_per_epoch(EPOCH_BLOCKS)
			.max_authorities(MAX_AUTHORITIES)
			.build()
			.execute_with(|| {
				let genesis_authorities = Validator::current_authorities();
				let (mut testnet, _) = network::Network::create(0, &genesis_authorities);

				let staked_amounts = || {
					genesis_authorities
						.iter()
						.map(|id| (id.clone(), Flip::staked_balance(id)))
						.collect()
				};

				let staked_amounts_before: Vec<(AccountId32, u128)> = staked_amounts();

				// each authority should author a block and mint FLIP to themselves
				testnet.move_forward_blocks(MAX_AUTHORITIES);

				// Each node should have more rewards now than before, since they've each authored a
				// block
				let staked_amounts_after = staked_amounts();

				// Ensure all nodes have increased the same amount
				let first_node_stake = staked_amounts_after.first().unwrap().1;
				staked_amounts_after.iter().all(|(_node, amount)| amount == &first_node_stake);

				// Ensure all nodes have a higher stake than before
				staked_amounts_before.into_iter().zip(staked_amounts_after).for_each(
					|((_node, amount_before), (_node2, amount_after))| {
						assert!(amount_before < amount_after)
					},
				);
			});
	}

	#[test]
	fn genesis_nodes_rotated_out_accumulate_rewards_correctly() {
		// We want to have at least one heartbeat within our reduced epoch
		const EPOCH_BLOCKS: u32 = 1000;
		// Reduce our validating set and hence the number of nodes we need to have a backup
		// set
		const MAX_AUTHORITIES: AuthorityCount = 10;
		super::genesis::default()
			.blocks_per_epoch(EPOCH_BLOCKS)
			.max_authorities(MAX_AUTHORITIES)
			.build()
			.execute_with(|| {
				// Create MAX_AUTHORITIES backup nodes and stake them above our genesis
				// authorities The result will be our newly created nodes will be authorities
				// and the genesis authorities will become backup nodes
				let mut genesis_authorities = Validator::current_authorities();
				let (mut testnet, mut init_backup_nodes) =
					network::Network::create(MAX_AUTHORITIES as u8, &genesis_authorities);

				// An initial stake which is greater than the genesis stakes
				// We intend for these initially backup nodes to win the auction
				const INITIAL_STAKE: FlipBalance = genesis::GENESIS_BALANCE * 2;
				// Stake these backup nodes so that they are included in the next epoch
				for node in &init_backup_nodes {
					testnet.stake_manager_contract.stake(
						node.clone(),
						INITIAL_STAKE,
						GENESIS_EPOCH,
					);
				}

				// Allow the stakes to be registered, then initialise the account keys and peer
				// ids.
				testnet.move_forward_blocks(1);

				for node in &init_backup_nodes {
					network::setup_account_and_peer_mapping(node);
					network::Cli::activate_account(node);
				}

				// Start an auction
				testnet.move_to_next_epoch();
				testnet.submit_heartbeat_all_engines();
				testnet.move_forward_blocks(1);

				assert_eq!(
					GENESIS_EPOCH,
					Validator::epoch_index(),
					"We should still be in the genesis epoch"
				);

				testnet.move_forward_blocks(VAULT_ROTATION_BLOCKS);
				assert_eq!(
					GENESIS_EPOCH + 1,
					Validator::epoch_index(),
					"We should be in a new epoch"
				);

				// assert list of authorities as being the new nodes
				let mut current_authorities: Vec<NodeId> = Validator::current_authorities();

				current_authorities.sort();
				init_backup_nodes.sort();

				assert_eq!(
					init_backup_nodes, current_authorities,
					"our new initial backup nodes should be the new authorities"
				);

				current_authorities.iter().for_each(|account_id| {
					let account_data = ChainflipAccountStore::<Runtime>::get(account_id);
					assert_eq!(account_data.state, ChainflipAccountState::CurrentAuthority);
					// we were active in teh first epoch

					// TODO: Check historical epochs
				});

				// assert list of backup validators as being the genesis authorities
				let mut highest_staked_backup_nodes: Vec<NodeId> =
					Validator::highest_staked_qualified_backup_nodes_lookup().into_iter().collect();

				highest_staked_backup_nodes.sort();
				genesis_authorities.sort();

				assert_eq!(
					genesis_authorities, highest_staked_backup_nodes,
					"the genesis authorities should now be the backup nodes"
				);

				highest_staked_backup_nodes.iter().for_each(|account_id| {
					let account_data = ChainflipAccountStore::<Runtime>::get(account_id);
					// we were active in the first epoch
					assert_eq!(account_data.state, ChainflipAccountState::HistoricalAuthority);
					// TODO: Check historical epochs
				});

				let backup_node_balances: HashMap<NodeId, FlipBalance> =
					highest_staked_backup_nodes
						.iter()
						.map(|validator_id| {
							(validator_id.clone(), Flip::staked_balance(validator_id))
						})
						.collect::<Vec<(NodeId, FlipBalance)>>()
						.into_iter()
						.collect();

				// Move forward a heartbeat, emissions should be shared to backup nodes
				testnet.move_forward_blocks(HEARTBEAT_BLOCK_INTERVAL);

				// We won't calculate the exact emissions but they should be greater than their
				// initial stake
				for (backup_node, pre_balance) in backup_node_balances {
					assert!(pre_balance < Flip::staked_balance(&backup_node));
				}
			});
	}
}
