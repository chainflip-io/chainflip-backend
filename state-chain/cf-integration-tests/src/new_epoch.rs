use super::*;
use crate::genesis::GENESIS_BALANCE;
use cf_chains::btc::{
	deposit_address::DepositAddress, utxo_selection::ConsolidationParameters, BtcAmount, Utxo,
	UtxoId, CHANGE_ADDRESS_SALT,
};
use cf_primitives::{AccountRole, GENESIS_EPOCH};
use cf_traits::{EpochInfo, KeyProvider};
use frame_support::traits::UnfilteredDispatchable;
use pallet_cf_environment::BitcoinAvailableUtxos;
use pallet_cf_validator::RotationPhase;
use state_chain_runtime::{BitcoinThresholdSigner, Environment, RuntimeEvent, Validator};

#[test]
fn auction_repeats_after_failure_because_of_liveness() {
	const EPOCH_BLOCKS: BlockNumber = 1000;
	super::genesis::with_test_defaults()
		.blocks_per_epoch(EPOCH_BLOCKS)
		// As we run a rotation at genesis we will need accounts to support
		// having 5 authorities as the default is 3 (Alice, Bob and Charlie)
		.accounts(vec![
			(AccountId::from(ALICE), AccountRole::Validator, GENESIS_BALANCE),
			(AccountId::from(BOB), AccountRole::Validator, GENESIS_BALANCE),
			(AccountId::from(CHARLIE), AccountRole::Validator, GENESIS_BALANCE),
			(AccountId::from([0xfc; 32]), AccountRole::Validator, GENESIS_BALANCE),
			(AccountId::from([0xfb; 32]), AccountRole::Validator, GENESIS_BALANCE),
		])
		.min_authorities(5)
		.build()
		.execute_with(|| {
			let mut nodes = Validator::current_authorities();
			let (mut testnet, mut backup_nodes) = network::Network::create(3, &nodes);

			nodes.append(&mut backup_nodes);

			// All nodes add funds to be included in the next epoch which are witnessed on the
			// state chain
			for node in &nodes {
				testnet.state_chain_gateway_contract.fund_account(
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
			testnet.set_auto_heartbeat_all_nodes(false);

			// Run to the next epoch to start the auction
			testnet.move_to_the_end_of_epoch();

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

			testnet.set_active_all_nodes(true);

			// Submit a heartbeat, for all the nodes. Given we were waiting for the nodes to
			// come online to start the rotation, the rotation ought to start on the next
			// block
			testnet.submit_heartbeat_all_engines(true);
			testnet.move_forward_blocks(1);

			assert_eq!(GENESIS_EPOCH, Validator::epoch_index());

			// We are still rotating, we have not completed a rotation
			assert!(
				matches!(
					Validator::current_rotation_phase(),
					RotationPhase::KeygensInProgress { .. }
				),
				"Expected RotationPhase::KeygensInProgress, got: {:?}.",
				Validator::current_rotation_phase(),
			);
		});
}

#[test]
// An epoch has completed.  We have a genesis where the blocks per epoch are set to 100
// - When the epoch is reached an auction is started and completed
// - All nodes add funds above the MAB
// - We have two nodes that haven't registered their session keys
// - New authorities have the state of Validator with the last active epoch stored
// - Nodes without keys state remain unqualified as a backup with `None` as their last active epoch
fn epoch_rotates() {
	const EPOCH_BLOCKS: BlockNumber = 1000;
	const MAX_SET_SIZE: AuthorityCount = 5;
	super::genesis::with_test_defaults()
		.blocks_per_epoch(EPOCH_BLOCKS)
		.min_authorities(MAX_SET_SIZE)
		.build()
		.execute_with(|| {
			let mut genesis_nodes = Validator::current_authorities();

			let number_of_backup_nodes = MAX_SET_SIZE
				.checked_sub(genesis_nodes.len() as AuthorityCount)
				.expect("Max set size must be at least the number of genesis authorities");

			let (mut testnet, mut backup_nodes) =
				network::Network::create(number_of_backup_nodes as u8, &genesis_nodes);

			assert_eq!(testnet.live_nodes().len() as AuthorityCount, MAX_SET_SIZE);
			// All nodes add funds to be included in the next epoch which are witnessed on the
			// state chain
			let funding_amount = genesis::GENESIS_BALANCE + 1;
			for node in &testnet.live_nodes() {
				testnet.state_chain_gateway_contract.fund_account(
					node.clone(),
					funding_amount,
					GENESIS_EPOCH,
				);
			}

			// Add two nodes which don't have session keys
			let keyless_nodes = vec![testnet.create_engine(), testnet.create_engine()];
			// Our keyless nodes also add funds
			for keyless_node in &keyless_nodes {
				testnet.state_chain_gateway_contract.fund_account(
					keyless_node.clone(),
					funding_amount,
					GENESIS_EPOCH,
				);
			}

			// A late funder which we will use after the auction.  They are yet to add funds
			// and will do after the auction with the intention of being a backup node
			let late_funder = testnet.create_engine();
			testnet.set_active(&late_funder, true);

			// Move forward one block to register the funds on-chain.
			testnet.move_forward_blocks(1);

			for node in &backup_nodes {
				network::Cli::register_as_validator(node);
				network::setup_account_and_peer_mapping(node);
				network::Cli::start_bidding(node);
			}
			for node in &keyless_nodes {
				network::Cli::register_as_validator(node);
				network::setup_peer_mapping(node);
				network::Cli::start_bidding(node);
			}

			testnet.move_to_the_end_of_epoch();
			testnet.move_forward_blocks(1);

			assert!(matches!(
				Validator::current_rotation_phase(),
				RotationPhase::KeygensInProgress(..)
			));

			testnet.move_forward_blocks(VAULT_ROTATION_BLOCKS);

			assert!(matches!(Validator::current_rotation_phase(), RotationPhase::Idle));

			assert_eq!(
				GENESIS_EPOCH + 1,
				Validator::epoch_index(),
				"We should be in the next epoch."
			);

			assert_eq!(
				Validator::bond(),
				funding_amount,
				"minimum active bid should be the balance of the latest funder"
			);

			genesis_nodes.append(&mut backup_nodes);
			assert_eq!(
						Validator::current_authorities(),
						genesis_nodes,
						"the new winners should be those genesis authorities and the backup nodes that have keys set"
					);

			for account in keyless_nodes.iter() {
				// TODO: Check historical epochs
				assert!(
					matches!(get_validator_state(account), ChainflipAccountState::Backup,),
					"should be a backup node"
				);
			}

			for account in &Validator::current_authorities() {
				// TODO: Check historical epochs
				assert_eq!(
					ChainflipAccountState::CurrentAuthority,
					get_validator_state(account),
					"should be CurrentAuthority"
				);
			}

			// A late funder comes along, they should become a backup node as long as they
			// are sufficiently funded and have
			testnet.state_chain_gateway_contract.fund_account(
				late_funder.clone(),
				funding_amount,
				GENESIS_EPOCH + 1,
			);

			// Register the new funds.
			testnet.move_forward_blocks(1);

			assert_eq!(
				ChainflipAccountState::Backup,
				get_validator_state(&late_funder),
				"late funder should be a backup node"
			);
		});
}

fn utxo(amount: BtcAmount, salt: u32, pub_key: Option<[u8; 32]>) -> Utxo {
	Utxo {
		amount,
		id: Default::default(),
		deposit_address: DepositAddress::new(pub_key.unwrap_or_default(), salt),
	}
}

fn add_utxo_amount(utxo: Utxo) {
	Environment::add_bitcoin_utxo_to_list(utxo.amount, utxo.id, utxo.deposit_address);
}

#[test]
fn bitcoin_utxos_are_sent_to_current_vault_or_discarded() {
	const EPOCH_BLOCKS: BlockNumber = 100;
	const MAX_AUTHORITIES: AuthorityCount = 5;
	super::genesis::with_test_defaults()
		.blocks_per_epoch(EPOCH_BLOCKS)
		.build()
		.execute_with(|| {
			let (mut testnet, _, _) =
				crate::network::fund_authorities_and_join_auction(MAX_AUTHORITIES);

			testnet.move_to_the_next_epoch();
			testnet.move_to_the_next_epoch();
			assert_eq!(Validator::current_epoch(), 3);

			let (epoch_2, epoch_3) = if let Some(cf_traits::EpochKey {
				key: cf_chains::btc::AggKey { previous: Some(prev_key), current },
				..
			}) = BitcoinThresholdSigner::active_epoch_key()
			{
				(prev_key, current)
			} else {
				unreachable!("Bitcoin vault key for epoch 0 and 1 must exist.");
			};

			// 	update_consolidation_parameters
			assert_ok!(RuntimeCall::Environment(
				pallet_cf_environment::Call::update_consolidation_parameters {
					params: ConsolidationParameters {
						consolidation_threshold: 10,
						consolidation_size: 2,
					}
				}
			)
			.clone()
			.dispatch_bypass_filter(pallet_cf_governance::RawOrigin::GovernanceApproval.into()));

			// Add some bitcoin utxos.
			add_utxo_amount(utxo(31_000_000, CHANGE_ADDRESS_SALT, Some(epoch_3)));
			add_utxo_amount(utxo(32_000_000, CHANGE_ADDRESS_SALT, Some(epoch_3)));
			add_utxo_amount(utxo(33_000_000, CHANGE_ADDRESS_SALT, Some(epoch_3)));
			add_utxo_amount(utxo(34_000_000, CHANGE_ADDRESS_SALT, Some(epoch_3)));

			testnet.move_forward_blocks(1);
			// Nothing changes.
			assert_eq!(BitcoinAvailableUtxos::<Runtime>::decode_len(), Some(4));

			// Add utxos from previous epoch
			add_utxo_amount(utxo(21_000_000, CHANGE_ADDRESS_SALT, Some(epoch_2)));
			add_utxo_amount(utxo(22_000_000, CHANGE_ADDRESS_SALT, Some(epoch_2)));
			add_utxo_amount(utxo(23_000_000, CHANGE_ADDRESS_SALT, Some(epoch_2)));
			add_utxo_amount(utxo(24_000_000, CHANGE_ADDRESS_SALT, Some(epoch_2)));

			testnet.move_forward_blocks(1);

			// 2 utxos from previous vault are sent to the current vault.
			assert_eq!(
				BitcoinAvailableUtxos::<Runtime>::get(),
				vec![
					utxo(31_000_000, CHANGE_ADDRESS_SALT, Some(epoch_3)),
					utxo(32_000_000, CHANGE_ADDRESS_SALT, Some(epoch_3)),
					utxo(33_000_000, CHANGE_ADDRESS_SALT, Some(epoch_3)),
					utxo(34_000_000, CHANGE_ADDRESS_SALT, Some(epoch_3)),
					utxo(23_000_000, CHANGE_ADDRESS_SALT, Some(epoch_2)),
					utxo(24_000_000, CHANGE_ADDRESS_SALT, Some(epoch_2)),
				]
			);

			System::assert_has_event(RuntimeEvent::Environment(
				pallet_cf_environment::Event::UtxoTransferred {
					broadcast_id: 1,
					total_amount: 42_999_817,
				},
			));

			// These are discarded
			add_utxo_amount(utxo(1_000_000, 0, None));
			add_utxo_amount(utxo(2_000_000, 0, None));
			add_utxo_amount(utxo(3_000_000, 0, None));

			testnet.move_forward_blocks(1);

			// Last epoch's utxo has been transferred to the current vault
			assert_eq!(
				BitcoinAvailableUtxos::<Runtime>::get(),
				vec![
					utxo(31_000_000, CHANGE_ADDRESS_SALT, Some(epoch_3)),
					utxo(32_000_000, CHANGE_ADDRESS_SALT, Some(epoch_3)),
					utxo(33_000_000, CHANGE_ADDRESS_SALT, Some(epoch_3)),
					utxo(34_000_000, CHANGE_ADDRESS_SALT, Some(epoch_3)),
					Utxo {
						id: UtxoId {
							tx_id: hex_literal::hex!(
								"d7ee4b2c95f67a0454a3c4e9774c057075e649100284cf62a4b8c6f3925a1d26"
							)
							.into(),
							vout: 0,
						},
						amount: 42_999_817,
						deposit_address: DepositAddress { pubkey_x: epoch_3, script_path: None }
					},
				]
			);
			System::assert_has_event(RuntimeEvent::Environment(
				pallet_cf_environment::Event::UtxoTransferred {
					broadcast_id: 2,
					total_amount: 46_999_817,
				},
			));
			System::assert_has_event(RuntimeEvent::Environment(
				pallet_cf_environment::Event::StaleUtxoDiscarded {
					utxos: vec![
						utxo(1_000_000, 0, None),
						utxo(2_000_000, 0, None),
						utxo(3_000_000, 0, None),
					],
				},
			));

			testnet.move_forward_blocks(1);

			assert_eq!(
				BitcoinAvailableUtxos::<Runtime>::get(),
				vec![
					utxo(31_000_000, CHANGE_ADDRESS_SALT, Some(epoch_3)),
					utxo(32_000_000, CHANGE_ADDRESS_SALT, Some(epoch_3)),
					utxo(33_000_000, CHANGE_ADDRESS_SALT, Some(epoch_3)),
					utxo(34_000_000, CHANGE_ADDRESS_SALT, Some(epoch_3)),
					Utxo {
						id: UtxoId {
							tx_id: hex_literal::hex!(
								"d7ee4b2c95f67a0454a3c4e9774c057075e649100284cf62a4b8c6f3925a1d26"
							)
							.into(),
							vout: 0,
						},
						amount: 42_999_817,
						deposit_address: DepositAddress { pubkey_x: epoch_3, script_path: None }
					},
					Utxo {
						id: UtxoId {
							tx_id: hex_literal::hex!(
								"5edf11df7cec1b6957e2ed603b2e93a72b5cb3c6e3e7b7094e54ccc77a4fe8d7"
							)
							.into(),
							vout: 0,
						},
						amount: 46_999_817,
						deposit_address: DepositAddress { pubkey_x: epoch_3, script_path: None }
					},
				]
			);
		});
}
