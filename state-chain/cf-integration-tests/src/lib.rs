#![cfg(test)]

mod network;

mod signer_nomination;

mod mock_runtime;

mod authorities;

mod staking;

use frame_support::{assert_noop, assert_ok, traits::OnInitialize};
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_core::crypto::{Pair, Public};
use sp_finality_grandpa::AuthorityId as GrandpaId;
use state_chain_runtime::{
	constants::common::*, opaque::SessionKeys, AccountId, Emissions, Flip, Governance, Origin,
	Reputation, Runtime, Staking, System, Timestamp, Validator, Witnesser,
};

use cf_primitives::{AuthorityCount, EpochIndex};
use cf_traits::{BlockNumber, FlipBalance};
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

mod genesis {
	use sp_std::collections::btree_set::BTreeSet;

	use crate::mock_runtime::ExtBuilder;

	use super::*;
	use cf_primitives::ChainflipAccountState;
	use cf_traits::{
		ChainflipAccount, ChainflipAccountStore, EpochInfo, QualifyNode, StakeTransfer,
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
				CURRENT_AUTHORITY_EMISSION_INFLATION_PERBILL,
				"invalid emission inflation for authorities"
			);

			assert_eq!(
				Emissions::backup_node_emission_inflation(),
				BACKUP_NODE_EMISSION_INFLATION_PERBILL,
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
	use cf_primitives::ChainflipAccountState;
	use cf_traits::{ChainflipAccount, ChainflipAccountStore, EpochInfo};
	use frame_support::traits::Hooks;
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
					"We should be in the next epoch."
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
	/// When an epoch expires, purge stale storages in the Witnesser pallet.
	/// This is done through ChainflipEpochTransitions.
	fn new_epoch_will_purge_stale_witnesser_storage() {
		const EPOCH_BLOCKS: BlockNumber = 100;
		const MAX_AUTHORITIES: AuthorityCount = 3;
		let storage_epoch = 4;
		let mut ext = super::genesis::default()
			.blocks_per_epoch(EPOCH_BLOCKS)
			.min_authorities(MAX_AUTHORITIES)
			.build();

		ext.execute_with(|| {
			let mut nodes = Validator::current_authorities();
			nodes.sort();
			let (mut testnet, _) = network::Network::create(0, &nodes);

			assert_eq!(Validator::epoch_index(), 1);

			let move_forward_by_epochs = |epochs: u32, testnet: &mut Network| {
				let start = Validator::epoch_index();
				let finish = start + epochs;
				for _ in start..finish {
					testnet.move_forward_blocks(EPOCH_BLOCKS + VAULT_ROTATION_BLOCKS + 1);
					testnet.submit_heartbeat_all_engines();
				}
			};

			move_forward_by_epochs(3, &mut testnet);
			assert_eq!(Validator::epoch_index(), 4);
			assert_eq!(Validator::last_expired_epoch(), 2);
			let mut current_authorities_after_some_epochs = Validator::current_authorities();
			current_authorities_after_some_epochs.sort();
			assert_eq!(nodes, current_authorities_after_some_epochs);

			let call = Box::new(state_chain_runtime::Call::System(frame_system::Call::remark {
				remark: vec![],
			}));
			let call_hash =
				pallet_cf_witnesser::CallHash(frame_support::Hashable::blake2_256(&*call));

			for node in &nodes {
				assert_ok!(Witnesser::witness_at_epoch(
					Origin::signed(node.clone()),
					call.clone(),
					storage_epoch
				));
			}
			pallet_cf_witnesser::ExtraCallData::<Runtime>::insert(
				storage_epoch,
				&call_hash,
				vec![vec![0u8]],
			);

			// Execute the call after voting has passed.
			testnet.move_forward_blocks(1);

			// Ensure Votes and calldata are registered in storage.
			assert!(pallet_cf_witnesser::Votes::<Runtime>::get(storage_epoch, &call_hash).is_some());
			assert!(pallet_cf_witnesser::ExtraCallData::<Runtime>::get(storage_epoch, &call_hash)
				.is_some());
			assert!(pallet_cf_witnesser::CallHashExecuted::<Runtime>::get(
				storage_epoch,
				&call_hash
			)
			.is_some());

			// Move forward in time until Epoch 4 is expired.
			move_forward_by_epochs(2, &mut testnet);

			assert_eq!(Validator::epoch_index(), 6);
			assert_eq!(Validator::last_expired_epoch(), storage_epoch);
		});

		// Commit Overlay changeset into the backend DB, to fully test clear_prefix logic.
		// See: /state-chain/TROUBLESHOOTING.md
		// Section: ## Substrate storage: Separation of front overlay and backend. Feat
		// clear_prefix()
		let _res = ext.commit_all();

		ext.execute_with(|| {
			let call = Box::new(state_chain_runtime::Call::System(frame_system::Call::remark {
				remark: vec![],
			}));
			let call_hash =
				pallet_cf_witnesser::CallHash(frame_support::Hashable::blake2_256(&*call));

			// Call on_idle to purge stale storage
			Witnesser::on_idle(0, 1_000_000_000_000);

			// Test that the storage has been purged.
			assert!(pallet_cf_witnesser::Votes::<Runtime>::get(storage_epoch, &call_hash).is_none());
			assert!(pallet_cf_witnesser::ExtraCallData::<Runtime>::get(storage_epoch, &call_hash)
				.is_none());
			assert!(pallet_cf_witnesser::CallHashExecuted::<Runtime>::get(
				storage_epoch,
				&call_hash
			)
			.is_none());
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
