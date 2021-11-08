#[cfg(test)]
mod tests {
	use frame_support::{
		assert_ok,
		sp_io::TestExternalities,
		traits::{GenesisBuild, OnInitialize},
	};
	use sp_consensus_aura::sr25519::AuthorityId as AuraId;
	use sp_core::crypto::{Pair, Public};
	use sp_finality_grandpa::AuthorityId as GrandpaId;
	use sp_runtime::{traits::Zero, Storage};
	use state_chain_runtime::{
		constants::common::*, opaque::SessionKeys, AccountId, Auction, Emissions, Flip, Governance,
		Online, Reputation, Rewards, Runtime, Session, Staking, System, Timestamp, Validator,
		Vaults,
	};

	use cf_chains::ChainId;
	use cf_traits::{BlockNumber, FlipBalance, IsOnline};
	use sp_runtime::AccountId32;

	type NodeId = AccountId32;

	macro_rules! on_events {
		($events:expr, $( $p:pat => $b:block ),*) => {
			for event in $events {
				$(if let $p = event { $b })*
			}
		}
	}

	mod network {
		use super::*;
		use crate::tests::BLOCK_TIME;
		use cf_chains::eth::SchnorrVerificationComponents;
		use cf_traits::{ChainflipAccount, ChainflipAccountState, ChainflipAccountStore};
		use frame_support::traits::HandleLifetime;
		use pallet_cf_staking::{EthTransactionHash, EthereumAddress};
		use state_chain_runtime::{Event, HeartbeatBlockInterval, Origin};
		use std::collections::HashMap;
		const ETH_ZERO_ADDRESS: EthereumAddress = [0xff; 20];
		const TX_HASH: EthTransactionHash = [211u8; 32];

		// Events from ethereum contract
		#[derive(Debug, Clone)]
		pub enum ContractEvent {
			Staked { node_id: NodeId, amount: FlipBalance, total: FlipBalance },
		}

		// A staking contract
		#[derive(Default)]
		pub struct StakingContract {
			// List of stakes
			pub stakes: HashMap<NodeId, FlipBalance>,
			// Events to be processed
			pub events: Vec<ContractEvent>,
		}

		impl StakingContract {
			// Stake for validator
			pub fn stake(&mut self, node_id: NodeId, amount: FlipBalance) {
				let current_amount = self.stakes.get(&node_id).unwrap_or(&0);
				let total = current_amount + amount;
				self.stakes.insert(node_id.clone(), total);

				self.events.push(ContractEvent::Staked { node_id, amount, total });
			}
			// Get events for this contract
			fn events(&self) -> Vec<ContractEvent> {
				self.events.clone()
			}
			// Clear events
			fn clear(&mut self) {
				self.events.clear();
			}
		}

		// Engine monitoring contract
		pub struct Engine {
			pub node_id: NodeId,
			pub active: bool,
		}

		impl Engine {
			fn new(node_id: NodeId) -> Self {
				Engine { node_id, active: true }
			}

			fn state(&self) -> ChainflipAccountState {
				ChainflipAccountStore::<Runtime>::get(&self.node_id).state
			}

			// Handle events from contract
			fn on_contract_event(&self, event: &ContractEvent) {
				if self.state() == ChainflipAccountState::Validator && self.active {
					match event {
						ContractEvent::Staked { node_id: validator_id, amount, .. } => {
							// Witness event -> send transaction to state chain
							state_chain_runtime::WitnesserApi::witness_staked(
								Origin::signed(self.node_id.clone()),
								validator_id.clone(),
								*amount,
								ETH_ZERO_ADDRESS,
								TX_HASH,
							)
							.expect("should be able to witness stake for node");
						},
					}
				}
			}

			// Handle events coming in from the state chain
			// TODO have this abstracted out
			fn handle_state_chain_events(&self, events: &[Event]) {
				if self.state() == ChainflipAccountState::Validator && self.active {
					// Handle events
					on_events!(
						events,
						Event::EthereumThresholdSigner(
							// A signature request
							pallet_cf_threshold_signature::Event::ThresholdSignatureRequest(
								ceremony_id,
								_,
								ref signers,
								_)) => {

							// Participate in signing ceremony if requested
							if signers.contains(&self.node_id) {
								// TODO signature generation, will fail when we have verification implemented
								let signature = SchnorrVerificationComponents {
									s: [0u8; 32],
									k_times_g_addr: [0u8; 20],
								};

								state_chain_runtime::WitnesserApi::witness_eth_signature_success(
									Origin::signed(self.node_id.clone()),
									*ceremony_id,
									signature,
								).expect("should be able to ethereum signature for node");
							}
						},
						Event::EthereumThresholdSigner(
							// A threshold has been met for this signature
							pallet_cf_threshold_signature::Event::ThresholdSignatureSuccess(
								_ceremony_id)) => {
								// Witness a vault rotation?
								let ethereum_block_number: u64 = 100;
								let mut new_public_key = vec![0u8; 33];
								new_public_key[0] = 2;
								let tx_hash = vec![1u8; 32];
								state_chain_runtime::WitnesserApi::witness_vault_key_rotated(
									Origin::signed(self.node_id.clone()),
									ChainId::Ethereum,
									new_public_key,
									ethereum_block_number,
									tx_hash,
								).expect("should be able to vault key rotation for node");
						},
						Event::Vaults(
							// A keygen request has been made
							pallet_cf_vaults::Event::KeygenRequest(ceremony_id, ..)) => {
								// Generate a public agg key, TODO refactor out
								let mut public_key = vec![0u8; 33];
								public_key[0] = 2;

								state_chain_runtime::WitnesserApi::witness_keygen_success(
									Origin::signed(self.node_id.clone()),
									*ceremony_id,
									ChainId::Ethereum,
									public_key
								).expect(&format!(
									"should be able to witness keygen request from node: {:?}",
									self.node_id)
								);
						}
					);
				}
			}

			// On block handler
			fn on_block(&self, block_number: BlockNumber) {
				if self.active {
					// Heartbeat -> Send transaction to state chain twice an interval
					if block_number % (HeartbeatBlockInterval::get() / 2) == 0 {
						// Online pallet, ignore error
						let _ = Online::heartbeat(state_chain_runtime::Origin::signed(
							self.node_id.clone(),
						));
					}
				}
			}
		}

		// Create an account, generate and register the session keys
		fn setup_account(node_id: &NodeId) {
			assert_ok!(frame_system::Provider::<Runtime>::created(&node_id));

			let seed = &node_id.clone().to_string();

			let key = SessionKeys {
				aura: get_from_seed::<AuraId>(seed),
				grandpa: get_from_seed::<GrandpaId>(seed),
			};

			assert_ok!(state_chain_runtime::Session::set_keys(
				state_chain_runtime::Origin::signed(node_id.clone()),
				key,
				vec![]
			));
		}

		#[derive(Default)]
		pub struct Network {
			engines: HashMap<NodeId, Engine>,
			pub stake_manager_contract: StakingContract,
			last_event: usize,
		}

		impl Network {
			pub fn create(number_of_nodes: u8) -> (Self, Vec<NodeId>) {
				let mut network: Network = Network::default();

				let mut nodes = Vec::new();
				for index in 1..=number_of_nodes {
					let node_id: NodeId = [index; 32].into();
					nodes.push(node_id.clone());
					setup_account(&node_id);
					network.engines.insert(node_id.clone(), Engine::new(node_id));
				}

				(network, nodes)
			}

			pub fn set_active(&mut self, node_id: &NodeId, active: bool) {
				self.engines.get_mut(node_id).expect("valid node_id").active = active;
			}

			pub fn add_node(&mut self, node_id: NodeId) {
				setup_account(&node_id);
				self.engines.insert(node_id.clone(), Engine { node_id, active: true });
			}

			pub fn move_forward_blocks(&mut self, n: u32) {
				pub const INIT_TIMESTAMP: u64 = 30_000;
				let current_block_number = System::block_number();
				while System::block_number() < current_block_number + n {
					System::set_block_number(System::block_number() + 1);
					Timestamp::set_timestamp(
						(System::block_number() as u64 * BLOCK_TIME) + INIT_TIMESTAMP,
					);
					Session::on_initialize(System::block_number());
					Online::on_initialize(System::block_number());
					Flip::on_initialize(System::block_number());
					Staking::on_initialize(System::block_number());
					Auction::on_initialize(System::block_number());
					Emissions::on_initialize(System::block_number());
					Governance::on_initialize(System::block_number());
					Reputation::on_initialize(System::block_number());
					Vaults::on_initialize(System::block_number());
					Validator::on_initialize(System::block_number());

					// Notify contract events
					for event in self.stake_manager_contract.events() {
						for (_, engine) in &self.engines {
							engine.on_contract_event(&event);
						}
					}

					// Clear events on contract
					self.stake_manager_contract.clear();

					// Collect state chain events
					let events = frame_system::Pallet::<Runtime>::events()
						.into_iter()
						.map(|e| e.event)
						.skip(self.last_event)
						.collect::<Vec<Event>>();

					self.last_event += events.len();

					// State chain events
					for (_, engine) in &self.engines {
						engine.handle_state_chain_events(&events);
					}

					// A completed block notification
					for (_, engine) in &self.engines {
						engine.on_block(System::block_number());
					}
				}
			}
		}
	}

	// TODO - remove collision of account numbers
	pub const ALICE: [u8; 32] = [0xff; 32];
	pub const BOB: [u8; 32] = [0xfe; 32];
	pub const CHARLIE: [u8; 32] = [0xfd; 32];
	pub const ERIN: [u8; 32] = [0xfc; 32];

	pub const BLOCK_TIME: u64 = 1000;

	pub fn get_from_seed<TPublic: Public>(seed: &str) -> <TPublic::Pair as Pair>::Public {
		TPublic::Pair::from_string(&format!("//{}", seed), None)
			.expect("static values are valid; qed")
			.public()
	}

	pub struct ExtBuilder {
		accounts: Vec<(AccountId, FlipBalance)>,
		winners: Vec<AccountId>,
		root: AccountId,
		blocks_per_epoch: BlockNumber,
		max_validators: u32,
	}

	impl Default for ExtBuilder {
		fn default() -> Self {
			Self {
				accounts: vec![],
				winners: vec![],
				root: AccountId::default(),
				blocks_per_epoch: Zero::zero(),
				max_validators: MAX_VALIDATORS,
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

		fn max_validators(mut self, max_validators: u32) -> Self {
			self.max_validators = max_validators;
			self
		}

		fn configure_storages(&self, storage: &mut Storage) {
			pallet_cf_flip::GenesisConfig::<Runtime> { total_issuance: TOTAL_ISSUANCE }
				.assimilate_storage(storage)
				.unwrap();

			pallet_cf_staking::GenesisConfig::<Runtime> { genesis_stakers: self.accounts.clone() }
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
				validator_size_range: (1, self.max_validators),
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

			GenesisBuild::<Runtime>::assimilate_storage(
				&pallet_cf_vaults::GenesisConfig {
					ethereum_vault_key: {
						let key: [u8; 33] = hex_literal::hex![
							"0339e302f45e05949fbb347e0c6bba224d82d227a701640158bc1c799091747015"
						];
						key.to_vec()
					},
				},
				storage,
			)
			.unwrap();

			pallet_cf_validator::GenesisConfig::<Runtime> {
				blocks_per_epoch: self.blocks_per_epoch,
			}
			.assimilate_storage(storage)
			.unwrap();
		}

		/// Default ext configuration with BlockNumber 1
		pub fn build(&self) -> TestExternalities {
			let mut storage =
				frame_system::GenesisConfig::default().build_storage::<Runtime>().unwrap();

			self.configure_storages(&mut storage);

			let mut ext = TestExternalities::from(storage);
			ext.execute_with(|| System::set_block_number(1));

			ext
		}
	}

	mod genesis {
		use super::*;
		use cf_traits::{AuctionResult, Auctioneer, StakeTransfer};
		pub const GENESIS_BALANCE: FlipBalance = TOTAL_ISSUANCE / 100;

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

				let accounts =
					[AccountId::from(ALICE), AccountId::from(BOB), AccountId::from(CHARLIE)];

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
				let AuctionResult { winners, minimum_active_bid } =
					Auction::auction_result().expect("an auction result");
				assert_eq!(minimum_active_bid, GENESIS_BALANCE);
				assert_eq!(winners, accounts);

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
					assert!(!Online::is_online(account), "node should have not sent a heartbeat");
				}

				assert_eq!(Emissions::last_mint_block(), 0, "no emissions");

				assert_eq!(
					Rewards::offchain_funds(pallet_cf_rewards::VALIDATOR_REWARDS),
					0,
					"no rewards"
				);

				assert_eq!(Vaults::keygen_ceremony_id_counter(), 0, "no key generation requests");

				assert_eq!(Vaults::chain_nonces(ChainId::Ethereum), 0, "nonce not incremented");

				assert!(
					Governance::members().contains(&AccountId::from(ERIN)),
					"expected governor"
				);
				assert_eq!(Governance::number_of_proposals(), 0, "no proposal for governance");

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

	// The number of blocks we expect an auction should last
	const AUCTION_BLOCKS: BlockNumber = 2;

	mod epoch {
		use super::*;
		use cf_traits::{AuctionPhase, AuctionResult, EpochInfo};
		use state_chain_runtime::{Auction, Validator};

		#[test]
		// An epoch has completed.  We have a genesis where the blocks per epoch are set to 100
		// - When the epoch is reached an auction is started and completed
		// - New stakers that were above the genesis MAB are now validating the network with the
		//   genesis validators
		// - A new auction index has been generated
		fn epoch_rotates() {
			const EPOCH_BLOCKS: BlockNumber = 100;
			super::genesis::default()
				.blocks_per_epoch(EPOCH_BLOCKS)
				.build()
				.execute_with(|| {
					// A network with a set of passive nodes
					let (mut testnet, mut nodes) = network::Network::create(5);
					// Add the genesis nodes to the test network
					for validator in Validator::current_validators() {
						testnet.add_node(validator);
					}
					// All nodes stake to be included in the next epoch which are witnessed on the
					// state chain
					for node in &nodes {
						testnet
							.stake_manager_contract
							.stake(node.clone(), genesis::GENESIS_BALANCE + 1);
					}
					// Run to the next epoch to start the auction
					testnet.move_forward_blocks(EPOCH_BLOCKS);
					// We should be in auction 1
					assert_eq!(
						Auction::current_auction_index(),
						1,
						"this should be the first auction"
					);

					let genesis_validators: Vec<NodeId> = Validator::current_validators();

					// We expect the following to become the next set of validators
					let mut expected_validators = genesis_validators;
					expected_validators.append(&mut nodes);
					expected_validators.sort();

					// In this block we should have reached the state `ValidatorsSelected`
					// and in this group we would have in this network the genesis validators and
					// the nodes that have staked as well
					assert_matches::assert_matches!(
						Auction::current_phase(),
						AuctionPhase::ValidatorsSelected(mut candidates, _) => {
							candidates.sort();
							assert_eq!(candidates, expected_validators);
						},
						"the new candidates should be those genesis validators and the new nodes created in test"
					);
					// For each subsequent block the state chain will check if the vault has rotated
					// until then we stay in the `ValidatorsSelected`
					// Run things the amount needed for an auction
					testnet.move_forward_blocks(2);
					// The vault rotation should have proceeded and we should now be back
					// at `WaitingForBids` with a new set of winners; the genesis validators and
					// the new nodes we staked into the network
					assert_matches::assert_matches!(
						Auction::current_phase(),
						AuctionPhase::WaitingForBids,
						"we should back waiting for bids after a successful auction and rotation"
					);

					let AuctionResult { mut winners, minimum_active_bid } =
						Auction::last_auction_result().expect("last auction result");

					assert_eq!(
						minimum_active_bid,
						genesis::GENESIS_BALANCE,
						"minimum active bid should be that set at genesis"
					);

					winners.sort();
					assert_eq!(
						winners,
						expected_validators,
						"the new winners should be those genesis validators and the new nodes created in test"
					);

					let mut new_validators = Validator::current_validators();
					new_validators.sort();

					// This new set of winners should also be the validators of the network
					assert_eq!(
						new_validators,
						expected_validators,
						"the new validators should be those genesis validators and the new nodes created in test"
					);
				});
		}
	}

	mod validators {
		use crate::tests::{genesis, network, NodeId, AUCTION_BLOCKS};
		use cf_traits::{AuctionPhase, EpochInfo, FlipBalance, IsOnline, StakeTransfer};
		use state_chain_runtime::{
			Auction, EmergencyRotationPercentageTrigger, Flip, HeartbeatBlockInterval, Online,
			Validator,
		};

		#[test]
		// We have a set of backup validators who receive rewards
		// A network is created where we have a validating set with a set of backup validators
		// The backup validators would receive emissions on each heartbeat
		fn backup_rewards() {
			// We want to have at least one heartbeat within our reduced epoch
			const EPOCH_BLOCKS: u32 = HeartbeatBlockInterval::get() * 2;
			// Reduce our validating set and hence the number of nodes we need to have a backup
			// set
			const MAX_VALIDATORS: u32 = 10;
			super::genesis::default()
				.blocks_per_epoch(EPOCH_BLOCKS)
				.max_validators(MAX_VALIDATORS)
				.build()
				.execute_with(|| {
					// Create MAX_VALIDATORS nodes and stake them above our genesis validators
					// The result will be our newly created nodes will be validators and the
					// genesis validators will become backup validators
					let (mut testnet, mut nodes) = network::Network::create(MAX_VALIDATORS as u8);
					// Add the genesis nodes to the test network
					let mut genesis_validators = Validator::current_validators();
					for validator in &genesis_validators {
						testnet.add_node(validator.clone());
					}

					// An initial stake which is superior to the genesis stakes
					const INITIAL_STAKE: FlipBalance = genesis::GENESIS_BALANCE + 1;
					// Stake these nodes so that they are included in the next epoch
					for node in &nodes {
						testnet.stake_manager_contract.stake(node.clone(), INITIAL_STAKE);
					}

					// Start an auction and confirm
					testnet.move_forward_blocks(EPOCH_BLOCKS);
					assert_eq!(
						Auction::current_auction_index(),
						1,
						"this should be the first auction"
					);

					// Complete auction over AUCTION_BLOCKS
					testnet.move_forward_blocks(AUCTION_BLOCKS);
					assert_matches::assert_matches!(
						Auction::current_phase(),
						AuctionPhase::WaitingForBids,
						"we should back waiting for bids after a successful auction and rotation"
					);

					// assert list of validators as being the new nodes
					let mut current_validators: Vec<NodeId> = Validator::current_validators();

					current_validators.sort();
					nodes.sort();

					assert_eq!(
						nodes, current_validators,
						"our new nodes should be the new validators"
					);

					// assert list of backup validators as being the genesis validators
					let mut current_backup_validators: Vec<NodeId> = Auction::remaining_bidders()
						.iter()
						.take(Auction::backup_group_size() as usize)
						.map(|(validator_id, _)| validator_id.clone())
						.collect();

					current_backup_validators.sort();
					genesis_validators.sort();

					assert_eq!(
						genesis_validators, current_backup_validators,
						"we should have new backup validators"
					);

					// Move forward a heartbeat, emissions should be shared to backup validators
					testnet.move_forward_blocks(HeartbeatBlockInterval::get());

					// We won't calculate the exact emissions but they should be greater than their
					// initial stake
					for backup_validator in &current_backup_validators {
						assert!(INITIAL_STAKE < Flip::stakeable_balance(backup_validator));
					}
				});
		}

		#[test]
		// A network is created with a set of validators and backup validators.
		// EmergencyRotationPercentageTrigger(80%) of the validators continue to submit heartbeats
		// with 20% going offline and forcing an emergency rotation in which a new set of validators
		// start to validate the network which includes live validators and previous backup
		// validators
		fn emergency_rotations() {
			// We want to be able to miss heartbeats to be offline and provoke an emergency rotation
			// In order to do this we would want to have missed 3 heartbeats
			const PERCENTAGE_OFFLINE: u32 = 100 - EmergencyRotationPercentageTrigger::get() as u32;
			// Blocks for our epoch
			const EPOCH_BLOCKS: u32 = HeartbeatBlockInterval::get() * 4;
			// Reduce our validating set and hence the number of nodes we need to have a backup
			// set to speed the test up
			const MAX_VALIDATORS: u32 = 10;
			super::genesis::default()
				.blocks_per_epoch(EPOCH_BLOCKS)
				.max_validators(MAX_VALIDATORS)
				.build()
				.execute_with(|| {
					let (mut testnet, nodes) = network::Network::create(MAX_VALIDATORS as u8);
					// Add the genesis nodes to the test network
					let genesis_validators = Validator::current_validators();
					for validator in &genesis_validators {
						testnet.add_node(validator.clone());
					}

					// An initial stake which is superior to the genesis stakes
					const INITIAL_STAKE: FlipBalance = genesis::GENESIS_BALANCE + 1;
					// Stake these nodes so that they are included in the next epoch
					for node in &nodes {
						testnet.stake_manager_contract.stake(node.clone(), INITIAL_STAKE);
					}

					// Start an auction and confirm
					testnet.move_forward_blocks(EPOCH_BLOCKS);
					// Complete auction over AUCTION_BLOCKS
					testnet.move_forward_blocks(AUCTION_BLOCKS);

					// Set PERCENTAGE_OFFLINE of the validators inactive
					let number_offline = (MAX_VALIDATORS * PERCENTAGE_OFFLINE / 100) as usize;

					let offline_nodes: Vec<_> =
						nodes.iter().take(number_offline).cloned().collect();

					for node in &offline_nodes {
						testnet.set_active(node, false);
					}

					// We need to move forward three heartbeats to be regarded as offline
					testnet.move_forward_blocks(3 * HeartbeatBlockInterval::get());

					// We should have a set of nodes offline
					for node in &offline_nodes {
						assert_eq!(false, Online::is_online(node), "the node should be offline");
					}

					// The network state should now be in an emergency and that the validator
					// pallet has been requested to start an emergency rotation
					assert!(
						Validator::emergency_rotation_requested(),
						"we should have requested an emergency rotation"
					);

					// The next block should see an auction started
					testnet.move_forward_blocks(1);

					assert_eq!(
						Auction::current_auction_index(),
						2,
						"this should be the second auction"
					);

					// Complete the 'Emergency rotation'
					testnet.move_forward_blocks(AUCTION_BLOCKS);
					assert_matches::assert_matches!(
						Auction::current_phase(),
						AuctionPhase::WaitingForBids,
						"we should back waiting for bids after a successful auction and rotation"
					);
				});
		}
	}
}
