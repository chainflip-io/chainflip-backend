use super::*;

use crate::threshold_signing::{BtcThresholdSigner, DotThresholdSigner, EthThresholdSigner};

use cf_primitives::{AccountRole, BlockNumber, EpochIndex, FlipBalance, TxId, GENESIS_EPOCH};
use cf_traits::{AccountRoleRegistry, Chainflip, EpochInfo, VaultRotator};
use cfe_events::{KeyHandoverRequest, ThresholdSignatureRequest};
use chainflip_node::test_account_from_seed;
use codec::Encode;
use frame_support::{
	inherent::ProvideInherent,
	pallet_prelude::InherentData,
	traits::{IntegrityTest, OnFinalize, OnIdle, UnfilteredDispatchable},
};
use pallet_cf_funding::{MinimumFunding, RedemptionAmount};
use sp_consensus_aura::SlotDuration;
use sp_std::collections::btree_set::BTreeSet;
use state_chain_runtime::{
	AccountRoles, AllPalletsWithSystem, BitcoinInstance, PalletExecutionOrder, PolkadotInstance,
	Runtime, RuntimeCall, RuntimeEvent, RuntimeOrigin, Validator, Weight,
};
use std::{
	cell::RefCell,
	collections::{HashMap, VecDeque},
	rc::Rc,
};

type CfeEvent = cfe_events::CfeEvent<<Runtime as Chainflip>::ValidatorId>;

// TODO: Can we use the actual events here?
// Events from ethereum contract
#[derive(Debug, Clone)]
pub enum ContractEvent {
	Funded { node_id: NodeId, amount: FlipBalance, total: FlipBalance, epoch: EpochIndex },

	Redeemed { node_id: NodeId, amount: FlipBalance, epoch: EpochIndex },
}

macro_rules! on_events {
	($events:expr, $($(#[$cfg_param:meta])? $p:pat => $b:block)+) => {
		for event in $events {
			$(
				$(#[$cfg_param])?
				if let $p = event { $b }
			)*
		}
	}
}

pub const NEW_FUNDING_AMOUNT: FlipBalance = mock_runtime::MIN_FUNDING + 1;

pub fn create_testnet_with_new_funder() -> (Network, AccountId32) {
	let (mut testnet, backup_nodes) = Network::create(1, &Validator::current_authorities());

	let new_backup = backup_nodes.first().unwrap().clone();

	testnet.state_chain_gateway_contract.fund_account(
		new_backup.clone(),
		NEW_FUNDING_AMOUNT,
		GENESIS_EPOCH,
	);
	// register the funds
	testnet.move_forward_blocks(2);

	(testnet, new_backup)
}

// An SC Gateway contract
#[derive(Default)]
pub struct ScGatewayContract {
	// List of balances
	pub balances: HashMap<NodeId, FlipBalance>,
	// Events to be processed
	pub events: Vec<ContractEvent>,
}

impl ScGatewayContract {
	pub fn fund_account(&mut self, node_id: NodeId, amount: FlipBalance, epoch: EpochIndex) {
		assert!(amount >= MinimumFunding::<Runtime>::get());
		let current_amount = self.balances.get(&node_id).unwrap_or(&0);
		let total = current_amount + amount;
		self.balances.insert(node_id.clone(), total);

		self.events.push(ContractEvent::Funded { node_id, amount, total, epoch });
	}

	// We don't really care about the process of "registering" and then "executing" redemption here.
	// The only thing the SC cares about is the *execution* of the redemption.
	pub fn execute_redemption(&mut self, node_id: NodeId, amount: FlipBalance, epoch: EpochIndex) {
		self.events.push(ContractEvent::Redeemed { node_id, amount, epoch });
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
	pub fn start_bidding(account: &NodeId) {
		assert_ok!(Funding::start_bidding(RuntimeOrigin::signed(account.clone())));
	}

	pub fn redeem(
		account: &NodeId,
		amount: RedemptionAmount<FlipBalance>,
		eth_address: EthereumAddress,
	) {
		assert_ok!(Funding::redeem(
			RuntimeOrigin::signed(account.clone()),
			amount,
			eth_address,
			Default::default()
		));
	}

	pub fn set_vanity_name(account: &NodeId, name: &str) {
		assert_ok!(Validator::set_vanity_name(
			RuntimeOrigin::signed(account.clone()),
			name.as_bytes().to_vec()
		));
	}

	pub fn register_as_validator(account: &NodeId) {
		assert_ok!(<AccountRoles as AccountRoleRegistry<Runtime>>::register_account_role(
			account,
			AccountRole::Validator
		));
	}
}

// Engine monitoring contract
pub struct Engine {
	pub node_id: NodeId,
	// Automatically responds to events and responds with "OK".
	pub live: bool,
	// Automatically submits heartbeat to keep alive.
	pub auto_submit_heartbeat: bool,
	pub last_heartbeat: BlockNumber,

	// conveniently creates a threshold "signature" (not really)
	// all engines have the same one, so they create the same sig
	pub eth_threshold_signer: Rc<RefCell<EthThresholdSigner>>,
	pub dot_threshold_signer: Rc<RefCell<DotThresholdSigner>>,
	pub btc_threshold_signer: Rc<RefCell<BtcThresholdSigner>>,
}

impl Engine {
	fn new(
		node_id: NodeId,
		eth_threshold_signer: Rc<RefCell<EthThresholdSigner>>,
		dot_threshold_signer: Rc<RefCell<DotThresholdSigner>>,
		btc_threshold_signer: Rc<RefCell<BtcThresholdSigner>>,
	) -> Self {
		Engine {
			node_id,
			live: true,
			eth_threshold_signer,
			dot_threshold_signer,
			btc_threshold_signer,
			auto_submit_heartbeat: true,
			last_heartbeat: Default::default(),
		}
	}

	fn state(&self) -> ChainflipAccountState {
		get_validator_state(&self.node_id)
	}

	// Handle events from contract
	fn on_contract_event(&self, event: &ContractEvent) {
		if self.state() == ChainflipAccountState::CurrentAuthority && self.live {
			match event {
				ContractEvent::Funded { node_id: validator_id, amount, epoch, .. } => {
					queue_dispatch_extrinsic(
						RuntimeCall::Witnesser(pallet_cf_witnesser::Call::witness_at_epoch {
							call: Box::new(
								pallet_cf_funding::Call::funded {
									account_id: validator_id.clone(),
									amount: *amount,
									funder: ETH_ZERO_ADDRESS,
									tx_hash: TX_HASH,
								}
								.into(),
							),
							epoch_index: *epoch,
						}),
						RuntimeOrigin::signed(self.node_id.clone()),
					);
				},
				ContractEvent::Redeemed { node_id, amount, epoch } => {
					queue_dispatch_extrinsic(
						RuntimeCall::Witnesser(pallet_cf_witnesser::Call::witness_at_epoch {
							call: Box::new(
								pallet_cf_funding::Call::redeemed {
									account_id: node_id.clone(),
									redeemed_amount: *amount,
									tx_hash: TX_HASH,
								}
								.into(),
							),
							epoch_index: *epoch,
						}),
						RuntimeOrigin::signed(self.node_id.clone()),
					);
				},
			}
		}
	}

	// Handle events coming in from the state chain
	// TODO have this abstracted out
	fn handle_state_chain_events(&mut self, events: &[RuntimeEvent], cfe_events: &[CfeEvent]) {
		if self.live {
			// Being a CurrentAuthority we would respond to certain events
			if self.state() == ChainflipAccountState::CurrentAuthority {
				// Note: these aren't events that the engine normally responds to, but
				// we process them here due to the way integration tests are set up
				on_events! {
					events,
					RuntimeEvent::Validator(
						pallet_cf_validator::Event::NewEpoch(_epoch_index)) => {
							self.eth_threshold_signer.borrow_mut().use_proposed_key();
							self.dot_threshold_signer.borrow_mut().use_proposed_key();
							self.btc_threshold_signer.borrow_mut().use_proposed_key();
					}

					RuntimeEvent::PolkadotVault(pallet_cf_vaults::Event::<_, PolkadotInstance>::AwaitingGovernanceActivation { .. }) => {
						queue_dispatch_extrinsic(
							RuntimeCall::Environment(pallet_cf_environment::Call::witness_polkadot_vault_creation {
								dot_pure_proxy_vault_key: Default::default(),
								tx_id: TxId {
									block_number: 1,
									extrinsic_index: 0,
								},
							}),
							pallet_cf_governance::RawOrigin::GovernanceApproval.into()
						);
					}
					RuntimeEvent::BitcoinVault(pallet_cf_vaults::Event::<_, BitcoinInstance>::AwaitingGovernanceActivation { new_public_key }) => {
						queue_dispatch_extrinsic(
							RuntimeCall::Environment(pallet_cf_environment::Call::witness_current_bitcoin_block_number_for_key {
								block_number: 0,
								new_public_key: *new_public_key,
							}),
							pallet_cf_governance::RawOrigin::GovernanceApproval.into()
						);
					}
				};
			}

			use cfe_events::CfeEvent;

			for event in cfe_events {
				match event {
					CfeEvent::EthThresholdSignatureRequest(ThresholdSignatureRequest {
						ceremony_id,
						key,
						signatories,
						payload,
						..
					}) =>
						if signatories.contains(&self.node_id) {
							queue_dispatch_extrinsic(
								RuntimeCall::EthereumThresholdSigner(
									pallet_cf_threshold_signature::Call::signature_success {
										ceremony_id: *ceremony_id,
										signature: self
											.eth_threshold_signer
											.borrow()
											.sign_with_key(*key, payload.as_fixed_bytes()),
									},
								),
								RuntimeOrigin::none(),
							);
						},
					CfeEvent::DotThresholdSignatureRequest(ThresholdSignatureRequest {
						ceremony_id,
						key,
						signatories,
						payload,
						..
					}) =>
						if signatories.contains(&self.node_id) {
							if self.dot_threshold_signer.borrow().is_key_valid(key) {
								queue_dispatch_extrinsic(
									RuntimeCall::PolkadotThresholdSigner(
										pallet_cf_threshold_signature::Call::signature_success {
											ceremony_id: *ceremony_id,
											signature: self
												.dot_threshold_signer
												.borrow()
												.sign_with_key(*key, payload),
										},
									),
									RuntimeOrigin::none(),
								);
							} else {
								let mut offenders = BTreeSet::new();
								offenders.insert(self.node_id.clone());
								queue_dispatch_extrinsic(
								RuntimeCall::PolkadotThresholdSigner(
									pallet_cf_threshold_signature::Call::report_signature_failed {
										ceremony_id: *ceremony_id,
										offenders,
									},
								),
								RuntimeOrigin::signed(self.node_id.clone()),
							);
							}
						},
					CfeEvent::BtcThresholdSignatureRequest(ThresholdSignatureRequest {
						ceremony_id,
						key,
						signatories,
						payload,
						..
					}) =>
						if signatories.contains(&self.node_id) {
							queue_dispatch_extrinsic(
								RuntimeCall::BitcoinThresholdSigner(
									pallet_cf_threshold_signature::Call::signature_success {
										ceremony_id: *ceremony_id,
										signature: vec![self
											.btc_threshold_signer
											.borrow()
											.sign_with_key(*key, &(payload[0].1.clone()))],
									},
								),
								RuntimeOrigin::none(),
							);
						},
					CfeEvent::EthKeygenRequest(req) =>
						if req.participants.contains(&self.node_id) {
							queue_dispatch_extrinsic(
								RuntimeCall::EthereumVault(
									pallet_cf_vaults::Call::report_keygen_outcome {
										ceremony_id: req.ceremony_id,
										reported_outcome: Ok(self
											.eth_threshold_signer
											.borrow_mut()
											.propose_new_key()),
									},
								),
								RuntimeOrigin::signed(self.node_id.clone()),
							);
						},
					CfeEvent::DotKeygenRequest(req) =>
						if req.participants.contains(&self.node_id) {
							queue_dispatch_extrinsic(
								RuntimeCall::PolkadotVault(
									pallet_cf_vaults::Call::report_keygen_outcome {
										ceremony_id: req.ceremony_id,
										reported_outcome: Ok(self
											.dot_threshold_signer
											.borrow_mut()
											.propose_new_key()),
									},
								),
								RuntimeOrigin::signed(self.node_id.clone()),
							);
						},
					CfeEvent::BtcKeygenRequest(req) =>
						if req.participants.contains(&self.node_id) {
							queue_dispatch_extrinsic(
								RuntimeCall::BitcoinVault(
									pallet_cf_vaults::Call::report_keygen_outcome {
										ceremony_id: req.ceremony_id,
										reported_outcome: Ok(self
											.btc_threshold_signer
											.borrow_mut()
											.propose_new_key()),
									},
								),
								RuntimeOrigin::signed(self.node_id.clone()),
							);
						},
					CfeEvent::BtcKeyHandoverRequest(KeyHandoverRequest {
						ceremony_id,
						sharing_participants,
						receiving_participants,
						..
					}) => {
						let all_participants = sharing_participants
							.union(receiving_participants)
							.cloned()
							.collect::<BTreeSet<_>>();
						if all_participants.contains(&self.node_id) {
							queue_dispatch_extrinsic(
								RuntimeCall::BitcoinVault(
									pallet_cf_vaults::Call::report_key_handover_outcome {
										ceremony_id: *ceremony_id,
										reported_outcome: Ok(self
											.btc_threshold_signer
											.borrow_mut()
											.propose_new_key()),
									},
								),
								RuntimeOrigin::signed(self.node_id.clone()),
							);
						}
					},
					_ => {
						// ignored
					},
				}
			}
		}
	}
}

/// Do this after funding.
pub(crate) fn setup_account_and_peer_mapping(node_id: &NodeId) {
	setup_account(node_id);
	setup_peer_mapping(node_id);
}

// Create an account, generate and register the session keys
pub(crate) fn setup_account(node_id: &NodeId) {
	let seed = &node_id.clone().to_string();

	assert_ok!(state_chain_runtime::Session::set_keys(
		RuntimeOrigin::signed(node_id.clone()),
		SessionKeys {
			aura: test_account_from_seed::<AuraId>(seed),
			grandpa: test_account_from_seed::<GrandpaId>(seed),
		},
		vec![]
	));
}

pub(crate) fn setup_peer_mapping(node_id: &NodeId) {
	let seed = &node_id.clone().to_string();
	let peer_keypair = sp_core::ed25519::Pair::from_legacy_string(seed, None);

	assert_ok!(state_chain_runtime::Validator::register_peer_id(
		RuntimeOrigin::signed(node_id.clone()),
		peer_keypair.public(),
		0,
		0,
		peer_keypair.sign(&node_id.encode()[..]),
	));
}

#[derive(Default)]
pub struct Network {
	engines: HashMap<NodeId, Engine>,
	pub state_chain_gateway_contract: ScGatewayContract,

	// Used to initialised the threshold signers of the engines added
	pub eth_threshold_signer: Rc<RefCell<EthThresholdSigner>>,
	pub dot_threshold_signer: Rc<RefCell<DotThresholdSigner>>,
	pub btc_threshold_signer: Rc<RefCell<BtcThresholdSigner>>,
}

thread_local! {
	static PENDING_EXTRINSICS: RefCell<VecDeque<(state_chain_runtime::RuntimeCall, RuntimeOrigin)>> = RefCell::default();
	static TIMESTAMP: RefCell<u64> = RefCell::new(SLOT_DURATION);
}

fn queue_dispatch_extrinsic(call: impl Into<RuntimeCall>, origin: RuntimeOrigin) {
	PENDING_EXTRINSICS.with_borrow_mut(|v| {
		v.push_back((call.into(), origin));
	});
}

/// Dispatch all pending extrinsics in the queue.
pub fn dispatch_all_pending_extrinsics() {
	PENDING_EXTRINSICS.with_borrow_mut(|v| {
		v.drain(..).for_each(|(call, origin)| {
			let expect_ok = match call {
				RuntimeCall::EthereumThresholdSigner(..) |
				RuntimeCall::PolkadotThresholdSigner(..) |
				RuntimeCall::BitcoinThresholdSigner(..) |
				RuntimeCall::Environment(..) => {
					// These are allowed to fail, since it is possible to sign things
					// that have already succeeded
					false
				},
				_ => true,
			};
			let res = call.clone().dispatch_bypass_filter(origin);
			if expect_ok && res.is_err() {
				// An extrinsic failed. Log as much info as needed to help debugging.
				match call {
					RuntimeCall::EthereumVault(..) => log::info!(
						"Validator status: {:?}\nVault Status: {:?}",
						Validator::current_rotation_phase(),
						EthereumVault::pending_vault_rotations()
					),
					RuntimeCall::PolkadotVault(..) => log::info!(
						"Validator status: {:?}\nVault Status: {:?}",
						Validator::current_rotation_phase(),
						PolkadotVault::pending_vault_rotations()
					),
					RuntimeCall::BitcoinVault(..) => log::info!(
						"Validator status: {:?}\nVault Status: {:?}",
						Validator::current_rotation_phase(),
						BitcoinVault::pending_vault_rotations()
					),
					RuntimeCall::Validator(..) => log::info!(
						"Validator status: {:?}\nAllVaults Status: {:?}",
						Validator::current_rotation_phase(),
						AllVaults::status()
					),
					_ => {},
				}
				panic!("Extrinsic Failed. Call: {:?} \n Result: {:?}", call, res);
			}
		});
	});
}

impl Network {
	pub fn live_nodes(&self) -> Vec<NodeId> {
		self.engines
			.iter()
			.filter_map(|(node_id, engine)| if engine.live { Some(node_id.clone()) } else { None })
			.collect()
	}

	pub fn set_active_all_nodes(&mut self, active: bool) {
		self.engines.iter_mut().for_each(|(_, e)| e.live = active);
	}

	pub fn set_auto_heartbeat_all_nodes(&mut self, auto_heartbeat: bool) {
		self.engines
			.iter_mut()
			.for_each(|(_, e)| e.auto_submit_heartbeat = auto_heartbeat);
	}

	// Create a network which includes the authorities in genesis of number of nodes
	// and return a network and sorted list of nodes within
	pub fn create(
		number_of_backup_nodes: u8,
		existing_nodes: &BTreeSet<NodeId>,
	) -> (Self, BTreeSet<NodeId>) {
		let mut network: Network = Default::default();

		// Include any nodes already *created* to the test network
		for node in existing_nodes {
			network.add_engine(node);
			setup_peer_mapping(node);
			assert_ok!(Reputation::heartbeat(RuntimeOrigin::signed(node.clone())));
		}

		// Create the backup nodes
		let mut backup_nodes = BTreeSet::new();
		for _ in 0..number_of_backup_nodes {
			let node_id = network.create_engine();
			backup_nodes.insert(node_id.clone());
		}

		(network, backup_nodes)
	}

	pub fn set_active(&mut self, node_id: &NodeId, active: bool) {
		self.engines.get_mut(node_id).expect("valid node_id").live = active;
	}

	pub fn create_engine(&mut self) -> NodeId {
		let node_id = NodeId::from([self.engines.len() as u8; 32]);
		self.add_engine(&node_id);
		node_id
	}

	// Adds an engine to the test network
	pub fn add_engine(&mut self, node_id: &NodeId) {
		self.engines.insert(
			node_id.clone(),
			Engine::new(
				node_id.clone(),
				self.eth_threshold_signer.clone(),
				self.dot_threshold_signer.clone(),
				self.btc_threshold_signer.clone(),
			),
		);
	}

	/// Move to the next epoch, to the block after the completion of Authority rotation.
	pub fn move_to_the_next_epoch(&mut self) {
		let epoch = Validator::epoch_index();
		self.move_to_the_end_of_epoch();
		self.move_forward_blocks(VAULT_ROTATION_BLOCKS);
		assert_eq!(epoch + 1, Validator::epoch_index());
	}

	/// Move to the last block of the epoch - next block will start Authority rotation
	pub fn move_to_the_end_of_epoch(&mut self) {
		let current_block = System::block_number();
		let target = Validator::current_epoch_started_at() + Validator::blocks_per_epoch();
		if target > current_block {
			self.move_forward_blocks(target - current_block - 1)
		}
	}

	/// Move to the next heartbeat interval block.
	pub fn move_to_next_heartbeat_block(&mut self) {
		self.move_forward_blocks(
			HEARTBEAT_BLOCK_INTERVAL - System::block_number() % HEARTBEAT_BLOCK_INTERVAL,
		);
	}

	// Submits heartbeat for keep alive.
	// If `force_update`, submit heartbeat unconditionally.
	// else, submit according to auto-heartbeat setting and current block_number.
	pub fn submit_heartbeat_all_engines(&mut self, force_update: bool) {
		let current_block = System::block_number();
		self.engines.iter_mut().for_each(|(_, engine)| {
			// only validator roles are allowed to submit heartbeat.
			if AccountRoles::has_account_role(&engine.node_id, AccountRole::Validator) &&
				match force_update {
					true => true,
					false =>
						engine.auto_submit_heartbeat &&
							engine.last_heartbeat + HEARTBEAT_BLOCK_INTERVAL - 1 <= current_block,
				} {
				assert_ok!(Reputation::heartbeat(RuntimeOrigin::signed(engine.node_id.clone())));
				engine.last_heartbeat = current_block;
			}
		});
	}

	pub fn move_forward_blocks(&mut self, n: u32) {
		let start_block = System::block_number() + 1;
		for block_number in start_block..(start_block + n) {
			// Process any external events that have occurred.
			for event in self.state_chain_gateway_contract.events() {
				for engine in self.engines.values() {
					engine.on_contract_event(&event);
				}
			}
			self.state_chain_gateway_contract.clear();

			// Inherent data.
			let timestamp = TIMESTAMP.with_borrow_mut(|t| std::mem::replace(t, *t + SLOT_DURATION));
			let slot = sp_consensus_aura::Slot::from_timestamp(
				sp_timestamp::Timestamp::new(timestamp),
				SlotDuration::from_millis(SLOT_DURATION),
			);
			let mut inherent_data = InherentData::new();
			inherent_data.put_data(sp_timestamp::INHERENT_IDENTIFIER, &timestamp).unwrap();
			inherent_data
				.put_data(sp_consensus_aura::inherents::INHERENT_IDENTIFIER, &slot)
				.unwrap();

			// Header digest.
			let mut digest = sp_runtime::Digest::default();
			digest.push(sp_runtime::DigestItem::PreRuntime(
				sp_consensus_aura::AURA_ENGINE_ID,
				slot.encode(),
			));

			// Reset events before on_initialise, same as in frame_executive.
			System::reset_events();

			// Initialize
			System::initialize(&block_number, &System::block_hash(block_number), &digest);
			PalletExecutionOrder::on_initialize(block_number);

			// Inherents
			assert_ok!(state_chain_runtime::Timestamp::create_inherent(&inherent_data)
				.unwrap()
				.dispatch_bypass_filter(RuntimeOrigin::none()));

			self.submit_heartbeat_all_engines(false);
			dispatch_all_pending_extrinsics();

			// Provide very large weight to ensure all on_idle processing can occur
			AllPalletsWithSystem::on_idle(
				block_number,
				Weight::from_parts(2_000_000_000_000, u64::MAX),
			);

			// We must finalise this to clear the previous author which is otherwise cached
			PalletExecutionOrder::on_finalize(block_number);
			AllPalletsWithSystem::integrity_test();

			// Engine reacts to events from the State Chain.
			let events = frame_system::Pallet::<Runtime>::events()
				.into_iter()
				.map(|e| e.event)
				.collect::<Vec<RuntimeEvent>>();

			let cfe_events = state_chain_runtime::CfeInterface::get_cfe_events();

			for engine in self.engines.values_mut() {
				engine.handle_state_chain_events(&events, &cfe_events);
			}
		}
	}
}

// Helper function that creates a network, funds backup nodes, and have them join the auction.
pub fn fund_authorities_and_join_auction(
	max_authorities: AuthorityCount,
) -> (network::Network, BTreeSet<NodeId>, BTreeSet<NodeId>) {
	// Create MAX_AUTHORITIES backup nodes and fund them above our genesis
	// authorities The result will be our newly created nodes will be authorities
	// and the genesis authorities will become backup nodes
	let genesis_authorities: BTreeSet<AccountId32> = Validator::current_authorities();
	let (mut testnet, init_backup_nodes) =
		network::Network::create(max_authorities as u8, &genesis_authorities);

	pallet_cf_flip::OffchainFunds::<Runtime>::set(u128::MAX);

	// An initial balance which is greater than the genesis balances
	// We intend for these initially backup nodes to win the auction
	const INITIAL_FUNDING: FlipBalance = genesis::GENESIS_BALANCE * 2;
	for node in &init_backup_nodes {
		testnet.state_chain_gateway_contract.fund_account(
			node.clone(),
			INITIAL_FUNDING,
			GENESIS_EPOCH,
		);
	}

	// Allow the funds to be registered, initialise the account keys and peer
	// ids, register as a validator, then start bidding.
	testnet.move_forward_blocks(2);

	for node in &init_backup_nodes {
		network::Cli::register_as_validator(node);
		network::setup_account_and_peer_mapping(node);
		network::Cli::start_bidding(node);
	}

	(testnet, genesis_authorities, init_backup_nodes)
}
