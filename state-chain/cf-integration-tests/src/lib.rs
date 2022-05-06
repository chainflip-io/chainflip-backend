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
		chainflip::Offence, constants::common::*, opaque::SessionKeys, AccountId, Auction,
		Emissions, EthereumVault, Flip, Governance, Online, Origin, Reputation, Runtime, Session,
		Staking, System, Timestamp, Validator,
	};

	use cf_traits::{AuthorityCount, BlockNumber, EpochIndex, FlipBalance, IsOnline};
	use libsecp256k1::SecretKey;
	use pallet_cf_staking::{EthTransactionHash, EthereumAddress};
	use rand::{prelude::*, SeedableRng};
	use sp_runtime::AccountId32;

	type NodeId = AccountId32;
	const ETH_DUMMY_ADDR: EthereumAddress = [42u8; 20];
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
		use pallet_cf_staking::AccountRetired;
		use pallet_cf_vaults::KeygenOutcome;
		use state_chain_runtime::{Event, HeartbeatBlockInterval, Origin};
		use std::{cell::RefCell, collections::HashMap, rc::Rc};

		// TODO: Can we use the actual events here?
		// Events from ethereum contract
		#[derive(Debug, Clone)]
		pub enum ContractEvent {
			Staked { node_id: NodeId, amount: FlipBalance, total: FlipBalance, epoch: EpochIndex },
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
			// Stake for NODE
			pub fn stake(&mut self, node_id: NodeId, amount: FlipBalance, epoch: EpochIndex) {
				let current_amount = self.stakes.get(&node_id).unwrap_or(&0);
				let total = current_amount + amount;
				self.stakes.insert(node_id.clone(), total);

				self.events.push(ContractEvent::Staked { node_id, amount, total, epoch });
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

		// Representation of the state-chain cmd tool
		pub struct Cli;

		impl Cli {
			// Activates an account to become an authority in the next epoch
			pub fn activate_account(account: NodeId) {
				AccountRetired::<Runtime>::insert(account, false);
			}
		}

		pub struct ThresholdSigner {
			agg_secret_key: SecretKey,
			signatures: HashMap<cf_chains::eth::H256, [u8; 32]>,
			key_seed: u64,
			proposed_seed: Option<u64>,
		}

		impl Default for ThresholdSigner {
			fn default() -> Self {
				let key_seed = GENESIS_KEY;
				let (agg_secret_key, _) = Self::generate_keypair(key_seed);
				ThresholdSigner {
					agg_secret_key,
					signatures: HashMap::new(),
					key_seed,
					proposed_seed: None,
				}
			}
		}

		impl ThresholdSigner {
			// Sign message with current key, caches signatures
			pub fn sign(
				&mut self,
				message: &cf_chains::eth::H256,
			) -> SchnorrVerificationComponents {
				// A nonce, k
				let (k, k_times_g) = Self::generate_keypair(self.key_seed * 2);
				// k.G
				let k_times_g_address = to_ethereum_address(k_times_g);
				// If this message has been signed before return from cache else sign and cache
				return match self.signatures.get(message) {
					Some(signature) =>
						SchnorrVerificationComponents { s: *signature, k_times_g_address },
					None => {
						let agg_key =
							AggKey::from_private_key_bytes(self.agg_secret_key.serialize());
						let signature = agg_key.sign(&(*message).into(), &self.agg_secret_key, &k);

						self.signatures.insert(*message, signature);

						SchnorrVerificationComponents { s: signature, k_times_g_address }
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
			// conveniently creates a threshold "signature" (not really)
			// all engines have the same one, so they create the same sig
			pub threshold_signer: Rc<RefCell<ThresholdSigner>>,
			pub engine_state: EngineState,
		}

		impl Engine {
			fn new(node_id: NodeId, signer: Rc<RefCell<ThresholdSigner>>) -> Self {
				Engine {
					node_id,
					active: true,
					threshold_signer: signer,
					engine_state: EngineState::None,
				}
			}

			fn state(&self) -> ChainflipAccountState {
				ChainflipAccountStore::<Runtime>::get(&self.node_id).state
			}

			// Handle events from contract
			fn on_contract_event(&self, event: &ContractEvent) {
				if self.state() == ChainflipAccountState::CurrentAuthority && self.active {
					match event {
						ContractEvent::Staked { node_id: validator_id, amount, epoch, .. } => {
							// Witness event -> send transaction to state chain
							state_chain_runtime::Witnesser::witness_at_epoch(
								Origin::signed(self.node_id.clone()),
								Box::new(
									pallet_cf_staking::Call::staked(
										validator_id.clone(),
										*amount,
										ETH_ZERO_ADDRESS,
										TX_HASH,
									)
									.into(),
								),
								*epoch,
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
					// Being a CurrentAuthority we would respond to certain events
					if self.state() == ChainflipAccountState::CurrentAuthority {
						on_events!(
							events,
							Event::Validator(
								// A new epoch
								pallet_cf_validator::Event::NewEpoch(_epoch_index)) => {
									(&*self.threshold_signer).borrow_mut().rotate_keys();
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
									state_chain_runtime::EthereumThresholdSigner::signature_success(
										Origin::none(),
										*ceremony_id,
										// Sign with current key
										(&*self.threshold_signer).borrow_mut().sign(payload),
									).expect("should be able to submit threshold signature for Ethereum");
								} };
							},
							Event::EthereumThresholdSigner(
								// A threshold has been met for this signature
								pallet_cf_threshold_signature::Event::ThresholdDispatchComplete(..)) => {
									if let EngineState::Rotation = self.engine_state {
										// If we rotating let's witness the keys being rotated on the contract
										state_chain_runtime::Witnesser::witness(
											Origin::signed(self.node_id.clone()),
											Box::new(pallet_cf_vaults::Call::vault_key_rotated(
												(&*self.threshold_signer).borrow_mut().proposed_public_key(),
												100,
												[1u8; 32].into(),
											).into()),
										).expect("should be able to vault key rotation for node");
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
							pallet_cf_vaults::Event::KeygenRequest(ceremony_id, authorities)) => {
								if authorities.contains(&self.node_id) {
									state_chain_runtime::EthereumVault::report_keygen_outcome(
										Origin::signed(self.node_id.clone()),
										*ceremony_id,
										// Propose a new key
										KeygenOutcome::Success((&*self.threshold_signer).borrow_mut().propose_new_public_key()),
									).unwrap_or_else(|_| panic!("should be able to report keygen outcome from node: {}", self.node_id));
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
						let _result = Online::heartbeat(state_chain_runtime::Origin::signed(
							self.node_id.clone(),
						));
					}
				}
			}
		}

		pub(crate) fn setup_account_and_peer_mapping(node_id: &NodeId, seed: &str) {
			setup_account(node_id, seed);
			setup_peer_mapping(node_id, seed);
		}

		// Create an account, generate and register the session keys
		pub(crate) fn setup_account(node_id: &NodeId, seed: &str) {
			assert_ok!(frame_system::Provider::<Runtime>::created(node_id));

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

		pub(crate) fn setup_peer_mapping(node_id: &NodeId, seed: &str) {
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

			// Used to initialised the threshold signers of the engines added
			pub threshold_signer: Rc<RefCell<ThresholdSigner>>,
		}

		impl Network {
			pub fn next_node_id(&mut self) -> NodeId {
				self.node_counter += 1;
				[self.node_counter as u8; 32].into()
			}

			// Create a network which includes the authorities in genesis of number of nodes
			// and return a network and sorted list of nodes within
			pub fn create(
				number_of_passive_nodes: u8,
				existing_nodes: &[NodeId],
			) -> (Self, Vec<NodeId>) {
				let mut network: Network = Default::default();

				// Include any nodes already *created* to the test network
				for node in existing_nodes {
					network.add_engine(node);
					// Only need to setup peer mapping as the AccountInfo is already set up if they
					// are genesis nodes
					setup_peer_mapping(node, &node.clone().to_string());
				}

				// Create the passive nodes
				let mut passive_nodes = Vec::new();
				for _ in 0..number_of_passive_nodes {
					let node_id = network.next_node_id();
					passive_nodes.push(node_id.clone());
					let seed = node_id.clone().to_string();
					setup_account_and_peer_mapping(&node_id, &seed);
					network.engines.insert(
						node_id.clone(),
						Engine::new(node_id, network.threshold_signer.clone()),
					);
				}

				(network, passive_nodes)
			}

			pub fn set_active(&mut self, node_id: &NodeId, active: bool) {
				self.engines.get_mut(node_id).expect("valid node_id").active = active;
			}

			pub fn create_engine(&mut self) -> NodeId {
				let node_id = self.next_node_id();
				self.add_engine(&node_id);
				node_id
			}

			// TODO: This seems like a pointless abstraction
			// Adds an engine to the test network
			pub fn add_engine(&mut self, node_id: &NodeId) {
				self.engines.insert(
					node_id.clone(),
					Engine::new(node_id.clone(), self.threshold_signer.clone()),
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
						for engine in self.engines.values() {
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
					for engine in self.engines.values_mut() {
						engine.handle_state_chain_events(&events);
					}

					// A completed block notification
					for engine in self.engines.values() {
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
		root: AccountId,
		blocks_per_epoch: BlockNumber,
		max_authorities: AuthorityCount,
		min_authorities: AuthorityCount,
	}

	impl Default for ExtBuilder {
		fn default() -> Self {
			Self {
				accounts: vec![],
				root: AccountId::default(),
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
			self.root = root;
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

		fn configure_storages(&self, storage: &mut Storage) {
			pallet_cf_flip::GenesisConfig::<Runtime> { total_issuance: TOTAL_ISSUANCE }
				.assimilate_storage(storage)
				.unwrap();

			pallet_cf_staking::GenesisConfig::<Runtime> {
				genesis_stakers: self.accounts.clone(),
				minimum_stake: MIN_STAKE,
				claim_ttl: core::time::Duration::from_secs(3 * CLAIM_DELAY),
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

			GenesisBuild::<Runtime>::assimilate_storage(
				&pallet_cf_auction::GenesisConfig {
					authority_set_size_range: (self.min_authorities, self.max_authorities),
				},
				storage,
			)
			.unwrap();

			GenesisBuild::<Runtime>::assimilate_storage(
				&pallet_cf_emissions::GenesisConfig {
					current_authority_emission_inflation: CURRENT_AUTHORITY_EMISSION_INFLATION_BPS,
					backup_node_emission_inflation: BACKUP_NODE_EMISSION_INFLATION_BPS,
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
				penalties: vec![(Offence::MissedHeartbeat, (15, 150))],
			}
			.assimilate_storage(storage)
			.unwrap();

			pallet_cf_validator::GenesisConfig::<Runtime> {
				blocks_per_epoch: self.blocks_per_epoch,
				// TODO Fix this
				bond: self.accounts[0].1,
				claim_period_as_percentage: PERCENT_OF_EPOCH_PERIOD_CLAIMABLE,
			}
			.assimilate_storage(storage)
			.unwrap();

			let (_, public_key) = network::ThresholdSigner::generate_keypair(GENESIS_KEY);
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
			ChainflipAccount, ChainflipAccountState, ChainflipAccountStore, EpochInfo,
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
		// The following state is to be expected at genesis
		// - Total issuance
		// - The genesis authorities are all staked equally
		// - The minimum active bid is set at the stake for a genesis authority
		// - The genesis authorities are available via authority_lookup()
		// - The genesis authorities are in the session
		// - The genesis authorities are considered offline for this heartbeat interval
		// - No emissions have been made
		// - No rewards have been distributed
		// - No vault rotation has occurred
		// - Relevant nonce are at 0
		// - Governance has its member
		// - There have been no proposals
		// - Emission inflation for both authorities and backup authorities are set
		// - No one has reputation
		// - The genesis authorities have last active epoch set
		fn state_of_genesis_is_as_expected() {
			default().build().execute_with(|| {
				// Confirmation that we have our assumed state at block 1
				assert_eq!(
					Flip::total_issuance(),
					TOTAL_ISSUANCE,
					"we have issued the total issuance"
				);

				let accounts =
					[AccountId::from(CHARLIE), AccountId::from(BOB), AccountId::from(ALICE)];

				for account in accounts.iter() {
					assert_eq!(
						Flip::stakeable_balance(account),
						GENESIS_BALANCE,
						"the account has its stake"
					);
				}

				assert_eq!(Validator::bond(), GENESIS_BALANCE);
				let mut authorities = Validator::current_authorities();
				authorities.sort();
				assert_eq!(authorities, accounts, "the authorities are those expected at genesis");

				assert_eq!(
					Validator::epoch_number_of_blocks(),
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
					assert!(!Online::is_online(account), "node should have not sent a heartbeat");
				}

				assert_eq!(Emissions::last_mint_block(), 0, "no emissions");

				assert_eq!(Validator::ceremony_id_counter(), 0, "no key generation requests");

				assert_eq!(
					pallet_cf_environment::GlobalSignatureNonce::<Runtime>::get(),
					0,
					"Global signature nonce should be 0"
				);

				assert!(
					Governance::members().contains(&AccountId::from(ERIN)),
					"expected governor"
				);
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
		use super::{genesis::GENESIS_BALANCE, *};
		use crate::tests::network::setup_account_and_peer_mapping;
		use cf_traits::{
			BackupOrPassive, ChainflipAccount, ChainflipAccountState, ChainflipAccountStore,
			EpochInfo,
		};
		use pallet_cf_validator::RotationStatus;
		use state_chain_runtime::{HeartbeatBlockInterval, Validator};

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
					let (mut testnet, mut passive_nodes) = network::Network::create(3, &nodes);

					nodes.append(&mut passive_nodes);

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
					}

					// Run to the next epoch to start the auction
					testnet.move_to_next_epoch(EPOCH_BLOCKS);

					// Move to start of auction
					testnet.move_forward_blocks(1);

					assert_eq!(Validator::rotation_phase(), RotationStatus::RunAuction);

					// Next block, another auction
					testnet.move_forward_blocks(1);

					assert_eq!(Validator::rotation_phase(), RotationStatus::RunAuction);

					for node in &offline_nodes {
						testnet.set_active(node, true);
					}

					assert_eq!(GENESIS_EPOCH, Validator::epoch_index());

					// Move forward heartbeat to get those missing nodes online
					testnet.move_forward_blocks(HeartbeatBlockInterval::get());

					// The rotation can now continue to the next phase.
					assert!(matches!(
						Validator::rotation_phase(),
						RotationStatus::AwaitingVaults(..)
					));
				});
		}

		#[test]
		// An epoch has completed.  We have a genesis where the blocks per epoch are
		// set to 100
		// - When the epoch is reached an auction is started and completed
		// - All nodes stake above the MAB
		// - We have two nodes that haven't registered their session keys
		// - New authorities have the state of Validator with the last active epoch stored
		// - Nodes without keys state remains passive with `None` as their last active epoch
		fn epoch_rotates() {
			const EPOCH_BLOCKS: BlockNumber = 100;
			const MAX_SET_SIZE: AuthorityCount = 5;
			super::genesis::default()
				.blocks_per_epoch(EPOCH_BLOCKS)
				.min_authorities(MAX_SET_SIZE)
				.build()
				.execute_with(|| {
					// Genesis nodes
					let mut nodes = Validator::current_authorities();

					let number_of_passive_nodes = MAX_SET_SIZE
						.checked_sub(nodes.len() as AuthorityCount)
						.expect("Max set size must be at least the number of genesis authorities");

					let (mut testnet, mut passive_nodes) =
						network::Network::create(number_of_passive_nodes as u8, &nodes);

					// Activate the passiv nodes
					for node in &passive_nodes {
						network::Cli::activate_account(node.clone());
					}

					nodes.append(&mut passive_nodes);
					assert_eq!(nodes.len() as AuthorityCount, MAX_SET_SIZE);
					// All nodes stake to be included in the next epoch which are witnessed on the
					// state chain
					let stake_amount = genesis::GENESIS_BALANCE + 1;
					for node in &nodes {
						testnet.stake_manager_contract.stake(
							node.clone(),
							stake_amount,
							GENESIS_EPOCH,
						);
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
					let seed = late_staker.to_string();
					setup_account_and_peer_mapping(&late_staker, &seed);

					// Run to the next epoch to start the auction
					testnet.move_forward_blocks(EPOCH_BLOCKS);

					assert_eq!(Validator::rotation_phase(), RotationStatus::RunAuction);

					testnet.move_forward_blocks(VAULT_ROTATION_BLOCKS);

					assert_eq!(
						GENESIS_EPOCH + 1,
						Validator::epoch_index(),
						"We should be in the next epoch"
					);

					assert_eq!(
						Validator::bond(),
						stake_amount,
						"minimum active bid should be that of the new stake"
					);

					let mut winners = Validator::current_authorities();
					winners.sort();
					nodes.sort();
					assert_eq!(
						winners,
						nodes,
						"the new winners should be those genesis authorities and the passive nodes that have keys"
					);

					let mut new_authorities = Validator::current_authorities();
					new_authorities.sort();

					// This new set of winners should also be the authorities of the network
					assert_eq!(
						new_authorities,
						nodes,
						"the new authorities should be those genesis authorities and the new nodes created in test"
					);

					for account in keyless_nodes.iter() {
						// TODO: Check historical epochs
						assert_eq!(
							ChainflipAccountState::BackupOrPassive(BackupOrPassive::Passive),
							ChainflipAccountStore::<Runtime>::get(account).state,
							"should be a passive node"
						);
					}

					for account in new_authorities.iter() {
						// TODO: Check historical epochs
						assert_eq!(
							ChainflipAccountState::CurrentAuthority,
							ChainflipAccountStore::<Runtime>::get(account).state,
							"should be CurrentAuthority"
						);
					}

					// A late staker comes along, they should become a backup node as they have
					// everything in place
					testnet.stake_manager_contract.stake(
						late_staker.clone(),
						stake_amount,
						GENESIS_EPOCH + 1,
					);
					testnet.move_forward_blocks(1);
					assert_eq!(
						ChainflipAccountState::BackupOrPassive(BackupOrPassive::Backup),
						ChainflipAccountStore::<Runtime>::get(&late_staker).state,
						"late staker should be a backup node"
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
					let (mut testnet, mut passive_nodes) = network::Network::create(0, &nodes);

					for passive_node in passive_nodes.clone() {
						network::Cli::activate_account(passive_node);
					}

					nodes.append(&mut passive_nodes);

					// Stake these nodes so that they are included in the next epoch
					let stake_amount = genesis::GENESIS_BALANCE;
					for node in &nodes {
						testnet.stake_manager_contract.stake(
							node.clone(),
							stake_amount,
							GENESIS_EPOCH,
						);
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
						assert_ok!(Staking::claim(Origin::signed(node.clone()), 1, ETH_DUMMY_ADDR));
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
								stake_amount,
								ETH_DUMMY_ADDR
							),
							Error::<Runtime>::AuctionPhase
						);
					}

					assert_eq!(
						1,
						Validator::epoch_index(),
						"We should still be in the first epoch"
					);

					// Move to new epoch
					testnet.move_to_next_epoch(EPOCH_BLOCKS);
					testnet.move_forward_blocks(1); // Start auction
								// Run things to a successful vault rotation
					testnet.move_forward_blocks(VAULT_ROTATION_BLOCKS);

					assert_eq!(2, Validator::epoch_index(), "We are in a new epoch");

					// We should be able to claim again outside of the auction
					// At the moment we have a pending claim so we would expect an error here for
					// this.
					// TODO implement Claims in Contract/Network
					for node in &nodes {
						assert_noop!(
							Staking::claim(Origin::signed(node.clone()), 1, ETH_DUMMY_ADDR),
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

	mod authorities {
		use crate::tests::{genesis, network, NodeId, GENESIS_EPOCH, VAULT_ROTATION_BLOCKS};
		use cf_traits::{
			AuthorityCount, BackupOrPassive, ChainflipAccount, ChainflipAccountState,
			ChainflipAccountStore, EpochInfo, FlipBalance, IsOnline, StakeTransfer,
		};
		use pallet_cf_validator::PercentageRange;
		use state_chain_runtime::{
			Auction, EmergencyRotationPercentageRange, Flip, HeartbeatBlockInterval, Online,
			Runtime, Validator,
		};
		use std::collections::HashMap;

		#[test]

		fn genesis_nodes_rotated_out_accumulate_rewards_correctly() {
			// We want to have at least one heartbeat within our reduced epoch
			const EPOCH_BLOCKS: u32 = HeartbeatBlockInterval::get() * 2;
			// Reduce our validating set and hence the number of nodes we need to have a backup
			// set
			const MAX_AUTHORITIES: AuthorityCount = 10;
			super::genesis::default()
				.blocks_per_epoch(EPOCH_BLOCKS)
				.max_authorities(MAX_AUTHORITIES)
				.build()
				.execute_with(|| {
					// Create MAX_AUTHORITIES passive nodes and stake them above our genesis
					// authorities The result will be our newly created nodes will be authorities
					// and the genesis authorities will become backup nodes
					let mut genesis_authorities = Validator::current_authorities();
					let (mut testnet, mut init_passive_nodes) =
						network::Network::create(MAX_AUTHORITIES as u8, &genesis_authorities);

					// An initial stake which is greater than the genesis stakes
					// We intend for these initially passive nodes to win the auction
					const INITIAL_STAKE: FlipBalance = genesis::GENESIS_BALANCE * 2;
					// Stake these passive nodes so that they are included in the next epoch
					for node in &init_passive_nodes {
						testnet.stake_manager_contract.stake(
							node.clone(),
							INITIAL_STAKE,
							GENESIS_EPOCH,
						);
						network::Cli::activate_account(node.clone());
					}

					// Start an auction
					testnet.move_forward_blocks(EPOCH_BLOCKS);

					assert_eq!(
						GENESIS_EPOCH,
						Validator::epoch_index(),
						"We should still be in the genesis epoch"
					);

					// Run things to a successful vault rotation
					testnet.move_forward_blocks(VAULT_ROTATION_BLOCKS);
					assert_eq!(
						GENESIS_EPOCH + 1,
						Validator::epoch_index(),
						"We should be in a new epoch"
					);

					// assert list of authorities as being the new nodes
					let mut current_authorities: Vec<NodeId> = Validator::current_authorities();

					current_authorities.sort();
					init_passive_nodes.sort();

					assert_eq!(
						init_passive_nodes, current_authorities,
						"our new initial passive nodes should be the new authorities"
					);

					current_authorities.iter().for_each(|account_id| {
						let account_data = ChainflipAccountStore::<Runtime>::get(account_id);
						assert_eq!(account_data.state, ChainflipAccountState::CurrentAuthority);
						// we were active in teh first epoch

						// TODO: Check historical epochs
					});

					// assert list of backup nodes as being the genesis authorities
					let mut current_backup_nodes: Vec<NodeId> = Auction::remaining_bidders()
						.iter()
						.take(Auction::backup_group_size() as usize)
						.map(|(validator_id, _)| validator_id.clone())
						.collect();

					current_backup_nodes.sort();
					genesis_authorities.sort();

					assert_eq!(
						genesis_authorities, current_backup_nodes,
						"the genesis authorities should now be the backup nodes"
					);

					current_backup_nodes.iter().for_each(|account_id| {
						let account_data = ChainflipAccountStore::<Runtime>::get(account_id);
						assert_eq!(
							account_data.state,
							ChainflipAccountState::HistoricalAuthority(BackupOrPassive::Backup)
						);
						// we were active in teh first epoch
						// TODO: Check historical epochs
					});

					let backup_node_balances: HashMap<NodeId, FlipBalance> = current_backup_nodes
						.iter()
						.map(|validator_id| {
							(validator_id.clone(), Flip::stakeable_balance(validator_id))
						})
						.collect::<Vec<(NodeId, FlipBalance)>>()
						.into_iter()
						.collect();

					// Move forward a heartbeat, emissions should be shared to backup nodes
					testnet.move_forward_blocks(HeartbeatBlockInterval::get());

					// We won't calculate the exact emissions but they should be greater than their
					// initial stake
					for (backup_node, pre_balance) in backup_node_balances {
						assert!(pre_balance < Flip::stakeable_balance(&backup_node));
					}
				});
		}

		#[test]
		// A network is created with a set of authorities and backup nodes.
		// EmergencyRotationPercentageTrigger(80%) of the authorities continue to submit heartbeats
		// with 20% going offline and forcing an emergency rotation in which a new set of
		// authorities start to validate the network which includes live authorities and previous
		// backup nodes
		fn emergency_rotations() {
			// We want to be able to miss heartbeats to be offline and provoke an emergency rotation
			// In order to do this we would want to have missed 1 heartbeat interval
			// Blocks for our epoch, something larger than one heartbeat
			const EPOCH_BLOCKS: u32 = HeartbeatBlockInterval::get() * 2;
			// Reduce our validating set and hence the number of nodes we need to have a backup
			// set to speed the test up
			const MAX_AUTHORITIES: AuthorityCount = 10;
			super::genesis::default()
				.blocks_per_epoch(EPOCH_BLOCKS)
				.max_authorities(MAX_AUTHORITIES)
				.build()
				.execute_with(|| {
					let mut nodes = Validator::current_authorities();
					let (mut testnet, mut passive_nodes) =
						network::Network::create(MAX_AUTHORITIES as u8, &nodes);

					for passive_node in passive_nodes.clone() {
						network::Cli::activate_account(passive_node);
					}

					nodes.append(&mut passive_nodes);
					// An initial stake which is superior to the genesis stakes
					const INITIAL_STAKE: FlipBalance = genesis::GENESIS_BALANCE + 1;
					// Stake these nodes so that they are included in the next epoch
					for node in &nodes {
						testnet.stake_manager_contract.stake(
							node.clone(),
							INITIAL_STAKE,
							GENESIS_EPOCH,
						);
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
					let number_offline =
						(MAX_AUTHORITIES as u32 * percentage_top_offline / 100) as usize;

					let offline_nodes: Vec<_> =
						nodes.iter().take(number_offline).cloned().collect();

					for node in &offline_nodes {
						testnet.set_active(node, false);
					}

					// We need to move forward one heartbeat interval to be regarded as offline
					testnet.move_to_next_heartbeat_interval();

					// We should have a set of nodes offline
					for node in &offline_nodes {
						assert!(!Online::is_online(node), "the node should be offline");
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
						assert!(!Online::is_online(node), "the node should be offline");
					}

					assert!(
						!Validator::emergency_rotation_requested(),
						"we should *not* have requested an emergency rotation"
					);
				});
		}
	}

	mod bond {
		use super::*;
		use cf_traits::{EpochInfo, HistoricalEpoch, StakeTransfer};
		use frame_system::RawOrigin;
		use pallet_cf_validator::EpochHistory;
		use state_chain_runtime::Validator;

		// TODO: Rename
		// Helper function that checks the epochs of an authority against a list of expected
		// epochs
		fn ensure_epoch_activity(account: &AccountId, epochs: Vec<EpochIndex>) {
			assert_eq!(
				EpochHistory::<Runtime>::active_epochs_for_authority(account),
				epochs,
				"The active epochs for the authority should be {:?}",
				epochs
			);
		}

		// This should be the normal scenario. We define a network with a smaller active set size
		// than nodes. During the test, the nodes bid each other out and expect an increase of the
		// MAB.
		#[test]
		fn ensure_right_bond_during_epoch_tranisition() {
			const EPOCH_BLOCKS: BlockNumber = 100;
			const ACTIVE_SET_SIZE: AuthorityCount = 3;
			const GENESIS_BALANCE: FlipBalance = 1;
			const BOND_EPOCH_2: u128 = 31;
			const BOND_EPOCH_3: u128 = 100;
			super::genesis::default()
				.blocks_per_epoch(EPOCH_BLOCKS)
				.accounts(vec![
					(AccountId::from(ALICE), GENESIS_BALANCE),
					(AccountId::from(BOB), GENESIS_BALANCE),
					(AccountId::from(CHARLIE), GENESIS_BALANCE),
				])
				.max_authorities(ACTIVE_SET_SIZE)
				.build()
				.execute_with(|| {
					assert_eq!(1, Validator::epoch_index(), "We should be in the first epoch");
					let current_authorities = Validator::current_authorities();
					let (mut testnet, passive_nodes) =
						network::Network::create(2, &current_authorities);
					// Define 5 nodes
					let genesis_node_1 = current_authorities.get(0).unwrap();
					let genesis_node_2 = current_authorities.get(1).unwrap();
					let genesis_node_3 = current_authorities.get(2).unwrap();
					let init_passive_node_1 = passive_nodes.get(0).unwrap();
					let init_passive_node_2 = passive_nodes.get(1).unwrap();

					// Activate accounts
					network::Cli::activate_account(init_passive_node_1.clone());
					network::Cli::activate_account(init_passive_node_2.clone());

					// Stake the nodes
					testnet.stake_manager_contract.stake(genesis_node_1.clone(), 99, GENESIS_EPOCH);
					testnet.stake_manager_contract.stake(genesis_node_2.clone(), 50, GENESIS_EPOCH);
					testnet.stake_manager_contract.stake(genesis_node_3.clone(), 30, GENESIS_EPOCH);
					testnet.stake_manager_contract.stake(
						init_passive_node_1.clone(),
						20,
						GENESIS_EPOCH,
					);
					testnet.stake_manager_contract.stake(
						init_passive_node_2.clone(),
						10,
						GENESIS_EPOCH,
					);

					testnet.move_forward_blocks(EPOCH_BLOCKS);
					// TODO: Should we? we don't seem to be given we start in epoch 1
					assert_eq!(1, Validator::epoch_index(), "We should be in the next epoch");
					// Expect the MAB to be the genesis balance
					assert_eq!(1, Validator::bond());

					testnet.move_forward_blocks(EPOCH_BLOCKS);
					assert_eq!(2, Validator::epoch_index(), "We should be in the next epoch");
					// Current epoch bond is 31
					assert_eq!(BOND_EPOCH_2, Validator::bond());

					let current_authorities = Validator::current_authorities();
					// Expect the genesis nodes in the active set, and only them
					assert!(current_authorities.contains(genesis_node_1));
					assert!(current_authorities.contains(genesis_node_2));
					assert!(current_authorities.contains(genesis_node_3));
					assert_eq!(current_authorities.len(), 3);

					// Stake the passive nodes
					testnet.stake_manager_contract.stake(
						init_passive_node_1.clone(),
						100,
						GENESIS_EPOCH + 1,
					);
					testnet.stake_manager_contract.stake(
						init_passive_node_2.clone(),
						100,
						GENESIS_EPOCH + 1,
					);

					testnet.move_forward_blocks(EPOCH_BLOCKS);
					assert_eq!(3, Validator::epoch_index(), "We should be in the next epoch");

					// Bond has increased to 100 after the passive nodes now have stakes of 120, and
					// 110 the 3rd highest genesis node has a stake of 100 (99 + 1)
					assert_eq!(BOND_EPOCH_3, Validator::bond());

					let current_authorities = Validator::current_authorities();
					// Expect 1, 4 and 5 in the active set
					assert!(current_authorities.contains(genesis_node_1));
					assert!(current_authorities.contains(init_passive_node_1));
					assert!(current_authorities.contains(init_passive_node_2));

					// Check activity in epochs
					ensure_epoch_activity(genesis_node_1, vec![2, 3]);
					ensure_epoch_activity(genesis_node_2, vec![2]);
					ensure_epoch_activity(genesis_node_3, vec![2]);
					ensure_epoch_activity(init_passive_node_1, vec![3]);
					ensure_epoch_activity(init_passive_node_2, vec![3]);

					// We expect genesis_node_1 to be bonded for the epoch with the higher bond
					assert_eq!(BOND_EPOCH_3, Flip::locked_balance(genesis_node_1));
					assert_eq!(BOND_EPOCH_2, Flip::locked_balance(genesis_node_2));
					assert_eq!(BOND_EPOCH_2, Flip::locked_balance(genesis_node_3));
					assert_eq!(BOND_EPOCH_3, Flip::locked_balance(init_passive_node_1));
					assert_eq!(BOND_EPOCH_3, Flip::locked_balance(init_passive_node_2));
				});
		}

		// In this scenario, we test the case when the MAB drops from one epoch to another. We
		// expect the authorities to be bonded for the epoch with the highest bond in which they are
		// currently active. To simulate this scenario we have to extend the set size during the
		// test to simulate a drop in the MAB.
		#[test]
		fn decreasing_mab_scenario() {
			const EPOCH_BLOCKS: BlockNumber = 100;
			const ACTIVE_SET_SIZE: AuthorityCount = 3;
			const GENESIS_BALANCE: FlipBalance = 1;
			const BOND_EPOCH_2: u128 = 31;
			const BOND_EPOCH_3: u128 = 6;
			super::genesis::default()
				.blocks_per_epoch(EPOCH_BLOCKS)
				.accounts(vec![
					(AccountId::from(ALICE), GENESIS_BALANCE),
					(AccountId::from(BOB), GENESIS_BALANCE),
					(AccountId::from(CHARLIE), GENESIS_BALANCE),
				])
				.max_authorities(ACTIVE_SET_SIZE)
				.build()
				.execute_with(|| {
					assert_eq!(
						GENESIS_EPOCH,
						Validator::epoch_index(),
						"We should be in the first epoch"
					);
					let current_authorities = &Validator::current_authorities();
					let (mut testnet, passive_nodes) =
						network::Network::create(2, current_authorities);

					// Define 5 nodes
					let genesis_node_1 = current_authorities.get(0).unwrap();
					let genesis_node_2 = current_authorities.get(1).unwrap();
					let genesis_node_3 = current_authorities.get(2).unwrap();
					let init_passive_node_1 = passive_nodes.get(0).unwrap();
					let init_passive_node_2 = passive_nodes.get(1).unwrap();

					// Activate accounts
					network::Cli::activate_account(init_passive_node_1.clone());
					network::Cli::activate_account(init_passive_node_2.clone());

					// Stake a genesis node, and the passive nodes.
					// They should have the highest stake now
					// they are just sorted nodes from the network output function
					testnet.stake_manager_contract.stake(genesis_node_1.clone(), 30, GENESIS_EPOCH);
					testnet.stake_manager_contract.stake(
						init_passive_node_1.clone(),
						50,
						GENESIS_EPOCH,
					);
					testnet.stake_manager_contract.stake(
						init_passive_node_2.clone(),
						100,
						GENESIS_EPOCH,
					);

					testnet.move_forward_blocks(EPOCH_BLOCKS);

					// Is this true? - Why can we move forward, epoch blocks and not have increased
					// an epoch number
					assert_eq!(
						1,
						Validator::epoch_index(),
						"We should still be in the first epoch"
					);
					// Expect the MAB to be the genesis balance
					assert_eq!(1, Validator::bond());

					testnet.move_forward_blocks(EPOCH_BLOCKS);
					assert_eq!(
						GENESIS_EPOCH + 1,
						Validator::epoch_index(),
						"We should be in the next epoch"
					);

					// Current epoch bond is 31
					assert_eq!(BOND_EPOCH_2, Validator::bond());
					let current_authorities = Validator::current_authorities();
					// Expect the staked nodes to be in the active set
					assert!(current_authorities.contains(genesis_node_1));
					assert!(current_authorities.contains(init_passive_node_1));
					assert!(current_authorities.contains(init_passive_node_2));

					// Increase the active set size to simulate an decrease of the MAB
					assert_ok!(Auction::set_current_authority_set_size_range(
						RawOrigin::Root.into(),
						(4, 5),
					));

					// give the genesis nodes some extra stake (bringing their stake to 6
					testnet.stake_manager_contract.stake(
						genesis_node_2.clone(),
						5,
						GENESIS_EPOCH + 1,
					);
					testnet.stake_manager_contract.stake(
						genesis_node_3.clone(),
						5,
						GENESIS_EPOCH + 1,
					);

					testnet.move_forward_blocks(EPOCH_BLOCKS);
					assert_eq!(3, Validator::epoch_index(), "We should be in the next epoch");
					// Bond has decreased from 31 to 6
					assert_eq!(BOND_EPOCH_3, Validator::bond());

					let current_authorities = Validator::current_authorities();
					// Expect all nodes to be in the active set
					assert!(current_authorities.contains(genesis_node_1));
					assert!(current_authorities.contains(genesis_node_2));
					assert!(current_authorities.contains(genesis_node_3));
					assert!(current_authorities.contains(init_passive_node_1));
					assert!(current_authorities.contains(init_passive_node_2));

					// Expect Node 1, 2 and 3 to be active in 2 epochs
					ensure_epoch_activity(genesis_node_1, vec![2, 3]);
					ensure_epoch_activity(init_passive_node_1, vec![2, 3]);
					ensure_epoch_activity(init_passive_node_2, vec![2, 3]);

					// Expect node 3 and 4 to be active in 1 epoch
					ensure_epoch_activity(genesis_node_2, vec![3]);
					ensure_epoch_activity(genesis_node_3, vec![3]);

					// Expect node 1, 2 and 3 to be be bonded for epoch 2
					assert_eq!(BOND_EPOCH_2, Flip::locked_balance(genesis_node_1));
					assert_eq!(BOND_EPOCH_2, Flip::locked_balance(init_passive_node_1));
					assert_eq!(BOND_EPOCH_2, Flip::locked_balance(init_passive_node_2));

					// Expect node 1 and 2 to bonded for epoch 3
					assert_eq!(BOND_EPOCH_3, Flip::locked_balance(genesis_node_2));
					assert_eq!(BOND_EPOCH_3, Flip::locked_balance(genesis_node_3));
				});
		}
	}
}
