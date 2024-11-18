#![cfg(test)]
mod network;

mod broadcasting;
mod mock_runtime;
mod signer_nomination;
mod threshold_signing;

mod account;
mod authorities;
mod funding;
mod genesis;
mod governance;
mod new_epoch;
mod solana;
mod solana_elections;
mod swapping;
mod witnessing;

use cf_chains::{
	eth::Address as EthereumAddress, evm::EvmTransactionMetadata, TransactionMetadata,
};
use cf_primitives::{AuthorityCount, BlockNumber, FlipBalance};
use cf_traits::EpochInfo;
use frame_support::{assert_noop, assert_ok, sp_runtime::AccountId32, traits::OnInitialize};
use pallet_cf_broadcast::AwaitingBroadcast;
use pallet_cf_funding::EthTransactionHash;
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_consensus_grandpa::AuthorityId as GrandpaId;
use sp_core::crypto::Pair;
use state_chain_runtime::{
	constants::common::*, opaque::SessionKeys, AccountId, BitcoinVault, Emissions, EthereumVault,
	Flip, Funding, Governance, PolkadotVault, Reputation, Runtime, RuntimeCall, RuntimeOrigin,
	SolanaVault, System, Validator, Witnesser,
};

type NodeId = AccountId32;
const ETH_DUMMY_ADDR: EthereumAddress = EthereumAddress::repeat_byte(42u8);
const ETH_ZERO_ADDRESS: EthereumAddress = EthereumAddress::repeat_byte(0xff);
const TX_HASH: EthTransactionHash = [211u8; 32];

pub const GENESIS_KEY_SEED: u64 = 42;

// Validators
pub const ALICE: [u8; 32] = [0xf0; 32];
pub const BOB: [u8; 32] = [0xf1; 32];
pub const CHARLIE: [u8; 32] = [0xf2; 32];
// Root and Gov member
pub const ERIN: [u8; 32] = [0xf3; 32];
// Broker
pub const BROKER: [u8; 32] = [0xf4; 32];
// Liquidity Provider
pub const LIQUIDITY_PROVIDER: [u8; 32] = [0xf5; 32];

pub fn get_validator_state(account_id: &AccountId) -> ChainflipAccountState {
	if Validator::current_authorities().contains(account_id) {
		ChainflipAccountState::CurrentAuthority
	} else {
		ChainflipAccountState::Backup
	}
}

// The minimum number of blocks a vault rotation should last
// 4 (keygen + key verification) + 4(key handover) + 2(activating_key) + 2(session rotating)
const VAULT_ROTATION_BLOCKS: BlockNumber = 12;

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum ChainflipAccountState {
	CurrentAuthority,
	Backup,
}

pub type AllVaults = <Runtime as pallet_cf_validator::Config>::KeyRotator;

/// Helper function that dispatches a call that requires EnsureWitnessed origin.
pub fn witness_call(call: RuntimeCall) {
	let epoch = Validator::epoch_index();
	let boxed_call = Box::new(call);
	for node in Validator::current_authorities() {
		assert_ok!(Witnesser::witness_at_epoch(
			RuntimeOrigin::signed(node),
			boxed_call.clone(),
			epoch,
		));
	}
}

/// this function witnesses the rotation tx broadcasts for all chains. It takes in the broadcast ids
/// of the rotation tx in the following chains order: Ethereum, Polkadot, Bitcoin, Arbitrum, Solana
/// (the order in which these chains were integrated in chainflip)
pub fn witness_rotation_broadcasts(broadcast_ids: [cf_primitives::BroadcastId; 5]) {
	witness_ethereum_rotation_broadcast(broadcast_ids[0]);
	witness_call(RuntimeCall::PolkadotBroadcaster(
		pallet_cf_broadcast::Call::transaction_succeeded {
			tx_out_id: AwaitingBroadcast::<Runtime, state_chain_runtime::PolkadotInstance>::get(
				broadcast_ids[1],
			)
			.unwrap()
			.transaction_out_id,
			signer_id: Default::default(),
			tx_fee: 1000,
			tx_metadata: Default::default(),
			transaction_ref: Default::default(),
		},
	));
	if let Some(broadcast_data) =
		AwaitingBroadcast::<Runtime, state_chain_runtime::BitcoinInstance>::get(broadcast_ids[2])
	{
		witness_call(RuntimeCall::BitcoinBroadcaster(
			pallet_cf_broadcast::Call::transaction_succeeded {
				tx_out_id: broadcast_data.transaction_out_id,
				// the ScriptPubkey doesnt mean anything here. we dont care
				// about the signer_id value so we just put any variant
				signer_id: cf_chains::btc::ScriptPubkey::P2PKH(Default::default()),
				tx_fee: 1000,
				tx_metadata: Default::default(),
				transaction_ref: Default::default(),
			},
		));
	}
	let arb_broadcast_data =
		AwaitingBroadcast::<Runtime, state_chain_runtime::ArbitrumInstance>::get(broadcast_ids[3])
			.unwrap();
	witness_call(RuntimeCall::ArbitrumBroadcaster(
			pallet_cf_broadcast::Call::transaction_succeeded {
				tx_out_id: arb_broadcast_data
				.transaction_out_id,
				signer_id: Default::default(),
				tx_fee: cf_chains::evm::TransactionFee {
					effective_gas_price: 1000000,
					gas_used: 100,
				},
				tx_metadata: <EvmTransactionMetadata as TransactionMetadata<
					cf_chains::Arbitrum,
				>>::extract_metadata(&arb_broadcast_data.transaction_payload),
				transaction_ref: Default::default(),
			},
		));
	witness_call(RuntimeCall::SolanaBroadcaster(
		pallet_cf_broadcast::Call::transaction_succeeded {
			tx_out_id: AwaitingBroadcast::<Runtime, state_chain_runtime::SolanaInstance>::get(
				broadcast_ids[4],
			)
			.unwrap()
			.transaction_out_id,
			signer_id: Default::default(),
			tx_fee: 1000,
			tx_metadata: Default::default(),
			transaction_ref: Default::default(),
		},
	));
	pallet_cf_environment::SolanaAvailableNonceAccounts::<Runtime>::append((
		<cf_chains::sol::SolAddress as Default>::default(),
		<cf_chains::sol::SolHash as Default>::default(),
	));
}

pub fn witness_ethereum_rotation_broadcast(broadcast_id: cf_primitives::BroadcastId) {
	let eth_broadcast_data =
		AwaitingBroadcast::<Runtime, state_chain_runtime::EthereumInstance>::get(broadcast_id)
			.unwrap();
	witness_call(RuntimeCall::EthereumBroadcaster(
	pallet_cf_broadcast::Call::transaction_succeeded {
		tx_out_id: eth_broadcast_data.transaction_out_id,
		signer_id: Default::default(),
		tx_fee: cf_chains::evm::TransactionFee {
			effective_gas_price: 1000000,
			gas_used: 1000,
		},
		tx_metadata:
			<EvmTransactionMetadata as TransactionMetadata<cf_chains::Ethereum>>::extract_metadata(
				&eth_broadcast_data.transaction_payload,
			),
		transaction_ref: Default::default(),
	}));
}

/// Provide helper structs and functions to make voting in Solana Elections easier.
pub mod solana_test_utils {
	use cf_chains::{
		sol::{sol_tx_core::sol_test_values, SolAddress, SolApiEnvironment, SolHash},
		ChannelRefundParameters, ForeignChainAddress,
	};
	use cf_traits::EpochInfo;
	use frame_support::{assert_ok, BoundedBTreeMap};
	use pallet_cf_elections::{
		electoral_system_runner::RunnerStorageAccessTrait,
		electoral_systems::{
			blockchain::delta_based_ingress::ChannelTotalIngressedFor,
			composite::tuple_6_impls::{
				CompositeElectionIdentifierExtra, CompositeElectionProperties,
			},
		},
		vote_storage::{
			change::MonotonicChangeVote, composite::tuple_6_impls::CompositeVote, AuthorityVote,
		},
		CompositeAuthorityVoteOf, CompositeElectionIdentifierOf, MAXIMUM_VOTES_PER_EXTRINSIC,
	};
	use sp_core::ConstU32;
	use sp_std::collections::btree_map::BTreeMap;
	use state_chain_runtime::{
		chainflip::solana_elections::TransactionSuccessDetails, Runtime, RuntimeOrigin,
		SolanaElections, SolanaIngressEgress, SolanaInstance, Validator,
	};

	pub type SolanaCompositeVote = CompositeAuthorityVoteOf<
		<Runtime as pallet_cf_elections::Config<SolanaInstance>>::ElectoralSystemRunner,
	>;
	pub type SolanaCompositeElectionIdentifier = CompositeElectionIdentifierOf<
		<Runtime as pallet_cf_elections::Config<SolanaInstance>>::ElectoralSystemRunner,
	>;
	pub type SolanaChannelIngressed = ChannelTotalIngressedFor<SolanaIngressEgress>;

	pub type SolanaElectionVote = BoundedBTreeMap<
		SolanaCompositeElectionIdentifier,
		SolanaCompositeVote,
		ConstU32<MAXIMUM_VOTES_PER_EXTRINSIC>,
	>;

	pub const DEPOSIT_AMOUNT: u64 = 5_000_000_000u64; // 5 Sol
	pub const FALLBACK_ADDRESS: SolAddress = SolAddress([0xf0; 32]);
	pub const REFUND_PARAMS: ChannelRefundParameters = ChannelRefundParameters {
		retry_duration: 0,
		refund_address: ForeignChainAddress::Sol(FALLBACK_ADDRESS),
		min_price: sp_core::U256::zero(),
	};

	/// Simulates observable state from the Solana chain. Can be converted into SolanaElection's
	/// Vote directly.
	pub enum SolanaState {
		BlockHeight(u64),
		Fee(u64),
		Ingressed(Vec<(SolAddress, SolanaChannelIngressed)>),
		Nonce(SolAddress, SolHash, u64),
		Egress(TransactionSuccessDetails),
	}

	impl SolanaState {
		/// Used to find the right Election Identifier.
		pub fn is_of_type(&self, target: &SolanaCompositeElectionIdentifier) -> bool {
			match self {
				SolanaState::BlockHeight(..) =>
					matches!(*target.extra(), CompositeElectionIdentifierExtra::A(..)),
				SolanaState::Fee(..) =>
					matches!(*target.extra(), CompositeElectionIdentifierExtra::B(..)),
				SolanaState::Ingressed(..) =>
					matches!(*target.extra(), CompositeElectionIdentifierExtra::C(..)),
				SolanaState::Nonce(address, ..) =>
					matches!(*target.extra(), CompositeElectionIdentifierExtra::D(..)) &&
						{
							if let CompositeElectionProperties::D((addr, _, _)) = pallet_cf_elections::RunnerStorageAccess::<Runtime, SolanaInstance>::election_properties(*target).expect("Election property must exist.") {
							*address == addr
						} else {
							false
						}
						},
				SolanaState::Egress(..) =>
					matches!(*target.extra(), CompositeElectionIdentifierExtra::EE(..)),
			}
		}
	}

	impl From<SolanaState> for SolanaCompositeVote {
		fn from(value: SolanaState) -> Self {
			match value {
				SolanaState::BlockHeight(block_height) =>
					AuthorityVote::Vote(CompositeVote::A(block_height)),
				SolanaState::Fee(fee) => AuthorityVote::Vote(CompositeVote::B(fee)),
				SolanaState::Ingressed(channel_ingresses) => AuthorityVote::Vote(CompositeVote::C(
					channel_ingresses
						.into_iter()
						.collect::<BTreeMap<_, _>>()
						.try_into()
						.expect("Too many ingress channels per election."),
				)),
				SolanaState::Nonce(_addr, value, block) =>
					AuthorityVote::Vote(CompositeVote::D(MonotonicChangeVote { value, block })),
				SolanaState::Egress(transaction_success_details) =>
					AuthorityVote::Vote(CompositeVote::EE(transaction_success_details)),
			}
		}
	}

	/// Have all validators to vote to witness the given `SolanaState` via SolanaElection.
	#[track_caller]
	pub fn witness_solana_state(state: SolanaState) {
		// Get the election identifier of the Solana egress.
		let election_id = SolanaElections::with_election_identifiers(|election_identifiers| {
			Ok(election_identifiers
				.into_iter()
				.find(|id| state.is_of_type(id))
				.expect("Election must exists to be voted on."))
		})
		.unwrap();

		let vote = state.into();

		// Submit vote to witness: transaction success, but execution failure
		let votes: SolanaElectionVote =
			BTreeMap::from_iter([(election_id, vote)]).try_into().unwrap();

		for v in Validator::current_authorities() {
			assert_ok!(SolanaElections::vote(RuntimeOrigin::signed(v), votes.clone()));
		}
	}

	pub fn setup_sol_environments() {
		// Environment::SolanaApiEnvironment
		pallet_cf_environment::SolanaApiEnvironment::<Runtime>::set(SolApiEnvironment {
			vault_program: sol_test_values::VAULT_PROGRAM,
			vault_program_data_account: sol_test_values::VAULT_PROGRAM_DATA_ACCOUNT,
			token_vault_pda_account: sol_test_values::TOKEN_VAULT_PDA_ACCOUNT,
			usdc_token_mint_pubkey: sol_test_values::USDC_TOKEN_MINT_PUB_KEY,
			usdc_token_vault_ata: sol_test_values::USDC_TOKEN_VAULT_ASSOCIATED_TOKEN_ACCOUNT,
			swap_endpoint_program: sol_test_values::SWAP_ENDPOINT_PROGRAM,
			swap_endpoint_program_data_account: sol_test_values::SWAP_ENDPOINT_PROGRAM_DATA_ACCOUNT,
		});

		// Environment::AvailableDurableNonces
		pallet_cf_environment::SolanaAvailableNonceAccounts::<Runtime>::set(
			sol_test_values::NONCE_ACCOUNTS
				.into_iter()
				.map(|nonce| (nonce, sol_test_values::TEST_DURABLE_NONCE))
				.collect::<Vec<_>>(),
		);

		// Enable voting for all validators
		for v in Validator::current_authorities() {
			assert_ok!(SolanaElections::stop_ignoring_my_votes(RuntimeOrigin::signed(v.clone()),));
		}
	}
}
