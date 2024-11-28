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

fn to_state(
	channels: Vec<DepositChannel>,
) -> BTreeMap<AccountId, ChannelTotalIngressedFor<MockIngressSink>> {
	channels
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

fn to_state_map(
	channels: Vec<DepositChannel>,
) -> BTreeMap<(AccountId, Asset), ChannelTotalIngressedFor<MockIngressSink>> {
	channels
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

fn to_properties(
	channels: Vec<DepositChannel>,
) -> BTreeMap<
	AccountId,
	(OpenChannelDetailsFor<MockIngressSink>, ChannelTotalIngressedFor<MockIngressSink>),
> {
	channels
		.into_iter()
		.map(|channel| {
			(
				channel.account,
				(
					OpenChannelDetails { asset: channel.asset, close_block: channel.close_block },
					ChannelTotalIngressed {
						amount: channel.total_ingressed,
						block_number: channel.block_number,
					},
				),
			)
		})
		.collect::<BTreeMap<_, _>>()
}

fn initial_channel_state() -> Vec<DepositChannel> {
	vec![
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
	]
}
fn channel_state_ingressed() -> Vec<DepositChannel> {
	vec![
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
	]
}

fn channel_state_final() -> Vec<DepositChannel> {
	vec![
		DepositChannel {
			account: 1u32,
			asset: Asset::Sol,
			total_ingressed: 4_000u64,
			block_number: 900u64,
			close_block: 1_000u64,
		},
		DepositChannel {
			account: 2u32,
			asset: Asset::SolUsdc,
			total_ingressed: 6_000u64,
			block_number: 900u64,
			close_block: 2_000u64,
		},
	]
}

fn channel_state_closed() -> Vec<DepositChannel> {
	vec![
		DepositChannel {
			account: 1u32,
			asset: Asset::Sol,
			total_ingressed: 4_000u64,
			block_number: 1_000u64,
			close_block: 1_000u64,
		},
		DepositChannel {
			account: 2u32,
			asset: Asset::SolUsdc,
			total_ingressed: 6_000u64,
			block_number: 2_000u64,
			close_block: 2_000u64,
		},
	]
}

fn with_setup(
	initial_elections: Option<Vec<DepositChannel>>,
) -> TestSetup<SimpleDeltaBasedIngress> {
	let initial_elections = initial_elections.unwrap_or(initial_channel_state());
	TestSetup::<_>::default()
		.with_initial_election_state(
			1u32,
			to_properties(initial_elections.clone()),
			to_state(initial_elections.clone()),
		)
		.with_initial_state_map(to_state_map(initial_elections).into_iter().collect::<Vec<_>>())
}

register_checks! {
	SimpleDeltaBasedIngress {
		started_at_state(pre_finalize, _post, state: Vec<DepositChannel>) {
			assert_eq!(
				pre_finalize.unsynchronised_state_map,
				to_state_map(state),
				"Expected state map incorrect before finalization."
			);
		},
		ended_at_state(_pre, post_finalize, state: Vec<DepositChannel>) {
			assert_eq!(
				post_finalize.unsynchronised_state_map,
				to_state_map(state),
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
	let ingressed_block = 900;
	with_setup(None)
		.build_with_initial_election()
		.force_consensus_update(ConsensusStatus::Gained {
			most_recent: None,
			new: to_state(channel_state_ingressed()),
		})
		.test_on_finalize(
			&ingressed_block,
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
			new: to_state(channel_state_final()),
		})
		.test_on_finalize(
			&ingressed_block,
			|_| (),
			vec![
				Check::started_at_state(channel_state_ingressed()),
				Check::ended_at_state(channel_state_final()),
				Check::ingressed(vec![
					(1u32, Asset::Sol, 1_000u64),
					(2u32, Asset::SolUsdc, 2_000u64),
					(1u32, Asset::Sol, 3_000u64),
					(2u32, Asset::SolUsdc, 4_000u64),
				]),
			],
		);
}

#[test]
fn only_trigger_ingress_on_witnessed_blocks() {
	let ingress_block = channel_state_ingressed()
		.into_iter()
		.map(|channel| channel.block_number)
		.collect::<Vec<_>>();
	with_setup(None)
		.build_with_initial_election()
		.force_consensus_update(ConsensusStatus::Gained {
			most_recent: None,
			new: to_state(channel_state_ingressed()),
		})
		.test_on_finalize(&(ingress_block[0] - 1), |_| (), vec![Check::assert_unchanged()])
		.test_on_finalize(
			&(ingress_block[1] - 1),
			|_| (),
			vec![
				Check::started_at_state(initial_channel_state()),
				Check::ingressed(vec![(1u32, Asset::Sol, 1_000u64)]),
			],
		)
		.test_on_finalize(
			&ingress_block[1],
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
	let channel_close_block = channel_state_closed()
		.into_iter()
		.map(|channel| channel.close_block)
		.collect::<Vec<_>>();
	with_setup(None)
		.build_with_initial_election()
		.force_consensus_update(ConsensusStatus::Gained {
			most_recent: None,
			new: to_state(channel_state_closed()),
		})
		.test_on_finalize(&channel_close_block[0], |_| (), vec![Check::channel_closed(vec![1u32])])
		.force_consensus_update(ConsensusStatus::Gained {
			most_recent: None,
			new: to_state(channel_state_closed()),
		})
		.test_on_finalize(
			&channel_close_block[1],
			|_| (),
			vec![Check::channel_closed(vec![1u32, 2u32])],
		);
}

#[test]
fn test_deposit_channel_recycling() {
	let channel_state_recycled_same_asset = vec![
		DepositChannel {
			account: 1u32,
			asset: Asset::Sol,
			total_ingressed: 20_000u64,
			block_number: 4_000u64,
			close_block: 4_000u64,
		},
		DepositChannel {
			account: 2u32,
			asset: Asset::SolUsdc,
			total_ingressed: 30_000u64,
			block_number: 4_000u64,
			close_block: 4_000u64,
		},
	];

	let channel_state_recycled_different_asset = vec![
		DepositChannel {
			account: 1u32,
			asset: Asset::SolUsdc,
			total_ingressed: 100_000u64,
			block_number: 5_000u64,
			close_block: 5_000u64,
		},
		DepositChannel {
			account: 2u32,
			asset: Asset::Sol,
			total_ingressed: 200_000u64,
			block_number: 5_000u64,
			close_block: 5_000u64,
		},
	];

	let initial_close_block = channel_state_closed()[1].close_block;
	let recycled_same_asset_close_block = channel_state_recycled_same_asset[0].close_block;
	let recycled_diff_asset_close_block = channel_state_recycled_different_asset[0].close_block;

	with_setup(None)
		.build()
		.then(|| {
			for deposit_channel in initial_channel_state() {
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
			new: to_state(channel_state_closed()),
		})
		.test_on_finalize(
			&initial_close_block,
			|_| (),
			vec![
				Check::ended_at_state(channel_state_closed()),
				Check::ingressed(vec![
					(1u32, Asset::Sol, 4_000u64),
					(2u32, Asset::SolUsdc, 6_000u64),
				]),
				Check::channel_closed(vec![1u32, 2u32]),
			],
		)
		.then(|| {
			// Channels are recycled using the same asset. Only the difference in total amount is
			// counted as ingressed
			for deposit_channel in channel_state_recycled_same_asset.clone() {
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
			new: to_state(channel_state_recycled_same_asset.clone()),
		})
		.test_on_finalize(
			&recycled_same_asset_close_block,
			|_| (),
			vec![
				Check::ended_at_state(channel_state_recycled_same_asset.clone()),
				Check::ingressed(vec![
					(1u32, Asset::Sol, 4_000u64),
					(2u32, Asset::SolUsdc, 6_000u64),
					// On recycled channels, only the diff amount is counted as ingress
					(1u32, Asset::Sol, 16_000u64),
					(2u32, Asset::SolUsdc, 24_000u64),
				]),
				Check::channel_closed(vec![1u32, 2u32, 1u32, 2u32]),
			],
		)
		.then(|| {
			// Channels are recycled using the same asset. Only the difference in total amount is
			// counted as ingressed
			for deposit_channel in channel_state_recycled_different_asset.clone() {
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
			new: to_state(channel_state_recycled_different_asset.clone()),
		})
		.test_on_finalize(
			&recycled_diff_asset_close_block,
			|_| (),
			vec![
				Check::ended_at_state(
					[channel_state_recycled_different_asset, channel_state_recycled_same_asset]
						.into_iter()
						.flatten()
						.collect(),
				),
				Check::ingressed(vec![
					(1u32, Asset::Sol, 4_000u64),
					(2u32, Asset::SolUsdc, 6_000u64),
					(1u32, Asset::Sol, 16_000u64),
					(2u32, Asset::SolUsdc, 24_000u64),
					// Total amount ingressed are accumulated per `(Account, Asset)` pair
					(1u32, Asset::SolUsdc, 100_000u64),
					(2u32, Asset::Sol, 200_000u64),
				]),
				Check::channel_closed(vec![1u32, 2u32, 1u32, 2u32, 1u32, 2u32]),
			],
		);
}

#[test]
fn do_nothing_on_revert() {
	let channel_state_reverted = vec![
		DepositChannel {
			account: 1u32,
			asset: Asset::Sol,
			total_ingressed: 500u64,
			block_number: 950u64,
			close_block: 1_000u64,
		},
		DepositChannel {
			account: 2u32,
			asset: Asset::SolUsdc,
			total_ingressed: 500u64,
			block_number: 950u64,
			close_block: 2_000u64,
		},
	];
	let ingress_block = channel_state_ingressed()[1].block_number;
	let revert_block = channel_state_reverted[0].block_number;
	let close_block = channel_state_reverted[1].close_block;

	with_setup(None)
		.build_with_initial_election()
		.force_consensus_update(ConsensusStatus::Gained {
			most_recent: None,
			new: to_state(channel_state_ingressed()),
		})
		.test_on_finalize(
			&ingress_block,
			|_| (),
			vec![
				Check::ended_at_state(channel_state_ingressed()),
				Check::ingressed(vec![
					(1u32, Asset::Sol, 1_000u64),
					(2u32, Asset::SolUsdc, 2_000u64),
				]),
			],
		)
		.force_consensus_update(ConsensusStatus::Gained {
			most_recent: None,
			new: to_state(channel_state_reverted),
		})
		.test_on_finalize(
			&revert_block,
			|_| (),
			vec![
				// No new ingress is expected.
				Check::ingressed(vec![
					(1u32, Asset::Sol, 1_000u64),
					(2u32, Asset::SolUsdc, 2_000u64),
				]),
			],
		)
		.force_consensus_update(ConsensusStatus::Gained {
			most_recent: None,
			new: to_state(channel_state_closed()),
		})
		.test_on_finalize(
			&close_block,
			|_| (),
			vec![
				Check::channel_closed(vec![1u32, 2u32]),
				Check::ingressed(vec![
					(1u32, Asset::Sol, 1_000u64),
					(2u32, Asset::SolUsdc, 2_000u64),
					// Reverts are ignored. Delta is calculated from states before reverts.
					(1u32, Asset::Sol, 3_000u64),
					(2u32, Asset::SolUsdc, 4_000u64),
				]),
			],
		);
}
