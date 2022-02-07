#[cfg(test)]
mod tests {
	use frame_support::{
		assert_noop, assert_ok,
		sp_io::TestExternalities,
		traits::{GenesisBuild, OnInitialize},
	};
	use sp_consensus_aura::sr25519::AuthorityId as AuraId;
	use sp_core::crypto::{Pair, Public};
	use sp_finality_grandpa::AuthorityId as GrandpaId;
	use sp_runtime::{traits::Zero, Storage};
	use state_chain_runtime::{
		constants::common::*, opaque::SessionKeys, AccountId, Auction, Emissions, EthereumVault,
		Flip, Governance, Online, Origin, Reputation, Runtime, Session, Staking, System, Timestamp,
		Validator,
	};

	use cf_traits::{BlockNumber, EpochIndex, FlipBalance, IsOnline};
	use libsecp256k1::SecretKey;
	use pallet_cf_staking::{EthTransactionHash, EthereumAddress};
	use rand::{prelude::*, SeedableRng};
	use sp_runtime::AccountId32;

	type NodeId = AccountId32;
	const ETH_ZERO_ADDRESS: EthereumAddress = [0xff; 20];
	const TX_HASH: EthTransactionHash = [211u8; 32];

	macro_rules! on_events {
		($events:expr, $( $p:pat => $b:block ),* $(,)?) => {
			for event in $events {
				$(if let $p = event { $b })*
			}
		}
	}

	pub const GENESIS_KEY: u64 = 42;

	mod network {
		use super::*;
		use crate::tests::BLOCK_TIME;
		use cf_chains::eth::{to_ethereum_address, AggKey, SchnorrVerificationComponents};
		use cf_traits::{ChainflipAccount, ChainflipAccountState, ChainflipAccountStore};
		use frame_support::traits::HandleLifetime;
		use libsecp256k1::PublicKey;
		use pallet_cf_vaults::KeygenOutcome;
		use state_chain_runtime::{Event, HeartbeatBlockInterval, Origin};
		use std::{cell::RefCell, collections::HashMap, rc::Rc};

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

		pub struct Signer {
			agg_secret_key: SecretKey,
			signatures: HashMap<cf_chains::eth::H256, [u8; 32]>,
			key_seed: u64,
			proposed_seed: Option<u64>,
		}

		impl Default for Signer {
			fn default() -> Self {
				let key_seed = GENESIS_KEY;
				let (agg_secret_key, _) = Self::generate_keypair(key_seed);
				Signer { agg_secret_key, signatures: HashMap::new(), key_seed, proposed_seed: None }
			}
		}

		impl Signer {
			// Sign message with current key, caches signatures
			pub fn sign(
				&mut self,
				message: &cf_chains::eth::H256,
			) -> SchnorrVerificationComponents {
				// A nonce, k
				let (k, k_times_g) = Self::generate_keypair(self.key_seed * 2);
				// k.G
				let k_times_g_addr = to_ethereum_address(k_times_g);
				// If this message has been signed before return from cache else sign and cache
				return match self.signatures.get(message) {
					Some(signature) =>
						SchnorrVerificationComponents { s: *signature, k_times_g_addr },
					None => {
						let agg_key =
							AggKey::from_private_key_bytes(self.agg_secret_key.serialize());
						let signature = agg_key.sign(&(*message).into(), &self.agg_secret_key, &k);

						self.signatures.insert(*message, signature);

						SchnorrVerificationComponents { s: signature, k_times_g_addr }
					},
				}
			}

			// Generate a keypair with seed
			pub fn generate_keypair(seed: u64) -> (SecretKey, PublicKey) {
				let agg_key_priv: [u8; 32] = StdRng::seed_from_u64(seed).gen();
				let secret_key = SecretKey::parse(&agg_key_priv).unwrap();
				(secret_key, PublicKey::from_secret_key(&secret_key))
			}

			// The public key proposed
			pub fn proposed_public_key(&mut self) -> AggKey {
				let (_, public) =
					Self::generate_keypair(self.proposed_seed.expect("No key has been proposed"));
				AggKey::from_pubkey_compressed(public.serialize_compressed())
			}

			// Propose a new public key
			pub fn propose_new_public_key(&mut self) -> AggKey {
				self.proposed_seed = Some(self.key_seed + 1);
				self.proposed_public_key()
			}

			// Rotate to the current proposed key and clear cache
			pub fn rotate_keys(&mut self) {
				if self.proposed_seed.is_some() {
					self.key_seed += self.proposed_seed.expect("No key has been proposed");
					let (secret_key, _) = Self::generate_keypair(self.key_seed);
					self.agg_secret_key = secret_key;
					self.signatures.clear();
					self.proposed_seed = None;
				}
			}
		}

		pub enum EngineState {
			None,
			Rotation,
		}

		// Engine monitoring contract
		pub struct Engine {
			pub node_id: NodeId,
			pub active: bool,
			pub signer: Rc<RefCell<Signer>>,
			pub engine_state: EngineState,
		}

		impl Engine {
			fn new(node_id: NodeId, signer: Rc<RefCell<Signer>>) -> Self {
				Engine { node_id, active: true, signer, engine_state: EngineState::None }
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
			fn handle_state_chain_events(&mut self, events: &[Event]) {
				// If active handle events
				if self.active {
					// Being a validator we would respond to certain events
					if self.state() == ChainflipAccountState::Validator {
						on_events!(
							events,
							Event::Validator(
								// A new epoch
								pallet_cf_validator::Event::NewEpoch(_epoch_index)) => {
									(&*self.signer).borrow_mut().rotate_keys();
							},
							Event::EthereumThresholdSigner(
								// A signature request
								pallet_cf_threshold_signature::Event::ThresholdSignatureRequest(
									ceremony_id,
									_,
									ref signers,
									payload)) => {

								// Participate in signing ceremony if requested.
								// We only need one node to submit the unsigned transaction.
								if let Some(node_id) = signers.get(0) { if node_id == &self.node_id {
									// Sign with current key
									let verification_components = (&*self.signer).borrow_mut().sign(payload);
									state_chain_runtime::EthereumThresholdSigner::signature_success(
										Origin::none(),
										*ceremony_id,
										verification_components,
									).expect("should be able to submit threshold signature for Ethereum");
								} };
							},
							Event::EthereumThresholdSigner(
								// A threshold has been met for this signature
								pallet_cf_threshold_signature::Event::ThresholdDispatchComplete(..)) => {
									match self.engine_state {
										// If we rotating let's witness the keys being rotated on the contract
										EngineState::Rotation => {
											let ethereum_block_number: u64 = 100;
											let tx_hash = [1u8; 32];

											let public_key = (&*self.signer).borrow_mut().proposed_public_key();

											state_chain_runtime::WitnesserApi::witness_eth_aggkey_rotation(
												Origin::signed(self.node_id.clone()),
												public_key,
												ethereum_block_number,
												tx_hash.into(),
											).expect("should be able to vault key rotation for node");
										},
										_ => {}
									}
							},
							Event::EthereumVault(pallet_cf_vaults::Event::KeygenSuccess(..)) => {
								self.engine_state = EngineState::Rotation;
							},
							Event::EthereumVault(pallet_cf_vaults::Event::VaultRotationCompleted) => {
								self.engine_state = EngineState::None;
							},
						);
					}

					// Being staked we would be required to respond to keygen requests
					on_events!(
						events,
						Event::EthereumVault(
							// A keygen request has been made
							pallet_cf_vaults::Event::KeygenRequest(ceremony_id, validators)) => {
								if validators.contains(&self.node_id) {
									// Propose a new key
									let public_key = (&*self.signer).borrow_mut().propose_new_public_key();

									state_chain_runtime::EthereumVault::report_keygen_outcome(
										Origin::signed(self.node_id.clone()),
										*ceremony_id,
										KeygenOutcome::Success(public_key),
									).expect(&format!(
										"should be able to report keygen outcome from node: {}",
										self.node_id)
									);
								}
						},
					);
				}
			}

			// On block handler
			fn on_block(&self, block_number: BlockNumber) {
				if self.active {
					// Heartbeat -> Send transaction to state chain twice an interval
					if block_number % (HeartbeatBlockInterval::get() / 2) == 0 {
						// Online pallet
						let _ = Online::heartbeat(state_chain_runtime::Origin::signed(
							self.node_id.clone(),
						));
					}
				}
			}
		}

		// Create an account, generate and register the session keys
		pub(crate) fn setup_account(node_id: &NodeId, seed: &String) {
			assert_ok!(frame_system::Provider::<Runtime>::created(&node_id));

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

		pub(crate) fn setup_peer_mapping(node_id: &NodeId, seed: &String) {
			let peer_keypair = sp_core::ed25519::Pair::from_legacy_string(seed, None);

			use sp_core::Encode;
			assert_ok!(state_chain_runtime::Validator::register_peer_id(
				state_chain_runtime::Origin::signed(node_id.clone()),
				peer_keypair.public(),
				0,
				0,
				peer_keypair.sign(&node_id.encode()[..]),
			));
		}

		#[derive(Default)]
		pub struct Network {
			engines: HashMap<NodeId, Engine>,
			pub stake_manager_contract: StakingContract,
			last_event: usize,
			node_counter: u32,
			pub signer: Rc<RefCell<Signer>>,
		}

		impl Network {
			pub fn next_node_id(&mut self) -> NodeId {
				self.node_counter += 1;
				// TODO improve this to not overflow
				[self.node_counter as u8; 32].into()
			}

			// Create a network which includes the validators in genesis of number of nodes
			// and return a network and sorted list of nodes within
			pub fn create(number_of_nodes: u8, nodes_to_include: &[NodeId]) -> (Self, Vec<NodeId>) {
				let mut network: Network = Default::default();

				// Include any nodes already *created* to the test network
				for node in nodes_to_include {
					network.add_node(node);
					setup_peer_mapping(node, &node.clone().to_string());
				}

				let remaining_nodes = number_of_nodes.saturating_sub(nodes_to_include.len() as u8);

				let mut nodes = Vec::new();
				for _ in 0..remaining_nodes {
					let node_id = network.next_node_id();
					nodes.push(node_id.clone());
					let seed = node_id.clone().to_string();
					setup_account(&node_id, &seed);
					setup_peer_mapping(&node_id, &seed);
					network
						.engines
						.insert(node_id.clone(), Engine::new(node_id, network.signer.clone()));
				}

				nodes.append(&mut nodes_to_include.to_vec());
				nodes.sort();
				(network, nodes)
			}

			pub fn filter_nodes(&self, state: ChainflipAccountState) -> Vec<NodeId> {
				self.engines
					.iter()
					.filter_map(
						|(node_id, engine)| {
							if engine.state() == state {
								Some(node_id)
							} else {
								None
							}
						},
					)
					.cloned()
					.collect()
			}

			pub fn set_active(&mut self, node_id: &NodeId, active: bool) {
				self.engines.get_mut(node_id).expect("valid node_id").active = active;
			}

			pub fn create_node(&mut self) -> NodeId {
				let node_id = self.next_node_id().into();
				self.add_node(&node_id);
				node_id
			}

			// Adds a node which doesn't have its session keys set
			pub fn add_node(&mut self, node_id: &NodeId) {
				self.engines.insert(
					node_id.clone(),
					Engine {
						node_id: node_id.clone(),
						active: true,
						signer: self.signer.clone(),
						engine_state: EngineState::None,
					},
				);
			}

			pub fn move_to_next_epoch(&mut self, epoch: u32) {
				let current_block_number = System::block_number();
				self.move_forward_blocks(epoch - (current_block_number % epoch));
			}

			pub fn move_to_next_heartbeat_interval(&mut self) {
				let current_block_number = System::block_number();
				self.move_forward_blocks(
					HeartbeatBlockInterval::get() -
						(current_block_number % HeartbeatBlockInterval::get()) +
						1,
				);
			}

			pub fn move_forward_blocks(&mut self, n: u32) {
				pub const INIT_TIMESTAMP: u64 = 30_000;
				let current_block_number = System::block_number();
				while System::block_number() < current_block_number + n {
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
					EthereumVault::on_initialize(System::block_number());
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
					for (_, engine) in self.engines.iter_mut() {
						engine.handle_state_chain_events(&events);
					}

					// A completed block notification
					for (_, engine) in &self.engines {
						engine.on_block(System::block_number());
					}
					System::set_block_number(System::block_number() + 1);
				}
			}
		}
	}

	// TODO - remove collision of account numbers
	pub const ALICE: [u8; 32] = [0xff; 32];
	pub const BOB: [u8; 32] = [0xfe; 32];
	pub const CHARLIE: [u8; 32] = [0xfd; 32];
	// Root and Gov member
	pub const ERIN: [u8; 32] = [0xfc; 32];

	pub const BLOCK_TIME: u64 = 1000;
	const GENESIS_EPOCH: EpochIndex = 1;

	pub fn get_from_seed<TPublic: Public>(seed: &str) -> <TPublic::Pair as Pair>::Public {
		TPublic::Pair::from_string(&format!("//{}", seed), None)
			.expect("static values are valid; qed")
			.public()
	}

	pub struct ExtBuilder {
		pub accounts: Vec<(AccountId, FlipBalance)>,
		winners: Vec<AccountId>,
		root: AccountId,
		blocks_per_epoch: BlockNumber,
		max_validators: u32,
		min_validators: u32,
	}

	impl Default for ExtBuilder {
		fn default() -> Self {
			Self {
				accounts: vec![],
				winners: vec![],
				root: AccountId::default(),
				blocks_per_epoch: Zero::zero(),
				max_validators: MAX_VALIDATORS,
				min_validators: 1,
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

		fn min_validators(mut self, min_validators: u32) -> Self {
			self.min_validators = min_validators;
			self
		}

		fn configure_storages(&self, storage: &mut Storage) {
			pallet_cf_flip::GenesisConfig::<Runtime> { total_issuance: TOTAL_ISSUANCE }
				.assimilate_storage(storage)
				.unwrap();

			pallet_cf_staking::GenesisConfig::<Runtime> {
				genesis_stakers: self.accounts.clone(),
				minimum_stake: MIN_STAKE,
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
				validator_size_range: (self.min_validators, self.max_validators),
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

			pallet_cf_validator::GenesisConfig::<Runtime> {
				blocks_per_epoch: self.blocks_per_epoch,
			}
			.assimilate_storage(storage)
			.unwrap();

			let (_, public_key) = network::Signer::generate_keypair(GENESIS_KEY);
			let ethereum_vault_key = public_key.serialize_compressed().to_vec();

			GenesisBuild::<Runtime, _>::assimilate_storage(
				&state_chain_runtime::EthereumVaultConfig {
					vault_key: ethereum_vault_key,
					deployment_block: 0,
				},
				storage,
			)
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
		use cf_traits::{
			AuctionResult, Auctioneer, ChainflipAccount, ChainflipAccountState,
			ChainflipAccountStore, StakeTransfer,
		};
		pub const GENESIS_BALANCE: FlipBalance = TOTAL_ISSUANCE / 100;
		pub const NUMBER_OF_VALIDATORS: u32 = 3;

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
		// - The genesis validators have last active epoch set
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
					EthereumVault::keygen_ceremony_id_counter(),
					0,
					"no key generation requests"
				);

				assert_eq!(EthereumVault::chain_nonce(), 0, "nonce not incremented");

				assert!(
					Governance::members().contains(&AccountId::from(ERIN)),
					"expected governor"
				);
				assert_eq!(Governance::proposal_id_counter(), 0, "no proposal for governance");

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

				for account in accounts.iter() {
					let account_data = ChainflipAccountStore::<Runtime>::get(account);
					assert_eq!(
						Some(1),
						account_data.last_active_epoch,
						"validator should be active in the genesis epoch(1)"
					);
					assert_eq!(ChainflipAccountState::Validator, account_data.state);
				}
			});
		}
	}

	// The minimum number of blocks an auction will last
	const AUCTION_BLOCKS: BlockNumber = 2;
	// The minimum number of blocks a keygen ceremony will last
	const KEYGEN_CEREMONY_BLOCKS: BlockNumber = 3;
	// The minimum number of blocks a vault rotation should last
	const VAULT_ROTATION_BLOCKS: BlockNumber = AUCTION_BLOCKS + KEYGEN_CEREMONY_BLOCKS;

	mod epoch {
		use super::*;
		use crate::tests::network::{setup_account, setup_peer_mapping};
		use cf_traits::{
			AuctionPhase, AuctionResult, ChainflipAccount, ChainflipAccountState,
			ChainflipAccountStore, EpochInfo,
		};
		use state_chain_runtime::{Auction, HeartbeatBlockInterval, Validator};

		#[test]
		// We have a test network which goes into the first epoch
		// The auction fails as the stakers are offline and we fail at `WaitingForBids`
		// We require that a network has a minimum of 5 nodes.  We have a network of 8(3 from
		// genesis and 5 new bidders).  We knock 4 of these nodes offline.
		// A new auction is started
		// This continues until we have a new set
		fn auction_repeats_after_failure_because_of_liveness() {
			const EPOCH_BLOCKS: BlockNumber = 100;
			super::genesis::default()
				.blocks_per_epoch(EPOCH_BLOCKS)
				.min_validators(5)
				.build()
				.execute_with(|| {
					// A network with a set of passive nodes
					let (mut testnet, nodes) =
						network::Network::create(8, &Validator::current_validators());

					// All nodes stake to be included in the next epoch which are witnessed on the
					// state chain
					for node in &nodes {
						testnet
							.stake_manager_contract
							.stake(node.clone(), genesis::GENESIS_BALANCE + 1);
					}

					// Set the first 4 nodes offline
					let offline_nodes: Vec<_> = nodes.iter().take(4).cloned().collect();

					for node in &offline_nodes {
						testnet.set_active(node, false);
					}

					// Run to the next epoch to start the auction
					testnet.move_to_next_epoch(EPOCH_BLOCKS);
					// Move to start of auction
					testnet.move_forward_blocks(1);
					assert_eq!(
						Auction::current_auction_index(),
						1,
						"we should have ran an auction"
					);

					assert_eq!(
						Auction::current_phase(),
						AuctionPhase::default(),
						"we should be back at the start"
					);

					// Next block, another auction
					testnet.move_forward_blocks(1);

					assert_eq!(
						Auction::current_auction_index(),
						2,
						"we should have ran another auction"
					);

					assert_eq!(
						Auction::current_phase(),
						AuctionPhase::default(),
						"we should be back at the start"
					);

					for node in &offline_nodes {
						testnet.set_active(node, true);
					}

					assert_eq!(GENESIS_EPOCH, Validator::epoch_index());

					// Move forward heartbeat to get those missing nodes online
					testnet.move_forward_blocks(HeartbeatBlockInterval::get());

					assert!(
						Auction::current_auction_index() > 2,
						"we should have ran several auctions"
					);

					assert_eq!(2, Validator::epoch_index());
				});
		}

		#[test]
		// An epoch has completed.  We have a genesis where the blocks per epoch are
		// set to 100
		// - When the epoch is reached an auction is started and completed
		// - All nodes stake above the MAB
		// - A new auction index has been generated
		// - We have two nodes that haven't registered their session keys
		// - New validators have the state of Validator with the last active epoch stored
		// - Nodes without keys state remains passive with `None` as their last active epoch
		fn epoch_rotates() {
			const EPOCH_BLOCKS: BlockNumber = 100;
			const ACTIVE_SET_SIZE: u32 = 5;
			super::genesis::default()
				.blocks_per_epoch(EPOCH_BLOCKS)
				.max_validators(ACTIVE_SET_SIZE)
				.build()
				.execute_with(|| {
					// A network with a set of passive nodes
					let (mut testnet, nodes) = network::Network::create(
						ACTIVE_SET_SIZE as u8,
						&Validator::current_validators(),
					);
					// Add two nodes which don't have session keys
					let keyless_nodes = vec![testnet.create_node(), testnet.create_node()];
					// All nodes stake to be included in the next epoch which are witnessed on the
					// state chain
					let stake_amount = genesis::GENESIS_BALANCE + 1;
					for node in &nodes {
						testnet.stake_manager_contract.stake(node.clone(), stake_amount);
					}
					// Our keyless nodes also stake
					for keyless_node in &keyless_nodes {
						testnet.stake_manager_contract.stake(keyless_node.clone(), stake_amount);
					}

					// A late staker which we will use after the auction.  They are yet to stake
					// and will do after the auction with the intention of being a backup validator
					let late_staker = testnet.create_node();
					testnet.set_active(&late_staker, true);
					let seed = late_staker.to_string();
					setup_account(&late_staker, &seed);
					setup_peer_mapping(&late_staker, &seed);

					// Run to the next epoch to start the auction
					testnet.move_forward_blocks(EPOCH_BLOCKS);
					// We should be in auction 1
					assert_eq!(
						Auction::current_auction_index(),
						1,
						"this should be the first auction"
					);

					// In this block we should have reached the state `ValidatorsSelected`
					// and in this group we would have in this network the genesis validators and
					// the nodes that have staked as well
					assert_matches::assert_matches!(
						Auction::current_phase(),
						AuctionPhase::ValidatorsSelected(mut candidates, _) => {
							candidates.sort();
							assert_eq!(candidates, nodes);
						},
						"the new candidates should be those genesis validators and the new nodes created in test"
					);
					// For each subsequent block the state chain will check if the vault has rotated
					// until then we stay in the `ValidatorsSelected`
					// Run things to a successful vault rotation
					testnet.move_forward_blocks(VAULT_ROTATION_BLOCKS);
					// The vault rotation should have proceeded and we should now be back
					// at `WaitingForBids` with a new set of winners; the genesis validators and
					// the new nodes we staked into the network
					assert_matches::assert_matches!(
						Auction::current_phase(),
						AuctionPhase::WaitingForBids,
						"we should be back waiting for bids after a successful auction and rotation"
					);

					assert_eq!(
						GENESIS_EPOCH + 1,
						Validator::epoch_index(),
						"We should be in the next epoch"
					);

					let AuctionResult { mut winners, minimum_active_bid } =
						Auction::last_auction_result().expect("last auction result");

					assert_eq!(
						minimum_active_bid, stake_amount,
						"minimum active bid should be that of the new stake"
					);

					winners.sort();
					assert_eq!(
						winners,
						nodes,
						"the new winners should be those genesis validators and the new nodes created in test"
					);

					let mut new_validators = Validator::current_validators();
					new_validators.sort();

					// This new set of winners should also be the validators of the network
					assert_eq!(
						new_validators,
						nodes,
						"the new validators should be those genesis validators and the new nodes created in test"
					);

					for account in keyless_nodes.iter() {
						assert_eq!(
							None,
							ChainflipAccountStore::<Runtime>::get(account).last_active_epoch,
							"this node should have never been active"
						);
						assert_eq!(
							ChainflipAccountState::Passive,
							ChainflipAccountStore::<Runtime>::get(account).state,
							"should be a passive node"
						);
					}

					let current_epoch = Validator::epoch_index();
					for account in new_validators.iter() {
						assert_eq!(
							Some(current_epoch),
							ChainflipAccountStore::<Runtime>::get(account).last_active_epoch,
							"validator should have been active in current epoch"
						);
						assert_eq!(
							ChainflipAccountState::Validator,
							ChainflipAccountStore::<Runtime>::get(account).state,
							"should be validator"
						);
					}

					// A late staker comes along, they should become a backup validator as they have
					// everything in place
					testnet.stake_manager_contract.stake(late_staker.clone(), stake_amount);
					testnet.move_forward_blocks(1);
					assert_eq!(
						ChainflipAccountState::Backup,
						ChainflipAccountStore::<Runtime>::get(&late_staker).state,
						"late staker should be a backup validator"
					);

					// Run to the next epoch to start the auction
					testnet.move_forward_blocks(EPOCH_BLOCKS);
					testnet.move_forward_blocks(VAULT_ROTATION_BLOCKS);
					assert_eq!(
						GENESIS_EPOCH + 2,
						Validator::epoch_index(),
						"We should be in the next epoch"
					);
				});
		}
	}

	mod staking {
		use super::{genesis, network, *};
		use cf_traits::EpochInfo;
		use pallet_cf_staking::pallet::Error;
		#[test]
		// Stakers cannot unstake during the conclusion of the auction
		// We have a set of nodes that are staked and that are included in the auction
		// Moving block by block of an auction we shouldn't be able to claim stake
		fn cannot_claim_stake_during_auction() {
			const EPOCH_BLOCKS: u32 = 100;
			const MAX_VALIDATORS: u32 = 3;
			super::genesis::default()
				.blocks_per_epoch(EPOCH_BLOCKS)
				.max_validators(MAX_VALIDATORS)
				.build()
				.execute_with(|| {
					// Create the test network with some fresh nodes and the genesis validators
					let (mut testnet, nodes) = network::Network::create(
						MAX_VALIDATORS as u8,
						&Validator::current_validators(),
					);
					// Stake these nodes so that they are included in the next epoch
					let stake_amount = genesis::GENESIS_BALANCE;
					for node in &nodes {
						testnet.stake_manager_contract.stake(node.clone(), stake_amount);
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
							1,
							ETH_ZERO_ADDRESS
						));
					}

					// Move to new epoch
					testnet.move_to_next_epoch(EPOCH_BLOCKS);
					// Start auction
					testnet.move_forward_blocks(1);

					assert_eq!(
						Auction::current_auction_index(),
						1,
						"this should be the first auction"
					);

					// We will try to claim some stake
					for node in &nodes {
						assert_noop!(
							Staking::claim(
								Origin::signed(node.clone()),
								stake_amount,
								ETH_ZERO_ADDRESS
							),
							Error::<Runtime>::NoClaimsDuringAuctionPhase
						);
					}

					assert_eq!(
						1,
						Validator::epoch_index(),
						"We should still be in the first epoch"
					);

					// Run things to a successful vault rotation
					testnet.move_forward_blocks(VAULT_ROTATION_BLOCKS);

					assert_eq!(2, Validator::epoch_index(), "We should still be in the new epoch");

					// We should be able to claim again outside of the auction
					// At the moment we have a pending claim so we would expect an error here for
					// this.
					// TODO implement Claims in Contract/Network
					for node in &nodes {
						assert_noop!(
							Staking::claim(Origin::signed(node.clone()), 1, ETH_ZERO_ADDRESS),
							Error::<Runtime>::PendingClaim
						);
					}
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
				let call: state_chain_runtime::Call = frame_system::Call::remark(vec![]).into();
				let gov_call: state_chain_runtime::Call =
					pallet_cf_governance::Call::approve(1).into();
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

	mod validators {
		use crate::tests::{genesis, network, NodeId, GENESIS_EPOCH, VAULT_ROTATION_BLOCKS};
		use cf_traits::{ChainflipAccountState, EpochInfo, FlipBalance, IsOnline, StakeTransfer};
		use pallet_cf_validator::PercentageRange;
		use state_chain_runtime::{
			Auction, EmergencyRotationPercentageRange, Flip, HeartbeatBlockInterval, Online,
			Validator,
		};
		use std::collections::HashMap;

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
			const BACKUP_VALDATORS: u32 = genesis::NUMBER_OF_VALIDATORS;
			super::genesis::default()
				.blocks_per_epoch(EPOCH_BLOCKS)
				.max_validators(MAX_VALIDATORS)
				.build()
				.execute_with(|| {
					// Create MAX_VALIDATORS nodes and stake them above our genesis validators
					// The result will be our newly created nodes will be validators and the
					// genesis validators will become backup validators
					let mut genesis_validators = Validator::current_validators();
					let (mut testnet, _) = network::Network::create(
						(MAX_VALIDATORS + BACKUP_VALDATORS) as u8,
						&genesis_validators.clone(),
					);

					let mut passive_nodes = testnet.filter_nodes(ChainflipAccountState::Passive);
					// An initial stake which is superior to the genesis stakes
					// The current validators would have been rewarded on us leaving the current
					// epoch so let's up the stakes for the passive nodes.
					const INITIAL_STAKE: FlipBalance = genesis::GENESIS_BALANCE * 2;
					// Stake these passive nodes so that they are included in the next epoch
					for node in &passive_nodes {
						testnet.stake_manager_contract.stake(node.clone(), INITIAL_STAKE);
					}

					// Start an auction
					testnet.move_forward_blocks(EPOCH_BLOCKS);

					assert_eq!(
						1,
						Validator::epoch_index(),
						"We should still be in the genesis epoch"
					);

					assert_eq!(
						Auction::current_auction_index(),
						1,
						"this should be the first auction"
					);

					// Run things to a successful vault rotation
					testnet.move_forward_blocks(VAULT_ROTATION_BLOCKS);
					assert_eq!(
						GENESIS_EPOCH + 1,
						Validator::epoch_index(),
						"We should be in a new epoch"
					);

					// assert list of validators as being the new nodes
					let mut current_validators: Vec<NodeId> = Validator::current_validators();

					current_validators.sort();
					passive_nodes.sort();

					assert_eq!(
						passive_nodes, current_validators,
						"our new testnet nodes should be the new validators"
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

					let backup_validator_balances: HashMap<NodeId, FlipBalance> =
						current_backup_validators
							.iter()
							.map(|validator_id| {
								(validator_id.clone(), Flip::stakeable_balance(&validator_id))
							})
							.collect::<Vec<(NodeId, FlipBalance)>>()
							.into_iter()
							.collect();

					// Move forward a heartbeat, emissions should be shared to backup validators
					testnet.move_forward_blocks(HeartbeatBlockInterval::get());

					// We won't calculate the exact emissions but they should be greater than their
					// initial stake
					for (backup_validator, pre_balance) in backup_validator_balances {
						assert!(pre_balance < Flip::stakeable_balance(&backup_validator));
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
			// In order to do this we would want to have missed 1 heartbeat interval
			// Blocks for our epoch, something larger than one heartbeat
			const EPOCH_BLOCKS: u32 = HeartbeatBlockInterval::get() * 2;
			// Reduce our validating set and hence the number of nodes we need to have a backup
			// set to speed the test up
			const MAX_VALIDATORS: u32 = 10;
			super::genesis::default()
				.blocks_per_epoch(EPOCH_BLOCKS)
				.max_validators(MAX_VALIDATORS)
				.build()
				.execute_with(|| {
					let (mut testnet, nodes) = network::Network::create(
						MAX_VALIDATORS as u8,
						&Validator::current_validators(),
					);
					// An initial stake which is superior to the genesis stakes
					const INITIAL_STAKE: FlipBalance = genesis::GENESIS_BALANCE + 1;
					// Stake these nodes so that they are included in the next epoch
					for node in &nodes {
						testnet.stake_manager_contract.stake(node.clone(), INITIAL_STAKE);
					}

					assert_eq!(
						1,
						Validator::epoch_index(),
						"We should still be in the first epoch"
					);

					// Start an auction and wait for rotation
					testnet.move_forward_blocks(EPOCH_BLOCKS);
					testnet.move_forward_blocks(VAULT_ROTATION_BLOCKS);
					assert_eq!(
						GENESIS_EPOCH + 1,
						Validator::epoch_index(),
						"We should be in the next epoch"
					);

					let PercentageRange { top, bottom: _ } =
						EmergencyRotationPercentageRange::get();
					let percentage_top_offline = 100 - top as u32;
					let number_offline = (MAX_VALIDATORS * percentage_top_offline / 100) as usize;

					let offline_nodes: Vec<_> =
						nodes.iter().take(number_offline).cloned().collect();

					for node in &offline_nodes {
						testnet.set_active(node, false);
					}

					// We need to move forward one heartbeat interval to be regarded as offline
					testnet.move_to_next_heartbeat_interval();

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

					assert_eq!(
						GENESIS_EPOCH + 1,
						Validator::epoch_index(),
						"We should be in the same epoch"
					);

					// The next block should see an auction started
					testnet.move_forward_blocks(1);

					assert_eq!(
						Auction::current_auction_index(),
						2,
						"this should be the second auction"
					);

					// Run things to a successful vault rotation
					testnet.move_forward_blocks(VAULT_ROTATION_BLOCKS);
					assert_eq!(
						GENESIS_EPOCH + 2,
						Validator::epoch_index(),
						"We should be in the next epoch"
					);

					// Emergency state reset
					assert!(
						!Validator::emergency_rotation_requested(),
						"we should have had the state of emergency reset"
					);

					for node in &nodes {
						testnet.set_active(node, false);
					}

					testnet.move_to_next_heartbeat_interval();

					// We should have a set of nodes offline
					for node in &nodes {
						assert_eq!(false, Online::is_online(node), "the node should be offline");
					}

					assert!(
						!Validator::emergency_rotation_requested(),
						"we should *not* have requested an emergency rotation"
					);
				});
		}
	}
}
