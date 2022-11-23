use super::*;
use cf_chains::eth::{to_ethereum_address, AggKey, SchnorrVerificationComponents};
use cf_primitives::{AccountRole, EpochIndex, FlipBalance};
use cf_traits::{AccountRoleRegistry, EpochInfo};
use codec::Encode;
use frame_support::traits::{OnFinalize, OnIdle};
use libsecp256k1::PublicKey;
use pallet_cf_staking::{ClaimAmount, MinimumStake};
use pallet_cf_validator::RotationPhase;
use state_chain_runtime::{AccountRoles, Authorship, EthereumInstance, Event, Origin};
use std::{cell::RefCell, collections::HashMap, rc::Rc};

// arbitrary units of block time
pub const BLOCK_TIME: u64 = 1000;

// TODO: Can we use the actual events here?
// Events from ethereum contract
#[derive(Debug, Clone)]
pub enum ContractEvent {
	Staked { node_id: NodeId, amount: FlipBalance, total: FlipBalance, epoch: EpochIndex },

	Claimed { node_id: NodeId, amount: FlipBalance, epoch: EpochIndex },
}

macro_rules! on_events {
	($events:expr, $( $p:pat => $b:block ),* $(,)?) => {
		for event in $events {
			$(if let $p = event { $b })*
		}
	}
}

pub const NEW_STAKE_AMOUNT: FlipBalance = mock_runtime::MIN_STAKE + 1;

pub fn create_testnet_with_new_staker() -> (Network, AccountId32) {
	let (mut testnet, backup_nodes) = Network::create(1, &Validator::current_authorities());

	let new_backup = backup_nodes.first().unwrap().clone();

	testnet
		.stake_manager_contract
		.stake(new_backup.clone(), NEW_STAKE_AMOUNT, GENESIS_EPOCH);
	// register the stake
	testnet.move_forward_blocks(1);

	(testnet, new_backup)
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
	pub fn stake(&mut self, node_id: NodeId, amount: FlipBalance, epoch: EpochIndex) {
		assert!(amount >= MinimumStake::<Runtime>::get());
		let current_amount = self.stakes.get(&node_id).unwrap_or(&0);
		let total = current_amount + amount;
		self.stakes.insert(node_id.clone(), total);

		self.events.push(ContractEvent::Staked { node_id, amount, total, epoch });
	}

	// We don't really care about the process of "registering" and then "executing" claim here.
	// The only thing the SC cares about is the *execution* of the claim.
	pub fn execute_claim(&mut self, node_id: NodeId, amount: FlipBalance, epoch: EpochIndex) {
		self.events.push(ContractEvent::Claimed { node_id, amount, epoch });
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
	pub fn activate_account(account: &NodeId) {
		assert_ok!(Staking::activate_account(Origin::signed(account.clone())));
	}

	pub fn claim(account: &NodeId, amount: ClaimAmount<FlipBalance>, eth_address: EthereumAddress) {
		assert_ok!(Staking::claim(Origin::signed(account.clone()), amount, eth_address));
	}

	pub fn set_vanity_name(account: &NodeId, name: &str) {
		assert_ok!(Validator::set_vanity_name(
			Origin::signed(account.clone()),
			name.as_bytes().to_vec()
		));
	}

	pub fn register_as_validator(account: &NodeId) {
		assert_ok!(
			<AccountRoles as AccountRoleRegistry<state_chain_runtime::Runtime>>::register_account_role(
				account,
				AccountRole::Validator
			)
		);
	}
}

#[derive(Clone)]
pub struct KeyComponents {
	pub seed: u64,
	pub secret: SecretKey,
	pub agg_key: AggKey,
}

impl KeyComponents {
	fn sign(&self, message: &cf_chains::eth::H256) -> SchnorrVerificationComponents {
		assert_eq!(self.agg_key, AggKey::from_private_key_bytes(self.secret.serialize()));

		// just use the same signature nonce for every ceremony in tests
		let k: [u8; 32] = StdRng::seed_from_u64(200).gen();
		let k = SecretKey::parse(&k).unwrap();
		let signature = self.agg_key.sign(message.as_fixed_bytes(), &self.secret, &k);

		let k_times_g_address = to_ethereum_address(PublicKey::from_secret_key(&k));
		SchnorrVerificationComponents { s: signature, k_times_g_address }
	}
}

pub struct ThresholdSigner {
	key_components: KeyComponents,
	proposed_key_components: Option<KeyComponents>,
}

impl Default for ThresholdSigner {
	fn default() -> Self {
		let (secret, _pub_key, agg_key) = Self::generate_keypair(GENESIS_KEY_SEED);
		ThresholdSigner {
			key_components: KeyComponents { secret, agg_key, seed: GENESIS_KEY_SEED },
			proposed_key_components: None,
		}
	}
}

impl ThresholdSigner {
	pub fn sign_with_key(
		&self,
		key_id: &[u8],
		message: &cf_chains::eth::H256,
	) -> SchnorrVerificationComponents {
		let curr_key_id = self.key_components.agg_key.to_pubkey_compressed();
		if key_id == curr_key_id {
			println!("Signing with current key");
			return self.key_components.sign(message)
		}
		let next_key_id =
			self.proposed_key_components.as_ref().unwrap().agg_key.to_pubkey_compressed();
		if key_id == next_key_id {
			println!("Signing with proposed key");
			self.proposed_key_components.as_ref().unwrap().sign(message)
		} else {
			panic!("Unknown key");
		}
	}

	// Generate a keypair with seed
	pub fn generate_keypair(seed: u64) -> (SecretKey, PublicKey, AggKey) {
		let agg_key_priv: [u8; 32] = StdRng::seed_from_u64(seed).gen();
		let secret_key = SecretKey::parse(&agg_key_priv).unwrap();
		let pub_key = PublicKey::from_secret_key(&secret_key);
		(secret_key, pub_key, AggKey::from_pubkey_compressed(pub_key.serialize_compressed()))
	}

	pub fn proposed_public_key(&self) -> AggKey {
		self.proposed_key_components.as_ref().expect("should have proposed key").agg_key
	}

	pub fn propose_new_public_key(&mut self) -> AggKey {
		let proposed_seed = self.key_components.seed + 1;
		let (secret, _pub_key, agg_key) = Self::generate_keypair(proposed_seed);
		self.proposed_key_components = Some(KeyComponents { secret, agg_key, seed: proposed_seed });
		self.proposed_public_key()
	}

	// Rotate to the current proposed key and clear the proposed key
	pub fn use_proposed_key(&mut self) {
		if self.proposed_key_components.is_some() {
			self.key_components =
				self.proposed_key_components.as_ref().expect("No key has been proposed").clone();
			self.proposed_key_components = None;
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
	pub live: bool,
	// conveniently creates a threshold "signature" (not really)
	// all engines have the same one, so they create the same sig
	pub threshold_signer: Rc<RefCell<ThresholdSigner>>,
	pub engine_state: EngineState,
}

impl Engine {
	fn new(node_id: NodeId, signer: Rc<RefCell<ThresholdSigner>>) -> Self {
		Engine { node_id, live: true, threshold_signer: signer, engine_state: EngineState::None }
	}

	fn state(&self) -> ChainflipAccountState {
		get_validator_state(&self.node_id)
	}

	// Handle events from contract
	fn on_contract_event(&self, event: &ContractEvent) {
		if self.state() == ChainflipAccountState::CurrentAuthority && self.live {
			match event {
				ContractEvent::Staked { node_id: validator_id, amount, epoch, .. } => {
					state_chain_runtime::Witnesser::witness_at_epoch(
						Origin::signed(self.node_id.clone()),
						Box::new(
							pallet_cf_staking::Call::staked {
								account_id: validator_id.clone(),
								amount: *amount,
								withdrawal_address: ETH_ZERO_ADDRESS,
								tx_hash: TX_HASH,
							}
							.into(),
						),
						*epoch,
					)
					.expect("should be able to witness stake for node");
				},
				ContractEvent::Claimed { node_id, amount, epoch } => {
					state_chain_runtime::Witnesser::witness_at_epoch(
						Origin::signed(self.node_id.clone()),
						Box::new(
							pallet_cf_staking::Call::claimed {
								account_id: node_id.clone(),
								claimed_amount: *amount,
								tx_hash: TX_HASH,
							}
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
		if self.live {
			// Being a CurrentAuthority we would respond to certain events
			if self.state() == ChainflipAccountState::CurrentAuthority {
				on_events!(
					events,
					Event::Validator(
						// A new epoch
						pallet_cf_validator::Event::NewEpoch(_epoch_index)) => {
							self.threshold_signer.borrow_mut().use_proposed_key();
					},
					Event::EthereumThresholdSigner(
						// A signature request
						pallet_cf_threshold_signature::Event::ThresholdSignatureRequest{
							request_id: _,
							ceremony_id,
							key_id,
							signatories: _signatories,
							payload}) => {

						// if we unwrap on this, we'll panic, because we will have already succeeded
						// on a previous submission (all nodes submit this)
						let _result = state_chain_runtime::EthereumThresholdSigner::signature_success(
							Origin::none(),
							*ceremony_id,
							self.threshold_signer.borrow().sign_with_key(key_id, payload),
						);
					},
					Event::Validator(
						pallet_cf_validator::Event::RotationPhaseUpdated { new_phase: RotationPhase::ActivatingKeys(_) }) => {
							if let EngineState::Rotation = self.engine_state {
								// If we rotating let's witness the keys being rotated on the contract
								let _result = state_chain_runtime::Witnesser::witness_at_epoch(
									Origin::signed(self.node_id.clone()),
									Box::new(pallet_cf_vaults::Call::<_, EthereumInstance>::vault_key_rotated {
										new_public_key: self.threshold_signer.borrow_mut().proposed_public_key(),
										block_number: 100,
										tx_hash: [1u8; 32].into(),
									}.into()),
									Validator::epoch_index()
								);
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
							self.threshold_signer.borrow_mut().propose_new_public_key();
							let threshold_signer = self.threshold_signer.borrow();
							let proposed_key_components = threshold_signer.proposed_key_components.as_ref().expect("should have propposed key");
							state_chain_runtime::EthereumVault::report_keygen_outcome(
								Origin::signed(self.node_id.clone()),
								*ceremony_id,
								Ok(proposed_key_components.agg_key),
							).unwrap_or_else(|_| panic!("should be able to report keygen outcome from node: {}", self.node_id));
						}
				},
			);
		}
	}
}

/// Do this after staking.
pub(crate) fn setup_account_and_peer_mapping(node_id: &NodeId) {
	setup_account(node_id);
	setup_peer_mapping(node_id);
}

// Create an account, generate and register the session keys
pub(crate) fn setup_account(node_id: &NodeId) {
	let seed = &node_id.clone().to_string();

	assert_ok!(state_chain_runtime::Session::set_keys(
		state_chain_runtime::Origin::signed(node_id.clone()),
		SessionKeys {
			aura: get_from_seed::<AuraId>(seed),
			grandpa: get_from_seed::<GrandpaId>(seed),
		},
		vec![]
	));
}

pub(crate) fn setup_peer_mapping(node_id: &NodeId) {
	let seed = &node_id.clone().to_string();
	let peer_keypair = sp_core::ed25519::Pair::from_legacy_string(seed, None);

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

	pub fn live_nodes(&self) -> Vec<NodeId> {
		self.engines
			.iter()
			.filter_map(|(node_id, engine)| if engine.live { Some(node_id.clone()) } else { None })
			.collect()
	}

	// Create a network which includes the authorities in genesis of number of nodes
	// and return a network and sorted list of nodes within
	pub fn create(number_of_backup_nodes: u8, existing_nodes: &[NodeId]) -> (Self, Vec<NodeId>) {
		let mut network: Network = Default::default();

		// Include any nodes already *created* to the test network
		for node in existing_nodes {
			network.add_engine(node);
			setup_peer_mapping(node);
		}

		// Create the backup nodes
		let mut backup_nodes = Vec::new();
		for _ in 0..number_of_backup_nodes {
			let node_id = network.create_engine();
			backup_nodes.push(node_id);
		}

		(network, backup_nodes)
	}

	pub fn set_active(&mut self, node_id: &NodeId, active: bool) {
		self.engines.get_mut(node_id).expect("valid node_id").live = active;
	}

	pub fn create_engine(&mut self) -> NodeId {
		let node_id = self.next_node_id();
		self.add_engine(&node_id);
		node_id
	}

	// Adds an engine to the test network
	pub fn add_engine(&mut self, node_id: &NodeId) {
		self.engines
			.insert(node_id.clone(), Engine::new(node_id.clone(), self.threshold_signer.clone()));
	}

	pub fn move_to_next_epoch(&mut self) {
		let blocks_per_epoch = Validator::blocks_per_epoch();
		let current_block_number = System::block_number();
		self.move_forward_blocks(blocks_per_epoch - (current_block_number % blocks_per_epoch));
	}

	pub fn submit_heartbeat_all_engines(&self) {
		for engine in self.engines.values() {
			let _result =
				Reputation::heartbeat(state_chain_runtime::Origin::signed(engine.node_id.clone()));
		}
	}

	pub fn move_forward_blocks(&mut self, n: u32) {
		const INIT_TIMESTAMP: u64 = 30_000;
		let current_block_number = System::block_number();
		while System::block_number() < current_block_number + n {
			let block_number = System::block_number() + 1;

			System::initialize(&block_number, &System::block_hash(block_number), &{
				let mut digest = sp_runtime::Digest::default();
				digest.push(sp_runtime::DigestItem::PreRuntime(
					sp_consensus_aura::AURA_ENGINE_ID,
					sp_consensus_aura::Slot::from(block_number as u64).encode(),
				));
				digest
			});

			Timestamp::set_timestamp((block_number as u64 * BLOCK_TIME) + INIT_TIMESTAMP);
			state_chain_runtime::AllPalletsWithoutSystem::on_initialize(block_number);
			// We must finalise this to clear the previous author which is otherwise cached
			Authorship::on_finalize(block_number);

			// Provide very large weight to ensure all on_idle processing can occur
			state_chain_runtime::AllPalletsWithoutSystem::on_idle(block_number, 1_000_000_000_000);

			for event in self.stake_manager_contract.events() {
				for engine in self.engines.values() {
					engine.on_contract_event(&event);
				}
			}

			self.stake_manager_contract.clear();

			let events = frame_system::Pallet::<Runtime>::events()
				.into_iter()
				.map(|e| e.event)
				.skip(self.last_event)
				.collect::<Vec<Event>>();

			self.last_event += events.len();

			for engine in self.engines.values_mut() {
				engine.handle_state_chain_events(&events);
			}
		}
	}
}
