use super::*;
use cf_chains::eth::{to_ethereum_address, AggKey, SchnorrVerificationComponents};
use cf_traits::{
	ChainflipAccount, ChainflipAccountState, ChainflipAccountStore, EpochIndex, EpochInfo,
	FlipBalance,
};
use codec::Encode;
use libsecp256k1::PublicKey;
use sp_core::H256;
use state_chain_runtime::{Event, Origin};
use std::{cell::RefCell, collections::HashMap, rc::Rc};

// TODO: Can we use the actual events here?
// Events from ethereum contract
#[derive(Debug, Clone)]
pub enum ContractEvent {
	Staked { node_id: NodeId, amount: FlipBalance, total: FlipBalance, epoch: EpochIndex },
}

macro_rules! on_events {
	($events:expr, $( $p:pat => $b:block ),* $(,)?) => {
		for event in $events {
			$(if let $p = event { $b })*
		}
	}
}

pub const NEW_STAKE_AMOUNT: FlipBalance = 4;

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
	pub fn activate_account(account: &NodeId) {
		assert_ok!(Staking::activate_account(Origin::signed(account.clone())));
	}
}

#[derive(Clone)]
pub struct KeyComponents {
	pub secret: SecretKey,
	// agg key
	pub agg_key: AggKey,
}

impl KeyComponents {
	fn sign(&self, message: &cf_chains::eth::H256) -> SchnorrVerificationComponents {
		assert_eq!(self.agg_key, AggKey::from_private_key_bytes(self.secret.serialize()));

		// just use the same signature nonce for every ceremony in tests
		let k: [u8; 32] = StdRng::seed_from_u64(200).gen();
		let k = SecretKey::parse(&k).unwrap();
		let signature = self.agg_key.sign(&(*message).into(), &self.secret, &k);

		let k_times_g_address = to_ethereum_address(PublicKey::from_secret_key(&k));
		SchnorrVerificationComponents { s: signature, k_times_g_address }
	}
}

pub struct ThresholdSigner {
	key_seed: u64,
	key_components: KeyComponents,
	proposed_seed: Option<u64>,
	proposed_key_components: Option<KeyComponents>,
}

impl Default for ThresholdSigner {
	fn default() -> Self {
		let (secret, _pub_key, agg_key) = Self::generate_keypair(GENESIS_KEY);
		ThresholdSigner {
			key_seed: GENESIS_KEY,
			key_components: KeyComponents { secret, agg_key },
			proposed_seed: None,
			proposed_key_components: None,
		}
	}
}

impl ThresholdSigner {
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
		let proposed_seed = self.key_seed + 1;
		let (secret, _pub_key, agg_key) = Self::generate_keypair(proposed_seed);
		self.proposed_seed = Some(proposed_seed);
		self.proposed_key_components = Some(KeyComponents { secret, agg_key });
		self.proposed_public_key()
	}

	// Rotate to the current proposed key and clear the proposed key
	pub fn use_proposed_key(&mut self) {
		if self.proposed_seed.is_some() {
			self.key_seed = self.proposed_seed.expect("No key has been proposed");
			self.key_components =
				self.proposed_key_components.as_ref().expect("No key has been proposed").clone();
			self.proposed_seed = None;
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
		ChainflipAccountStore::<Runtime>::get(&self.node_id).state
	}

	// Handle events from contract
	fn on_contract_event(&self, event: &ContractEvent) {
		if self.state() == ChainflipAccountState::CurrentAuthority && self.live {
			match event {
				ContractEvent::Staked { node_id: validator_id, amount, epoch, .. } => {
					// Witness event -> send transaction to state chain
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
							(&*self.threshold_signer).borrow_mut().use_proposed_key();
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
								(&*self.threshold_signer).borrow().key_components.sign(payload),
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
									Box::new(pallet_cf_vaults::Call::vault_key_rotated {
										new_public_key: (&*self.threshold_signer).borrow_mut().proposed_public_key(),
										block_number: 100,
										tx_hash: [1u8; 32].into(),
									}.into()),
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
							(&*self.threshold_signer).borrow_mut().propose_new_public_key();
							let threshold_signer = (&*self.threshold_signer).borrow();
							let proposed_key_components = threshold_signer.proposed_key_components.as_ref().expect("should have propposed key");
							let payload: H256 = proposed_key_components.agg_key.pub_key_x.into();
							let sig = proposed_key_components.sign(&payload);
							state_chain_runtime::EthereumVault::report_keygen_outcome(
								Origin::signed(self.node_id.clone()),
								*ceremony_id,
								// Propose a new key
								Ok((proposed_key_components.agg_key, payload, sig)),
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
			// Only need to setup peer mapping as the AccountInfo is already set up if they
			// are genesis nodes
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
		pub const INIT_TIMESTAMP: u64 = 30_000;
		let current_block_number = System::block_number();
		while System::block_number() < current_block_number + n {
			let block_number = System::block_number() + 1;
			let mut digest = sp_runtime::Digest::default();
			digest.push(sp_runtime::DigestItem::PreRuntime(
				sp_consensus_aura::AURA_ENGINE_ID,
				sp_consensus_aura::Slot::from(block_number as u64).encode(),
			));
			System::initialize(&block_number, &System::block_hash(block_number), &digest);
			Timestamp::set_timestamp((block_number as u64 * BLOCK_TIME) + INIT_TIMESTAMP);
			state_chain_runtime::AllPalletsWithSystem::on_initialize(block_number);

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
