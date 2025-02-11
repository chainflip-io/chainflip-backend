use super::{mocks::*, register_checks};
use crate::{
	electoral_system::ConsensusStatus, electoral_systems::blockchain::delta_based_ingress::*,
	ElectionIdentifier, UniqueMonotonicIdentifier,
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

const DEFAULT_CHANNEL_ACCOUNT: u32 = 1234;
const DEFAULT_CHANNEL_OPEN_BLOCK: BlockNumber = 100;
const DEFAULT_CHANNEL_CLOSE_BLOCK: BlockNumber = 200;
const DEFAULT_CHANNEL: DepositChannel = DepositChannel {
	account: DEFAULT_CHANNEL_ACCOUNT,
	asset: Asset::Sol,
	total_ingressed: 0,
	block_number: DEFAULT_CHANNEL_OPEN_BLOCK,
	close_block: DEFAULT_CHANNEL_CLOSE_BLOCK,
};

impl Default for DepositChannel {
	fn default() -> Self {
		DEFAULT_CHANNEL
	}
}

impl DepositChannel {
	pub fn open(&self) {
		assert_ok!(DeltaBasedIngress::open_channel::<MockAccess<SimpleDeltaBasedIngress>>(
			TestContext::<SimpleDeltaBasedIngress>::identifiers(),
			self.account,
			self.asset,
			self.close_block
		));
	}
}

fn to_state(
	channels: impl IntoIterator<Item = DepositChannel>,
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
	channels: impl IntoIterator<Item = DepositChannel>,
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
	channels: impl IntoIterator<Item = DepositChannel>,
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

const INITIAL_CHANNEL_STATE: [DepositChannel; 2] = [
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
];
const CHANNEL_STATE_INGRESSED: [DepositChannel; 2] = [
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
];

const CHANNEL_STATE_FINAL: [DepositChannel; 2] = [
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
];

const CHANNEL_STATE_CLOSED: [DepositChannel; 2] = [
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
];

fn with_default_setup() -> TestSetup<SimpleDeltaBasedIngress> {
	TestSetup::<_>::default()
		.with_initial_election_state(
			1u32,
			to_properties(INITIAL_CHANNEL_STATE),
			to_state(INITIAL_CHANNEL_STATE),
		)
		.with_initial_state_map(to_state_map(INITIAL_CHANNEL_STATE).into_iter().collect::<Vec<_>>())
}

impl TestContext<SimpleDeltaBasedIngress> {
	#[track_caller]
	fn assert_state_update(
		self,
		chain_tracking: &BlockNumber,
		channels: impl IntoIterator<Item = DepositChannel>,
		expected_state: impl IntoIterator<Item = DepositChannel>,
	) -> Self {
		self.force_consensus_update(ConsensusStatus::Gained {
			most_recent: None,
			new: to_state(channels),
		})
		.test_on_finalize(
			chain_tracking,
			|_| (),
			[Check::ingressed(vec![]), Check::ended_at_state(to_state(expected_state))],
		)
	}
}

register_checks! {
	SimpleDeltaBasedIngress {
		started_at_state_map_state(pre_finalize, _post, state: impl Clone + IntoIterator<Item = DepositChannel> + 'static) {
			assert_eq!(
				pre_finalize.unsynchronised_state_map,
				to_state_map(state),
				"Expected state map incorrect before finalization."
			);
		},
		ended_at_state_map_state(_pre, post_finalize, state: impl Clone + IntoIterator<Item = DepositChannel> + 'static) {
			assert_eq!(
				post_finalize.unsynchronised_state_map,
				to_state_map(state),
				"Expected state map incorrect after finalization."
			);
		},
		ended_at_state(_pre, post, election_state: BTreeMap<AccountId, ChannelTotalIngressedFor<MockIngressSink>>) {
			assert_eq!(
				*post.election_state.get(post.election_identifiers[0].unique_monotonic()).unwrap(),
				election_state,
				"Expected election state incorrect. Expected {:?}, got: {:?}",
				election_state,
				*post.election_state.get(post.election_identifiers[0].unique_monotonic()).unwrap()
			);
		},
		ingressed(_pre, _post, expected_ingressed: Vec<(AccountId, Asset, Amount)>) {
			AMOUNT_INGRESSED.with(|ingresses| {
				assert_eq!(
					ingresses.clone().into_inner(),
					expected_ingressed,
					"Unexpected ingresses. Expected {:?}, got {:?}", expected_ingressed, ingresses.clone().into_inner()
				);
			});
		},
		channels_closed_matches(_pre, _post, expected_closed_channels: Vec<AccountId>) {
			CHANNELS_CLOSED.with(|channels| {
				assert!(
					*channels.borrow() == expected_closed_channels,
					"Channels closed incorrect: expected {:?}, got {:?}", expected_closed_channels, channels.clone().into_inner()
				);
			});
		},
		channel_closed_once(_pre, _post, expected_closed: AccountId) {
			CHANNELS_CLOSED.with(|channels| {
				assert!(
					channels.borrow().iter().filter(|c| **c == expected_closed).count() == 1,
					"Channels closed incorrect: expected {:?}, got {:?}", expected_closed, channels.clone().into_inner()
				);
			});
		},
		channel_not_closed(_pre, _post, expected_closed: AccountId) {
			CHANNELS_CLOSED.with(|channels| {
				assert!(
					!channels.borrow().contains(&expected_closed),
					"Expected {:?} to be open, but is contained in closed channels: {:?}",
					expected_closed,
					channels.clone().into_inner()
				);
			});
		}
	}
}

#[test]
fn trigger_ingress_on_consensus() {
	let ingressed_block = 900;
	with_default_setup()
		.build_with_initial_election()
		.force_consensus_update(ConsensusStatus::Gained {
			most_recent: None,
			new: to_state(CHANNEL_STATE_INGRESSED),
		})
		.test_on_finalize(
			&ingressed_block,
			|_| (),
			vec![
				Check::started_at_state_map_state(INITIAL_CHANNEL_STATE),
				Check::ended_at_state_map_state(CHANNEL_STATE_INGRESSED),
				Check::ingressed(vec![
					(1u32, Asset::Sol, 1_000u64),
					(2u32, Asset::SolUsdc, 2_000u64),
				]),
			],
		)
		.force_consensus_update(ConsensusStatus::Gained {
			most_recent: None,
			new: to_state(CHANNEL_STATE_FINAL),
		})
		.test_on_finalize(
			&ingressed_block,
			|_| (),
			vec![
				Check::started_at_state_map_state(CHANNEL_STATE_INGRESSED),
				Check::ended_at_state_map_state(CHANNEL_STATE_FINAL),
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
	let ingress_block = CHANNEL_STATE_INGRESSED
		.into_iter()
		.map(|channel| channel.block_number)
		.collect::<Vec<_>>();
	with_default_setup()
		.build_with_initial_election()
		.assert_state_update(
			&(ingress_block[0] - 1),
			CHANNEL_STATE_INGRESSED,
			CHANNEL_STATE_INGRESSED,
		)
		.test_on_finalize(
			&(ingress_block[1] - 1),
			|_| (),
			vec![
				Check::started_at_state_map_state(INITIAL_CHANNEL_STATE),
				Check::ended_at_state(to_state([CHANNEL_STATE_INGRESSED[1]])),
				Check::ingressed(vec![(
					CHANNEL_STATE_INGRESSED[0].account,
					CHANNEL_STATE_INGRESSED[0].asset,
					CHANNEL_STATE_INGRESSED[0].total_ingressed,
				)]),
			],
		)
		.test_on_finalize(
			&ingress_block[1],
			|_| (),
			vec![
				Check::ended_at_state_map_state(CHANNEL_STATE_INGRESSED),
				Check::ingressed(
					CHANNEL_STATE_INGRESSED
						.iter()
						.map(|channel| (channel.account, channel.asset, channel.total_ingressed))
						.collect::<Vec<_>>(),
				),
				Check::ended_at_state(Default::default()),
			],
		);
}

mod channel_closure {
	use super::*;
	const DEPOSIT_BLOCK: BlockNumber = DEFAULT_CHANNEL_OPEN_BLOCK + 10;
	const DEPOSIT_AMOUNT: u64 = 500;

	#[test]
	fn can_close_channels() {
		fn check_closure(
			ctx: TestContext<SimpleDeltaBasedIngress>,
			channel: DepositChannel,
		) -> TestContext<SimpleDeltaBasedIngress> {
			ctx.force_consensus_update(ConsensusStatus::Gained {
				most_recent: None,
				new: [(
					channel.account,
					ChannelTotalIngressed {
						amount: channel.total_ingressed,
						block_number: channel.close_block,
					},
				)]
				.into_iter()
				.collect(),
			})
			.test_on_finalize(
				&channel.close_block,
				|_| (),
				vec![Check::channel_closed_once(channel.account), Check::ingressed(vec![])],
			)
		}

		let channels = [
			DepositChannel { account: 1u32, ..Default::default() },
			DepositChannel {
				account: 2u32,
				close_block: DEFAULT_CHANNEL_CLOSE_BLOCK + 100,
				..Default::default()
			},
		];
		let test_ctx = with_default_setup()
			.build()
			.then(|| channels.iter().for_each(|channel| channel.open()));
		let test_ctx = check_closure(test_ctx, channels[0]);
		let _test_ctx = check_closure(test_ctx, channels[1]);
	}

	fn setup_close_after_deposits(
		chain_tracking_lagging: bool,
	) -> TestContext<SimpleDeltaBasedIngress> {
		let setup = with_default_setup()
			.build()
			.then(|| DEFAULT_CHANNEL.open())
			.force_consensus_update(ConsensusStatus::Gained {
				most_recent: None,
				new: [(
					DEFAULT_CHANNEL_ACCOUNT,
					ChannelTotalIngressed { amount: DEPOSIT_AMOUNT, block_number: DEPOSIT_BLOCK },
				)]
				.into_iter()
				.collect(),
			});

		if chain_tracking_lagging {
			setup
				// Chain tracking is lagging, nothing should be ingressed.
				.test_on_finalize(
					&{ DEPOSIT_BLOCK - 1 },
					|_| (),
					vec![
						Check::ingressed(vec![]),
						Check::ended_at_state(
							[(
								DEFAULT_CHANNEL_ACCOUNT,
								ChannelTotalIngressed {
									amount: DEPOSIT_AMOUNT,
									block_number: DEPOSIT_BLOCK,
								},
							)]
							.into_iter()
							.collect(),
						),
					],
				)
		} else {
			setup
				// Chain tracking is caught up, deposit is ingressed.
				.test_on_finalize(
					&DEPOSIT_BLOCK,
					|_| (),
					vec![
						Check::ingressed(vec![(
							DEFAULT_CHANNEL_ACCOUNT,
							Asset::Sol,
							DEPOSIT_AMOUNT,
						)]),
						Check::ended_at_state([].into_iter().collect()),
					],
				)
		}
		// Engines reach consensus on account closure: total balance is unchanged, block number is
		// the close block.
		.force_consensus_update(ConsensusStatus::Gained {
			most_recent: None,
			new: [(
				DEFAULT_CHANNEL_ACCOUNT,
				ChannelTotalIngressed {
					amount: DEPOSIT_AMOUNT,
					block_number: DEFAULT_CHANNEL_CLOSE_BLOCK,
				},
			)]
			.into_iter()
			.collect(),
		})
	}

	#[test]
	fn close_after_deposit() {
		setup_close_after_deposits(false)
			// Chain tracking reaches close block, channel is closed.
			.test_on_finalize(
				&DEFAULT_CHANNEL_CLOSE_BLOCK,
				|_| (),
				vec![
					Check::channel_closed_once(DEFAULT_CHANNEL_ACCOUNT),
					Check::ingressed(vec![(DEFAULT_CHANNEL_ACCOUNT, Asset::Sol, DEPOSIT_AMOUNT)]),
					Check::all_elections_deleted(),
				],
			);
	}

	#[test]
	fn close_after_deposit_lagging() {
		setup_close_after_deposits(true)
			// Chain tracking reaches close block, channel is closed.
			.test_on_finalize(
				&DEFAULT_CHANNEL_CLOSE_BLOCK,
				|_| (),
				vec![
					Check::channel_closed_once(DEFAULT_CHANNEL_ACCOUNT),
					Check::ingressed(vec![(DEFAULT_CHANNEL_ACCOUNT, Asset::Sol, DEPOSIT_AMOUNT)]),
					Check::all_elections_deleted(),
				],
			);
	}

	// Same as above, except tracking catches up to a block between the deposit and close block.
	#[test]
	fn close_after_deposit_lagging_recovered() {
		setup_close_after_deposits(true)
			// Chain tracking catches up, deposit is ingressed, channel not yet closed.
			.test_on_finalize(
				&{ DEFAULT_CHANNEL_CLOSE_BLOCK - 1 },
				|_| (),
				vec![
					Check::channel_not_closed(DEFAULT_CHANNEL_ACCOUNT),
					Check::ingressed(vec![(DEFAULT_CHANNEL_ACCOUNT, Asset::Sol, DEPOSIT_AMOUNT)]),
					Check::election_id_updated_by(|id| {
						ElectionIdentifier::new(*id.unique_monotonic(), id.extra() + 1)
					}),
					// Channel state not cleaned up yet, since the channel is not yet closed.
					Check::ended_at_state_map_state([DepositChannel {
						total_ingressed: DEPOSIT_AMOUNT,
						block_number: DEPOSIT_BLOCK,
						..DEFAULT_CHANNEL
					}]),
				],
			)
			// Chain tracking reaches close block, channel is closed.
			.test_on_finalize(
				&DEFAULT_CHANNEL_CLOSE_BLOCK,
				|_| (),
				vec![
					Check::channel_closed_once(DEFAULT_CHANNEL_ACCOUNT),
					Check::ingressed(vec![(DEFAULT_CHANNEL_ACCOUNT, Asset::Sol, DEPOSIT_AMOUNT)]),
					Check::all_elections_deleted(),
				],
			);
	}
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

	let initial_close_block = CHANNEL_STATE_CLOSED[1].close_block;
	let recycled_same_asset_close_block = channel_state_recycled_same_asset[0].close_block;
	let recycled_diff_asset_close_block = channel_state_recycled_different_asset[0].close_block;

	with_default_setup()
		.build()
		.then(|| {
			for deposit_channel in INITIAL_CHANNEL_STATE {
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
			new: to_state(CHANNEL_STATE_CLOSED),
		})
		.test_on_finalize(
			&initial_close_block,
			|_| (),
			vec![
				Check::ended_at_state_map_state(CHANNEL_STATE_CLOSED),
				Check::ingressed(vec![
					(1u32, Asset::Sol, 4_000u64),
					(2u32, Asset::SolUsdc, 6_000u64),
				]),
				Check::channels_closed_matches(vec![1u32, 2u32]),
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
				Check::ended_at_state_map_state(channel_state_recycled_same_asset.clone()),
				Check::ingressed(vec![
					(1u32, Asset::Sol, 4_000u64),
					(2u32, Asset::SolUsdc, 6_000u64),
					// On recycled channels, only the diff amount is counted as ingress
					(1u32, Asset::Sol, 16_000u64),
					(2u32, Asset::SolUsdc, 24_000u64),
				]),
				Check::channels_closed_matches(vec![1u32, 2u32, 1u32, 2u32]),
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
				Check::ended_at_state_map_state(
					[channel_state_recycled_different_asset, channel_state_recycled_same_asset]
						.into_iter()
						.flatten(),
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
				Check::channels_closed_matches(vec![1u32, 2u32, 1u32, 2u32, 1u32, 2u32]),
			],
		);
}

#[test]
fn do_nothing_on_revert() {
	const CHANNEL_STATE_REVERTED: [DepositChannel; 2] = [
		DepositChannel {
			total_ingressed: CHANNEL_STATE_INGRESSED[0].total_ingressed - 500u64,
			..CHANNEL_STATE_INGRESSED[0]
		},
		DepositChannel {
			total_ingressed: CHANNEL_STATE_INGRESSED[1].total_ingressed - 1_500u64,
			..CHANNEL_STATE_INGRESSED[1]
		},
	];
	let total_ingress = CHANNEL_STATE_INGRESSED
		.iter()
		.map(|channel| (channel.account, channel.asset, channel.total_ingressed))
		.collect::<Vec<_>>();

	with_default_setup()
		.build()
		.then(|| CHANNEL_STATE_INGRESSED.iter().for_each(|channel| channel.open()))
		.force_consensus_update(ConsensusStatus::Gained {
			most_recent: None,
			new: to_state(CHANNEL_STATE_INGRESSED),
		})
		.test_on_finalize(
			&CHANNEL_STATE_INGRESSED[1].block_number,
			|_| (),
			vec![
				Check::ingressed(total_ingress.clone()),
				Check::ended_at_state_map_state(CHANNEL_STATE_INGRESSED),
			],
		)
		.force_consensus_update(ConsensusStatus::Gained {
			most_recent: None,
			new: to_state(CHANNEL_STATE_REVERTED),
		})
		.test_on_finalize(
			&CHANNEL_STATE_REVERTED[0].block_number,
			|_| (),
			vec![
				// No new ingress is expected.
				Check::ingressed(total_ingress.clone()),
			],
		)
		.force_consensus_update(ConsensusStatus::Gained {
			most_recent: None,
			new: to_state(CHANNEL_STATE_CLOSED),
		})
		.test_on_finalize(
			&CHANNEL_STATE_CLOSED[1].close_block,
			|_| (),
			vec![
				Check::channels_closed_matches(vec![1u32, 2u32]),
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

#[test]
fn test_open_channel_with_existing_election() {
	let channel_close_block = 2_000u64;
	let additional_channels = vec![
		DepositChannel {
			account: 3u32,
			asset: Asset::Sol,
			total_ingressed: 5_000u64,
			block_number: channel_close_block,
			close_block: channel_close_block,
		},
		DepositChannel {
			account: 4u32,
			asset: Asset::SolUsdc,
			total_ingressed: 6_000u64,
			block_number: channel_close_block,
			close_block: channel_close_block,
		},
	];

	let combined_state_closed = CHANNEL_STATE_CLOSED.into_iter().chain(additional_channels.clone());

	with_default_setup()
		.build()
		.then(|| {
			for deposit_channel in INITIAL_CHANNEL_STATE {
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
			new: to_state(CHANNEL_STATE_INGRESSED),
		})
		.test_on_finalize(
			&CHANNEL_STATE_INGRESSED[1].block_number,
			|_| (),
			vec![
				Check::ended_at_state_map_state(CHANNEL_STATE_INGRESSED),
				Check::ingressed(vec![
					(1u32, Asset::Sol, 1_000u64),
					(2u32, Asset::SolUsdc, 2_000u64),
				]),
			],
		)
		.then(|| {
			// Channels are recycled using the same asset. Only the difference in total amount is
			// counted as ingressed
			for deposit_channel in additional_channels.clone() {
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
			new: to_state(combined_state_closed.clone()),
		})
		.test_on_finalize(
			&channel_close_block,
			|_| (),
			vec![
				Check::ended_at_state_map_state(combined_state_closed),
				Check::ingressed(vec![
					(1u32, Asset::Sol, 1_000u64),
					(2u32, Asset::SolUsdc, 2_000u64),
					// All channels can ingress and close correctly.
					(1u32, Asset::Sol, 3_000u64),
					(2u32, Asset::SolUsdc, 4_000u64),
					(3u32, Asset::Sol, 5_000u64),
					(4u32, Asset::SolUsdc, 6_000u64),
				]),
				Check::channels_closed_matches(vec![1u32, 2u32, 3u32, 4u32]),
			],
		);
}

#[test]
fn start_new_election_if_too_many_channels_in_current_election() {
	let channel_close_block = 1_000u64;
	let deposit_channels = (0..MAXIMUM_CHANNELS_PER_ELECTION)
		.map(|i| DepositChannel {
			account: i,
			asset: Asset::Sol,
			total_ingressed: 1_000u64,
			block_number: channel_close_block,
			close_block: channel_close_block,
		})
		.collect::<Vec<_>>();

	with_default_setup().build().then(|| {
		for deposit_channel in deposit_channels.clone() {
			assert_ok!(DeltaBasedIngress::open_channel::<MockAccess<SimpleDeltaBasedIngress>>(
				TestContext::<SimpleDeltaBasedIngress>::identifiers(),
				deposit_channel.account,
				deposit_channel.asset,
				deposit_channel.close_block
			));
		}
		assert_ok!(DeltaBasedIngress::open_channel::<MockAccess<SimpleDeltaBasedIngress>>(
			TestContext::<SimpleDeltaBasedIngress>::identifiers(),
			51u32,
			Asset::Sol,
			channel_close_block,
		));

		assert_eq!(
			TestContext::<SimpleDeltaBasedIngress>::identifiers(),
			vec![
				ElectionIdentifier::new(UniqueMonotonicIdentifier::from_u64(0), 49),
				ElectionIdentifier::new(UniqueMonotonicIdentifier::from_u64(1), 0),
			]
		);
	});
}

#[test]
fn pending_ingresses_update_with_consensus() {
	const CHAIN_TRACKING: BlockNumber = 1_000;
	let deposit_channel_pending = DepositChannel {
		account: 1u32,
		asset: Asset::Sol,
		total_ingressed: 1_000u64,
		block_number: CHAIN_TRACKING + 10,
		close_block: BlockNumber::MAX, // we're not testing closing here
	};
	let deposit_channel_pending_lower_amount = DepositChannel {
		total_ingressed: deposit_channel_pending.total_ingressed - 1,
		..deposit_channel_pending
	};
	let deposit_channel_pending_higher_amount = DepositChannel {
		total_ingressed: deposit_channel_pending.total_ingressed + 1,
		..deposit_channel_pending
	};
	let deposit_channel_pending_lower_block = DepositChannel {
		block_number: deposit_channel_pending.block_number - 1,
		..deposit_channel_pending
	};
	let deposit_channel_pending_higher_block = DepositChannel {
		block_number: deposit_channel_pending.block_number + 1,
		..deposit_channel_pending
	};
	let deposit_channel_with_next_deposit = DepositChannel {
		total_ingressed: 2_500u64,
		block_number: deposit_channel_pending.block_number + 10,
		..deposit_channel_pending
	};

	let test = with_default_setup()
		.build()
		.then(|| deposit_channel_pending.open())
		.assert_state_update(&CHAIN_TRACKING, [deposit_channel_pending], [deposit_channel_pending])
		.assert_state_update(
			&{ CHAIN_TRACKING + 1 },
			[deposit_channel_pending_lower_amount],
			[deposit_channel_pending_lower_amount],
		)
		.assert_state_update(
			&{ CHAIN_TRACKING + 2 },
			[deposit_channel_pending_higher_amount],
			[deposit_channel_pending_higher_amount],
		)
		.assert_state_update(
			&{ CHAIN_TRACKING + 3 },
			[deposit_channel_pending_lower_block],
			[deposit_channel_pending_lower_block],
		)
		.assert_state_update(
			&{ CHAIN_TRACKING + 4 },
			[deposit_channel_pending_higher_block],
			[deposit_channel_pending_higher_block],
		)
		// Trying to push the state to a different amount at a higher block will have no effect.
		.assert_state_update(
			&{ CHAIN_TRACKING + 5 },
			[deposit_channel_with_next_deposit],
			[deposit_channel_pending_higher_block],
		);

	test
		// Once chain tracking advances past the latest consensus value, we process the first
		// deposit.
		.test_on_finalize(
			&(deposit_channel_pending_higher_block.block_number),
			|_| (),
			vec![
				Check::ingressed(vec![(
					deposit_channel_pending.account,
					deposit_channel_pending.asset,
					deposit_channel_pending.total_ingressed,
				)]),
				Check::election_id_updated_by(|id| {
					ElectionIdentifier::new(*id.unique_monotonic(), id.extra() + 1)
				}),
				Check::ended_at_state(to_state(vec![deposit_channel_with_next_deposit])),
			],
		)
		// After the deposit, the latest consensus value will have been promoted to the pending
		// state.
		.test_on_finalize(
			&{ deposit_channel_pending_higher_block.block_number + 1 },
			|_| (),
			vec![
				Check::ingressed(vec![(
					deposit_channel_pending.account,
					deposit_channel_pending.asset,
					deposit_channel_pending.total_ingressed,
				)]),
				Check::ended_at_state(to_state(vec![deposit_channel_with_next_deposit])),
			],
		)
		// Once chain tracking advances past the block of the next deposit, we process it too.
		.test_on_finalize(
			&(deposit_channel_with_next_deposit.block_number),
			|_| (),
			vec![
				Check::ingressed(vec![
					(
						deposit_channel_pending.account,
						deposit_channel_pending.asset,
						deposit_channel_pending.total_ingressed,
					),
					(
						deposit_channel_pending.account,
						deposit_channel_pending.asset,
						deposit_channel_with_next_deposit.total_ingressed -
							deposit_channel_pending.total_ingressed,
					),
				]),
				Check::ended_at_state(Default::default()),
			],
		);
}

mod multiple_deposits {
	use super::*;

	const DEPOSIT_ADDRESS: u32 = 1;
	const TOTAL_1: ChannelTotalIngressed<u64, u64> =
		ChannelTotalIngressed { amount: 1000, block_number: 10 };
	const TOTAL_2: ChannelTotalIngressed<u64, u64> =
		ChannelTotalIngressed { amount: 1500, block_number: 20 };

	#[test]
	fn multiple_deposits_result_in_multiple_ingresses() {
		// Case 1: Two deposits, and three finality checks:
		// - Check 1: Chain tracking has not reached the block of the first deposit.
		// - Check 2: Chain tracking has not reached the block of the second deposit, but has passed
		//   the first.
		// - Check 3: Chain tracking has reached the block of the second deposit.
		with_default_setup()
			.build()
			.then(|| {
				DepositChannel {
					account: DEPOSIT_ADDRESS,
					asset: Asset::Sol,
					close_block: 100,
					total_ingressed: 0,
					block_number: 0,
				}
				.open();
			})
			.force_consensus_update(ConsensusStatus::Gained {
				most_recent: None,
				new: BTreeMap::from_iter([(DEPOSIT_ADDRESS, TOTAL_1)]),
			})
			// Before chain tracking reaches the ingress block, nothing should be ingressed.
			.test_on_finalize(
				&{ TOTAL_1.block_number - 1 },
				|_| {},
				[
					Check::ingressed(vec![]),
					Check::election_id_updated_by(|id| {
						ElectionIdentifier::new(*id.unique_monotonic(), id.extra() + 1)
					}),
				],
			)
			// Simulate a second deposit at a later block.
			.force_consensus_update(ConsensusStatus::Gained {
				most_recent: Some(BTreeMap::from_iter([(DEPOSIT_ADDRESS, TOTAL_1)])),
				new: BTreeMap::from_iter([(DEPOSIT_ADDRESS, TOTAL_2)]),
			})
			// Finalize with chain tracking at a block between the two deposits. Only the first
			// should be ingressed.
			.test_on_finalize(
				&{ TOTAL_2.block_number - 1 },
				|_| {},
				[
					Check::ingressed(vec![(DEPOSIT_ADDRESS, Asset::Sol, TOTAL_1.amount)]),
					Check::election_id_updated_by(|id| {
						ElectionIdentifier::new(*id.unique_monotonic(), id.extra() + 1)
					}),
				],
			)
			// Finalize with chain tracking at the block of the second deposit. Both should be
			// ingressed.
			.test_on_finalize(
				&TOTAL_2.block_number,
				|_| {},
				[Check::ingressed(vec![
					(DEPOSIT_ADDRESS, Asset::Sol, TOTAL_1.amount),
					(DEPOSIT_ADDRESS, Asset::Sol, TOTAL_2.amount - TOTAL_1.amount),
				])],
			);
	}

	#[test]
	fn multiple_deposits_result_in_single_deposit() {
		// Case 2: Two deposits and three finality checks:
		// - Check 1: Chain tracking has not reached the block of the first deposit.
		// - Check 2: Chain tracking has still not reached the block of the first deposit.
		// - Check 3: Chain tracking has reached the block of the second deposit.
		with_default_setup()
			.build()
			.then(|| {
				DepositChannel {
					account: DEPOSIT_ADDRESS,
					asset: Asset::Sol,
					close_block: 100,
					total_ingressed: 0,
					block_number: 0,
				}
				.open();
			})
			.force_consensus_update(ConsensusStatus::Gained {
				most_recent: None,
				new: BTreeMap::from_iter([(DEPOSIT_ADDRESS, TOTAL_1)]),
			})
			// Before chain tracking reaches the ingress block, nothing should be ingressed.
			.test_on_finalize(&{ TOTAL_1.block_number - 1 }, |_| {}, [Check::ingressed(vec![])])
			// Simulate a second deposit at a later block.
			.force_consensus_update(ConsensusStatus::Gained {
				most_recent: Some(BTreeMap::from_iter([(DEPOSIT_ADDRESS, TOTAL_1)])),
				new: BTreeMap::from_iter([(DEPOSIT_ADDRESS, TOTAL_2)]),
			})
			// Finalize with chain tracking at a block before the first deposit. Nothing should be
			// ingressed.
			.test_on_finalize(&{ TOTAL_1.block_number - 1 }, |_| {}, [Check::ingressed(vec![])])
			// Finalize with chain tracking at the block of the second deposit. Both should be
			// ingressed as a single deposit.
			.test_on_finalize(
				&TOTAL_2.block_number,
				|_| {},
				[Check::ingressed(vec![(DEPOSIT_ADDRESS, Asset::Sol, TOTAL_2.amount)])],
			);
	}
}
