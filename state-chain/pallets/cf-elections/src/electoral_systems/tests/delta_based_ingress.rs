use super::{mocks::*, register_checks};
use crate::{
	electoral_system::ConsensusStatus, electoral_systems::blockchain::delta_based_ingress::*,
};
use cf_primitives::Asset;
use cf_traits::IngressSink;
use codec::{Decode, Encode};
use sp_std::collections::btree_map::BTreeMap;
use std::cell::RefCell;

thread_local! {
	pub static AMOUNT_INGRESSED: RefCell<Vec<(AccountId, Asset, Amount)>> = const { RefCell::new(vec![]) };
	pub static AMOUNT_REVERTED: RefCell<Vec<(AccountId, Asset, Amount)>> = const { RefCell::new(vec![]) };
	pub static CHANNELS_CLOSED: RefCell<Vec<AccountId>> = const { RefCell::new(vec![]) };
}

type AccountId = u32;
type Amount = u64;
type BlockNumber = u64;

struct MockIngressSink;
impl IngressSink for MockIngressSink {
	type Account = AccountId;
	type Asset = Asset;
	type Amount = Amount;
	type BlockNumber = BlockNumber;
	type DepositDetails = ();

	fn on_ingress(
		channel: Self::Account,
		asset: Self::Asset,
		amount: Self::Amount,
		_block_number: Self::BlockNumber,
		_details: Self::DepositDetails,
	) {
		AMOUNT_INGRESSED.with(|cell| {
			let mut ingresses = cell.borrow_mut();
			ingresses.push((channel, asset, amount));
		});
	}

	fn on_ingress_reverted(channel: Self::Account, asset: Self::Asset, amount: Self::Amount) {
		AMOUNT_REVERTED.with(|cell| {
			let mut reverts = cell.borrow_mut();
			reverts.push((channel, asset, amount))
		});
	}

	fn on_channel_closed(channel: Self::Account) {
		CHANNELS_CLOSED.with(|cell| {
			let mut closed = cell.borrow_mut();
			closed.push(channel);
		});
	}
}

type SimpleDeltaBasedIngress = DeltaBasedIngress<MockIngressSink, (), AccountId>;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Encode, Decode)]
struct DepositChannel {
	pub account: AccountId,
	pub asset: Asset,
	pub total_ingressed: Amount,
	pub block_number: BlockNumber,
	pub close_block: BlockNumber,
}

#[derive(Clone, Debug, Eq, PartialEq, Encode, Decode)]
struct DepositChannels(Vec<DepositChannel>);
impl DepositChannels {
	pub fn new(inner: Vec<DepositChannel>) -> Self {
		Self(inner)
	}

	pub fn to_state(&self) -> BTreeMap<AccountId, ChannelTotalIngressedFor<MockIngressSink>> {
		self.0
			.clone()
			.into_iter()
			.map(|channel| {
				(
					channel.account,
					ChannelTotalIngressed {
						amount: channel.total_ingressed,
						block_number: channel.block_number,
					},
				)
			})
			.collect::<BTreeMap<_, _>>()
	}

	pub fn to_state_map(
		&self,
	) -> BTreeMap<(AccountId, Asset), ChannelTotalIngressedFor<MockIngressSink>> {
		self.0
			.clone()
			.into_iter()
			.map(|channel| {
				(
					(channel.account, channel.asset),
					ChannelTotalIngressed {
						amount: channel.total_ingressed,
						block_number: channel.block_number,
					},
				)
			})
			.collect::<BTreeMap<_, _>>()
	}

	pub fn to_properties(
		&self,
	) -> BTreeMap<
		AccountId,
		(OpenChannelDetailsFor<MockIngressSink>, ChannelTotalIngressedFor<MockIngressSink>),
	> {
		self.0
			.clone()
			.into_iter()
			.map(|channel| {
				(
					channel.account,
					(
						OpenChannelDetails {
							asset: channel.asset,
							close_block: channel.close_block,
						},
						ChannelTotalIngressed {
							amount: channel.total_ingressed,
							block_number: channel.block_number,
						},
					),
				)
			})
			.collect::<BTreeMap<_, _>>()
	}
}

fn to_raw_state_map<K: Encode, V: Encode, M: IntoIterator<Item = (K, V)>>(
	state_map: M,
) -> BTreeMap<Vec<u8>, Option<Vec<u8>>> {
	BTreeMap::from_iter(
		state_map.into_iter().map(|(key, value)| (key.encode(), Some(value.encode()))),
	)
}

fn initial_channel_state() -> DepositChannels {
	DepositChannels::new(vec![
		DepositChannel {
			account: 1u32,
			asset: Asset::Sol,
			total_ingressed: 0u64,
			block_number: 0u64,
			close_block: 1_000u64,
		},
		DepositChannel {
			account: 2u32,
			asset: Asset::SolUsdc,
			total_ingressed: 0u64,
			block_number: 0u64,
			close_block: 2_000u64,
		},
	])
}
fn channel_state_ingressed() -> DepositChannels {
	DepositChannels::new(vec![
		DepositChannel {
			account: 1u32,
			asset: Asset::Sol,
			total_ingressed: 1_000u64,
			block_number: 700u64,
			close_block: 1_000u64,
		},
		DepositChannel {
			account: 2u32,
			asset: Asset::SolUsdc,
			total_ingressed: 2_000u64,
			block_number: 800u64,
			close_block: 2_000u64,
		},
	])
}

fn channel_state_reverted() -> DepositChannels {
	DepositChannels::new(vec![
		DepositChannel {
			account: 1u32,
			asset: Asset::Sol,
			total_ingressed: 500u64,
			block_number: 900u64,
			close_block: 1_000u64,
		},
		DepositChannel {
			account: 2u32,
			asset: Asset::SolUsdc,
			total_ingressed: 1_000u64,
			block_number: 900u64,
			close_block: 2_000u64,
		},
	])
}

fn channel_state_final() -> DepositChannels {
	DepositChannels::new(vec![
		DepositChannel {
			account: 1u32,
			asset: Asset::Sol,
			total_ingressed: 2_000u64,
			block_number: 900u64,
			close_block: 1_000u64,
		},
		DepositChannel {
			account: 2u32,
			asset: Asset::SolUsdc,
			total_ingressed: 4_000u64,
			block_number: 900u64,
			close_block: 2_000u64,
		},
	])
}

fn channel_state_closed() -> DepositChannels {
	DepositChannels::new(vec![
		DepositChannel {
			account: 1u32,
			asset: Asset::Sol,
			total_ingressed: 2_000u64,
			block_number: 1_000u64,
			close_block: 1_000u64,
		},
		DepositChannel {
			account: 2u32,
			asset: Asset::SolUsdc,
			total_ingressed: 4_000u64,
			block_number: 2_000u64,
			close_block: 2_000u64,
		},
	])
}

fn with_setup(initial_elections: Option<DepositChannels>) -> TestSetup<SimpleDeltaBasedIngress> {
	let initial_elections = initial_elections.unwrap_or(initial_channel_state());
	TestSetup::<_>::default()
		.with_initial_election_state(
			1u32,
			initial_elections.to_properties(),
			initial_elections.to_state(),
		)
		.with_initial_state_map(initial_elections.to_state_map().into_iter().collect::<Vec<_>>())
}

register_checks! {
	SimpleDeltaBasedIngress {
		started_at_default_state(pre_finalize, _post) {
			assert_eq!(
				pre_finalize.unsynchronised_state_map,
				to_raw_state_map(initial_channel_state().to_state_map()),
				"Expected initial state incorrect pre-finalization."
			);
		},
		ended_at_ingressed_state(_pre, post_finalize) {
			assert_eq!(
				post_finalize.unsynchronised_state_map,
				to_raw_state_map(channel_state_ingressed().to_state_map()),
				"Expected ingressed state incorrect after finalization."
			);
		},
		deposit_channel_states_final(pre_finalize, post_finalize) {
			assert_eq!(
				pre_finalize.unsynchronised_state_map,
				to_raw_state_map(channel_state_ingressed().to_state_map()),
				"Expected ingressed state incorrect pre-finalization."
			);
			assert_eq!(
				post_finalize.unsynchronised_state_map,
				to_raw_state_map(channel_state_final().to_state_map()),
				"Expected final state incorrect after finalization."
			);
		},
	}
}

#[test]
fn trigger_ingress_on_consensus() {
	with_setup(None)
		.build_with_initial_election()
		.then(|| {
			AMOUNT_INGRESSED.with(|ingresses| {
				assert_eq!(ingresses.clone().into_inner().len(), 0);
			});
		})
		.force_consensus_update(ConsensusStatus::Gained {
			most_recent: None,
			new: channel_state_ingressed().to_state(),
		})
		.test_on_finalize(
			&900,
			|_| (),
			vec![Check::started_at_default_state(), Check::ended_at_ingressed_state()],
		)
		.then(|| {
			AMOUNT_INGRESSED.with(|ingresses| {
				assert_eq!(
					ingresses.clone().into_inner(),
					vec![(1u32, Asset::Sol, 1_000u64), (2u32, Asset::SolUsdc, 2_000u64)]
				);
			});
		});
}

#[test]
fn only_trigger_ingress_on_witnessed_blocks() {
	with_setup(None)
		.build_with_initial_election()
		.force_consensus_update(ConsensusStatus::Gained {
			most_recent: None,
			new: channel_state_ingressed().to_state(),
		})
		.test_on_finalize(&699, |_| (), vec![Check::assert_unchanged()])
		.test_on_finalize(&799, |_| (), vec![Check::started_at_default_state()])
		.then(|| {
			AMOUNT_INGRESSED.with(|ingresses| {
				assert_eq!(ingresses.clone().into_inner(), vec![(1u32, Asset::Sol, 1_000u64)]);
			});
		})
		.test_on_finalize(&800, |_| (), vec![Check::ended_at_ingressed_state()])
		.then(|| {
			AMOUNT_INGRESSED.with(|ingresses| {
				assert_eq!(
					ingresses.clone().into_inner(),
					vec![(1u32, Asset::Sol, 1_000u64), (2u32, Asset::SolUsdc, 2_000u64)]
				);
			});
		});
}

#[test]
fn can_trigger_revert_logic() {
	with_setup(None)
		.build_with_initial_election()
		.force_consensus_update(ConsensusStatus::Gained {
			most_recent: None,
			new: channel_state_ingressed().to_state(),
		})
		.test_on_finalize(&800, |_| (), vec![Check::ended_at_ingressed_state()])
		.force_consensus_update(ConsensusStatus::Gained {
			most_recent: None,
			new: channel_state_reverted().to_state(),
		})
		.test_on_finalize(&900, |_| (), vec![])
		.then(|| {
			AMOUNT_REVERTED.with(|reverts| {
				assert_eq!(
					reverts.clone().into_inner(),
					vec![(1u32, Asset::Sol, 500u64), (2u32, Asset::SolUsdc, 1_000u64)]
				);
			});
		});
}

#[test]
fn can_close_channels() {
	with_setup(None)
		.build_with_initial_election()
		.force_consensus_update(ConsensusStatus::Gained {
			most_recent: None,
			new: channel_state_closed().to_state(),
		})
		.test_on_finalize(&1000, |_| (), vec![])
		.then(|| {
			CHANNELS_CLOSED.with(|closed| {
				assert_eq!(closed.clone().into_inner(), vec![1u32]);
			});
		})
		.force_consensus_update(ConsensusStatus::Gained {
			most_recent: None,
			new: channel_state_closed().to_state(),
		})
		.test_on_finalize(&2000, |_| (), vec![])
		.then(|| {
			CHANNELS_CLOSED.with(|closed| {
				assert_eq!(closed.clone().into_inner(), vec![1u32, 2u32]);
			});
		});
}
