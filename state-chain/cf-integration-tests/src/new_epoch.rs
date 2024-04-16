use std::collections::BTreeSet;

use super::*;
use crate::genesis::GENESIS_BALANCE;
use cf_chains::btc::{
	deposit_address::DepositAddress, utxo_selection::ConsolidationParameters, BitcoinFeeInfo,
	BtcAmount, Utxo, UtxoId, CHANGE_ADDRESS_SALT,
};
use cf_primitives::{AccountRole, GENESIS_EPOCH};
use cf_traits::{EpochInfo, KeyProvider};
use frame_support::traits::UnfilteredDispatchable;
use pallet_cf_environment::BitcoinAvailableUtxos;
use pallet_cf_validator::RotationPhase;
use state_chain_runtime::{
	BitcoinInstance, BitcoinThresholdSigner, Environment, RuntimeEvent, Validator,
};

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
						Validator::current_authorities().into_iter().collect::<BTreeSet<AccountId32>>(),
						genesis_nodes.into_iter().collect::<BTreeSet<AccountId32>>(),
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
fn can_consolidate_bitcoin_utxos() {
	const EPOCH_BLOCKS: BlockNumber = 100;
	const MAX_AUTHORITIES: AuthorityCount = 5;
	const CONSOLIDATION_SIZE: u32 = 2;
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
						consolidation_threshold: 5,
						consolidation_size: CONSOLIDATION_SIZE,
					}
				}
			)
			.clone()
			.dispatch_bypass_filter(pallet_cf_governance::RawOrigin::GovernanceApproval.into()));

			let bitcoin_fee_info: BitcoinFeeInfo =
				pallet_cf_chain_tracking::Pallet::<Runtime, BitcoinInstance>::chain_state()
					.expect("There should always be a chain state.")
					.tracked_data
					.btc_fee_info;
			let expected_consolidation_fee = bitcoin_fee_info.min_fee_required_per_tx() // base fee
				+ CONSOLIDATION_SIZE as BtcAmount * bitcoin_fee_info.fee_per_input_utxo() // consolidation inputs
				+ bitcoin_fee_info.fee_per_output_utxo(); // change output

			// Add some bitcoin utxos.
			add_utxo_amount(utxo(31_000_000, CHANGE_ADDRESS_SALT, Some(epoch_3)));
			add_utxo_amount(utxo(32_000_000, CHANGE_ADDRESS_SALT, Some(epoch_3)));
			add_utxo_amount(utxo(33_000_000, CHANGE_ADDRESS_SALT, Some(epoch_3)));
			add_utxo_amount(utxo(34_000_000, CHANGE_ADDRESS_SALT, Some(epoch_3)));

			testnet.move_forward_blocks(1);

			// Nothing changes.
			assert_eq!(BitcoinAvailableUtxos::<Runtime>::decode_len(), Some(4));

			// Add utxos
			add_utxo_amount(utxo(21_000_000, CHANGE_ADDRESS_SALT, Some(epoch_2)));
			add_utxo_amount(utxo(22_000_000, CHANGE_ADDRESS_SALT, Some(epoch_2)));
			add_utxo_amount(utxo(23_000_000, CHANGE_ADDRESS_SALT, Some(epoch_2)));
			add_utxo_amount(utxo(35_000_000, CHANGE_ADDRESS_SALT, Some(epoch_3)));

			testnet.move_forward_blocks(1);

			// Consolidate 2 utxos.
			assert_eq!(
				BitcoinAvailableUtxos::<Runtime>::get(),
				vec![
					utxo(33_000_000, CHANGE_ADDRESS_SALT, Some(epoch_3)),
					utxo(34_000_000, CHANGE_ADDRESS_SALT, Some(epoch_3)),
					utxo(21_000_000, CHANGE_ADDRESS_SALT, Some(epoch_2)),
					utxo(22_000_000, CHANGE_ADDRESS_SALT, Some(epoch_2)),
					utxo(23_000_000, CHANGE_ADDRESS_SALT, Some(epoch_2)),
					utxo(35_000_000, CHANGE_ADDRESS_SALT, Some(epoch_3)),
				]
			);

			// These are discarded
			add_utxo_amount(utxo(1_000_000, 0, None));
			add_utxo_amount(utxo(2_000_000, 0, None));
			add_utxo_amount(utxo(3_000_000, 0, None));

			testnet.move_forward_blocks(1);

			assert_eq!(
				BitcoinAvailableUtxos::<Runtime>::get(),
				vec![
					utxo(21_000_000, CHANGE_ADDRESS_SALT, Some(epoch_2)),
					utxo(22_000_000, CHANGE_ADDRESS_SALT, Some(epoch_2)),
					utxo(23_000_000, CHANGE_ADDRESS_SALT, Some(epoch_2)),
					utxo(35_000_000, CHANGE_ADDRESS_SALT, Some(epoch_3)),
					Utxo {
						id: UtxoId {
							tx_id: hex_literal::hex!(
								"583c4f12102fc4bea1af47d14f9d0b90fca0783af1c5d03d894a44be5ae165bc"
							)
							.into(),
							vout: 0,
						},
						amount: 31_000_000 + 32_000_000 - expected_consolidation_fee,
						deposit_address: DepositAddress { pubkey_x: epoch_3, script_path: None }
					},
				]
			);
			System::assert_has_event(RuntimeEvent::Environment(
				pallet_cf_environment::Event::StaleUtxosDiscarded {
					utxos: vec![
						utxo(1_000_000, 0, None),
						utxo(2_000_000, 0, None),
						utxo(3_000_000, 0, None),
					],
				},
			));

			testnet.move_forward_blocks(1);

			// 2 more utxos has been consolidated.
			assert_eq!(
				BitcoinAvailableUtxos::<Runtime>::get(),
				vec![
					utxo(23_000_000, CHANGE_ADDRESS_SALT, Some(epoch_2)),
					utxo(35_000_000, CHANGE_ADDRESS_SALT, Some(epoch_3)),
					Utxo {
						id: UtxoId {
							tx_id: hex_literal::hex!(
								"583c4f12102fc4bea1af47d14f9d0b90fca0783af1c5d03d894a44be5ae165bc"
							)
							.into(),
							vout: 0,
						},
						amount: 31_000_000 + 32_000_000 - expected_consolidation_fee,
						deposit_address: DepositAddress { pubkey_x: epoch_3, script_path: None }
					},
					Utxo {
						id: UtxoId {
							tx_id: hex_literal::hex!(
								"7412bc12c3e68a975c910f998afb9ac4b7de1426474642f2067727183dfe6c26"
							)
							.into(),
							vout: 0,
						},
						amount: 33_000_000 + 34_000_000 - expected_consolidation_fee,
						deposit_address: DepositAddress { pubkey_x: epoch_3, script_path: None }
					},
				]
			);

			// Increase the threshold so only previous utxos are sent
			assert_ok!(RuntimeCall::Environment(
				pallet_cf_environment::Call::update_consolidation_parameters {
					params: ConsolidationParameters {
						consolidation_threshold: 10,
						consolidation_size: 5,
					}
				}
			)
			.clone()
			.dispatch_bypass_filter(pallet_cf_governance::RawOrigin::GovernanceApproval.into()));

			testnet.move_forward_blocks(1);

			// Only 1 utxo from epoch 2 is consolidated
			assert_eq!(
				BitcoinAvailableUtxos::<Runtime>::get(),
				vec![
					utxo(35_000_000, CHANGE_ADDRESS_SALT, Some(epoch_3)),
					Utxo {
						id: UtxoId {
							tx_id: hex_literal::hex!(
								"583c4f12102fc4bea1af47d14f9d0b90fca0783af1c5d03d894a44be5ae165bc"
							)
							.into(),
							vout: 0,
						},
						amount: 31_000_000 + 32_000_000 - expected_consolidation_fee,
						deposit_address: DepositAddress { pubkey_x: epoch_3, script_path: None }
					},
					Utxo {
						id: UtxoId {
							tx_id: hex_literal::hex!(
								"7412bc12c3e68a975c910f998afb9ac4b7de1426474642f2067727183dfe6c26"
							)
							.into(),
							vout: 0,
						},
						amount: 33_000_000 + 34_000_000 - expected_consolidation_fee,
						deposit_address: DepositAddress { pubkey_x: epoch_3, script_path: None }
					},
					Utxo {
						id: UtxoId {
							tx_id: hex_literal::hex!(
								"2e80638a37518a081ff08a6b81e522dc68c1d46496ffcde64516489935150ecf"
							)
							.into(),
							vout: 0,
						},
						amount: 21_000_000 + 22_000_000 - expected_consolidation_fee,
						deposit_address: DepositAddress { pubkey_x: epoch_3, script_path: None }
					},
				]
			);
		});
}
