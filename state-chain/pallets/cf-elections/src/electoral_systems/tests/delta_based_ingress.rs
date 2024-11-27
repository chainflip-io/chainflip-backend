use super::{mocks::*, register_checks};
use crate::{
	electoral_system::ConsensusStatus, electoral_systems::blockchain::delta_based_ingress::*,
};
use cf_primitives::Asset;
use cf_traits::IngressSink;
use codec::{Decode, Encode};
use frame_support::assert_ok;
use sp_std::collections::btree_map::BTreeMap;
use std::cell::RefCell;

thread_local! {
	pub static AMOUNT_INGRESSED: RefCell<Vec<(AccountId, Asset, Amount)>> = const { RefCell::new(vec![]) };
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

	pub fn into_inner(self) -> Vec<DepositChannel> {
		self.0
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

	pub fn combine(self, other: Self) -> Self {
		Self(
			[self.into_inner(), other.into_inner()]
				.into_iter()
				.flatten()
				.collect::<Vec<_>>(),
		)
	}
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

fn channel_state_recycled_same_asset() -> DepositChannels {
	DepositChannels::new(vec![
		DepositChannel {
			account: 1u32,
			asset: Asset::Sol,
			total_ingressed: 8_000u64,
			block_number: 4_000u64,
			close_block: 4_000u64,
		},
		DepositChannel {
			account: 2u32,
			asset: Asset::SolUsdc,
			total_ingressed: 10_000u64,
			block_number: 4_000u64,
			close_block: 4_000u64,
		},
	])
}

fn channel_state_recycled_different_asset() -> DepositChannels {
	DepositChannels::new(vec![
		DepositChannel {
			account: 1u32,
			asset: Asset::SolUsdc,
			total_ingressed: 4_000u64,
			block_number: 5_000u64,
			close_block: 5_000u64,
		},
		DepositChannel {
			account: 2u32,
			asset: Asset::Sol,
			total_ingressed: 5_000u64,
			block_number: 5_000u64,
			close_block: 5_000u64,
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
		started_at_state(pre_finalize, _post, state: DepositChannels) {
			assert_eq!(
				pre_finalize.unsynchronised_state_map,
				state.to_state_map(),
				"Expected state map incorrect before finalization."
			);
		},
		ended_at_state(_pre, post_finalize, state: DepositChannels) {
			assert_eq!(
				post_finalize.unsynchronised_state_map,
				state.to_state_map(),
				"Expected state map incorrect after finalization."
			);
		},
		ingressed(_pre, _post, expected_ingressed: Vec<(AccountId, Asset, Amount)>) {
			AMOUNT_INGRESSED.with(|ingresses| {
				assert_eq!(
					ingresses.clone().into_inner(),
					expected_ingressed,
					"Amount ingressed incorrect."
				);
			});
		},
		channel_closed(_pre, _post, expected_closed_channels: Vec<AccountId>) {
			CHANNELS_CLOSED.with(|channels| {
				assert_eq!(
					channels.clone().into_inner(),
					expected_closed_channels,
					"Channels closed incorrect."
				);
			});
		},
	}
}

#[test]
fn trigger_ingress_on_consensus() {
	with_setup(None)
		.build_with_initial_election()
		.force_consensus_update(ConsensusStatus::Gained {
			most_recent: None,
			new: channel_state_ingressed().to_state(),
		})
		.test_on_finalize(
			&900,
			|_| (),
			vec![
				Check::started_at_state(initial_channel_state()),
				Check::ended_at_state(channel_state_ingressed()),
				Check::ingressed(vec![
					(1u32, Asset::Sol, 1_000u64),
					(2u32, Asset::SolUsdc, 2_000u64),
				]),
			],
		)
		.force_consensus_update(ConsensusStatus::Gained {
			most_recent: None,
			new: channel_state_final().to_state(),
		})
		.test_on_finalize(
			&950,
			|_| (),
			vec![
				Check::started_at_state(channel_state_ingressed()),
				Check::ended_at_state(channel_state_final()),
				Check::ingressed(vec![
					(1u32, Asset::Sol, 1_000u64),
					(2u32, Asset::SolUsdc, 2_000u64),
					(1u32, Asset::Sol, 1_000u64),
					(2u32, Asset::SolUsdc, 2_000u64),
				]),
			],
		);
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
		.test_on_finalize(
			&799,
			|_| (),
			vec![
				Check::started_at_state(initial_channel_state()),
				Check::ingressed(vec![(1u32, Asset::Sol, 1_000u64)]),
			],
		)
		.test_on_finalize(
			&800,
			|_| (),
			vec![
				Check::ended_at_state(channel_state_ingressed()),
				Check::ingressed(vec![
					(1u32, Asset::Sol, 1_000u64),
					(2u32, Asset::SolUsdc, 2_000u64),
				]),
			],
		);
}

#[test]
fn can_close_channels() {
	with_setup(None)
		.build_with_initial_election()
		.force_consensus_update(ConsensusStatus::Gained {
			most_recent: None,
			new: channel_state_closed().to_state(),
		})
		.test_on_finalize(&1000, |_| (), vec![Check::channel_closed(vec![1u32])])
		.force_consensus_update(ConsensusStatus::Gained {
			most_recent: None,
			new: channel_state_closed().to_state(),
		})
		.test_on_finalize(&2000, |_| (), vec![Check::channel_closed(vec![1u32, 2u32])]);
}

#[test]
fn test_deposit_channel_recycling() {
	with_setup(None)
		.build()
		.then(|| {
			for deposit_channel in initial_channel_state().into_inner() {
				assert_ok!(DeltaBasedIngress::open_channel::<MockAccess<SimpleDeltaBasedIngress>>(
					TestContext::<SimpleDeltaBasedIngress>::identifiers(),
					deposit_channel.account,
					deposit_channel.asset,
					deposit_channel.close_block
				));
			}
		})
		.force_consensus_update(ConsensusStatus::Gained {
			most_recent: None,
			new: channel_state_closed().to_state(),
		})
		.test_on_finalize(
			&2000,
			|_| (),
			vec![
				Check::ended_at_state(channel_state_closed()),
				Check::ingressed(vec![
					(1u32, Asset::Sol, 2_000u64),
					(2u32, Asset::SolUsdc, 4_000u64),
				]),
				Check::channel_closed(vec![1u32, 2u32]),
			],
		)
		.then(|| {
			// Channels are recycled using the same asset. Only the difference in total amount is
			// counted as ingressed
			for deposit_channel in channel_state_recycled_same_asset().into_inner() {
				assert_ok!(DeltaBasedIngress::open_channel::<MockAccess<SimpleDeltaBasedIngress>>(
					TestContext::<SimpleDeltaBasedIngress>::identifiers(),
					deposit_channel.account,
					deposit_channel.asset,
					deposit_channel.close_block
				));
			}
		})
		.force_consensus_update(ConsensusStatus::Gained {
			most_recent: None,
			new: channel_state_recycled_same_asset().to_state(),
		})
		.test_on_finalize(
			&4000,
			|_| (),
			vec![
				Check::ended_at_state(channel_state_recycled_same_asset()),
				Check::ingressed(vec![
					(1u32, Asset::Sol, 2_000u64),
					(2u32, Asset::SolUsdc, 4_000u64),
					// On recycled channels, only the diff amount is counted as ingress
					(1u32, Asset::Sol, 6_000u64),
					(2u32, Asset::SolUsdc, 6_000u64),
				]),
				Check::channel_closed(vec![1u32, 2u32, 1u32, 2u32]),
			],
		)
		.then(|| {
			// Channels are recycled using the same asset. Only the difference in total amount is
			// counted as ingressed
			for deposit_channel in channel_state_recycled_different_asset().into_inner() {
				assert_ok!(DeltaBasedIngress::open_channel::<MockAccess<SimpleDeltaBasedIngress>>(
					TestContext::<SimpleDeltaBasedIngress>::identifiers(),
					deposit_channel.account,
					deposit_channel.asset,
					deposit_channel.close_block
				));
			}
		})
		.force_consensus_update(ConsensusStatus::Gained {
			most_recent: None,
			new: channel_state_recycled_different_asset().to_state(),
		})
		.test_on_finalize(
			&5000,
			|_| (),
			vec![
				Check::ended_at_state(
					channel_state_recycled_different_asset()
						.combine(channel_state_recycled_same_asset()),
				),
				Check::ingressed(vec![
					(1u32, Asset::Sol, 2_000u64),
					(2u32, Asset::SolUsdc, 4_000u64),
					(1u32, Asset::Sol, 6_000u64),
					(2u32, Asset::SolUsdc, 6_000u64),
					// Total amount ingressed are accumulated per `(Account, Asset)` pair
					(1u32, Asset::SolUsdc, 4_000u64),
					(2u32, Asset::Sol, 5_000u64),
				]),
				Check::channel_closed(vec![1u32, 2u32, 1u32, 2u32, 1u32, 2u32]),
			],
		);
}
