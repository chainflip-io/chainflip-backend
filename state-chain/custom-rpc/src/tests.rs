use crate::*;

pub mod account_info;
pub mod before_v7;
pub mod eip712;

use cf_amm::math::{Price, SqrtPriceQ64F96};
use cf_rpc_apis::{
	broker::{SwapDepositAddress, WithdrawFeesDetail},
	lp::{
		LimitOrRangeOrder, LiquidityDepositChannelDetails, RangeOrderChange, SwapRequestResponse,
	},
	OrderFilled, RefundParametersRpc, SwapChannelInfo,
};
use codec::Encode;
use pallet_cf_lending_pools::{LtvThresholds, NetworkFeeContributions, OwedAmount};
use pallet_cf_pools::{
	IncreaseOrDecrease, LimitOrder, LimitOrderLiquidity, PoolOrder, RangeOrder,
	RangeOrderLiquidity, UnidirectionalSubPoolDepth,
};
use pallet_cf_swapping::FeeRateAndMinimum;
use pallet_cf_validator::{DelegationAcceptance, OperatorSettings};
use std::collections::BTreeSet;

use cf_chains::{
	address::EncodedAddress,
	assets::sol,
	btc::{deposit_address::DepositAddress, ScriptPubkey, Utxo, UtxoId},
	ccm_checker::{DecodedCcmAdditionalData, VersionedSolanaCcmAdditionalData},
	dot::PolkadotAccountId,
	sol::{
		SolAddress, SolAddressLookupTableAccount, SolApiEnvironment, SolCcmAccounts, SolCcmAddress,
		SolPubkey, VaultSwapOrDepositChannelId,
	},
	Arbitrum, Bitcoin, CcmAdditionalData, CcmChannelMetadataChecked, Ethereum,
	EvmVaultSwapExtraParameters, ForeignChainAddress, Solana,
};

use cf_primitives::{
	chains::assets::{any, arb, btc, dot, eth, hub},
	ApiWaitForResult, AssetAndAmount, Beneficiary, PrewitnessedDepositId, FLIPPERINOS_PER_FLIP,
};

use state_chain_runtime::{
	runtime_apis::types::{
		BrokerRejectionEventFor, ChannelActionType, EvmCallDetails, LendingPosition,
		NetworkFeeDetails, OpenedDepositChannels,
	},
	Runtime,
};

use sp_core::{H160, H256};
use sp_runtime::{AccountId32, FixedU64};

/*
	changing any of these serialization tests signifies a breaking change in the
	API. please make sure to get approval from the product team before merging
	any changes that break a serialization test.

	if approval is received and a new breaking change is introduced, please
	stale the review and get a new review from someone on product.
*/

const ID_1: AccountId32 = AccountId32::new([1; 32]);
const ID_2: AccountId32 = AccountId32::new([2; 32]);

fn asset_map<T: Clone>(v: T) -> any::AssetMap<T> {
	any::AssetMap {
		eth: eth::AssetMap { eth: v.clone(), usdc: v.clone(), flip: v.clone(), usdt: v.clone() },
		btc: btc::AssetMap { btc: v.clone() },
		dot: dot::AssetMap { dot: v.clone() },
		arb: arb::AssetMap { eth: v.clone(), usdc: v.clone() },
		sol: sol::AssetMap { sol: v.clone(), usdc: v.clone() },
		hub: hub::AssetMap { dot: v.clone(), usdc: v.clone(), usdt: v },
	}
}

fn ccm_checked() -> CcmChannelMetadataChecked {
	CcmChannelMetadataChecked {
		message: vec![124u8, 29u8, 15u8, 7u8].try_into().unwrap(),
		gas_budget: 0u128,
		ccm_additional_data: DecodedCcmAdditionalData::Solana(
			VersionedSolanaCcmAdditionalData::V0(SolCcmAccounts {
				cf_receiver: SolCcmAddress { pubkey: SolPubkey([0x10; 32]), is_writable: true },
				additional_accounts: vec![SolCcmAddress {
					pubkey: SolPubkey([0x11; 32]),
					is_writable: false,
				}],
				fallback_address: SolPubkey([0x12; 32]),
			}),
		),
	}
}

fn ccm_unchecked() -> CcmChannelMetadataUnchecked {
	CcmChannelMetadataUnchecked {
		message: vec![124u8, 29u8, 15u8, 7u8].try_into().unwrap(),
		gas_budget: 0u128,
		ccm_additional_data: CcmAdditionalData::try_from(
			VersionedSolanaCcmAdditionalData::V1 {
				ccm_accounts: SolCcmAccounts {
					cf_receiver: SolCcmAddress { pubkey: SolPubkey([0x10; 32]), is_writable: true },
					additional_accounts: vec![SolCcmAddress {
						pubkey: SolPubkey([0x11; 32]),
						is_writable: false,
					}],
					fallback_address: SolPubkey([0x12; 32]),
				},
				alts: vec![SolAddress([0x13; 32]), SolAddress([0x14; 32])],
			}
			.encode(),
		)
		.unwrap(),
	}
}

#[test]
fn test_environment_serialization() {
	#[expect(deprecated)]
	let env = RpcEnvironment {
		swapping: SwappingEnvironment {
			maximum_swap_amounts: any::AssetMap {
				eth: eth::AssetMap {
					eth: Some(0u32.into()),
					flip: None,
					usdc: Some((u64::MAX / 2 - 1).into()),
					usdt: None,
				},
				btc: btc::AssetMap { btc: Some(0u32.into()) },
				dot: dot::AssetMap { dot: None },
				arb: arb::AssetMap { eth: None, usdc: Some(0u32.into()) },
				sol: sol::AssetMap { sol: None, usdc: None },
				hub: hub::AssetMap { dot: None, usdc: None, usdt: None },
			},
			network_fee_hundredth_pips: Permill::from_percent(100),
			swap_retry_delay_blocks: 5,
			max_swap_retry_duration_blocks: 600,
			max_swap_request_duration_blocks: 14400,
			minimum_chunk_size: any::AssetMap {
				eth: eth::AssetMap {
					eth: 123_u32.into(),
					flip: 0u32.into(),
					usdc: 456_u32.into(),
					usdt: 0u32.into(),
				},
				btc: btc::AssetMap { btc: 789_u32.into() },
				dot: dot::AssetMap { dot: 0u32.into() },
				arb: arb::AssetMap { eth: 0u32.into(), usdc: 101112_u32.into() },
				sol: sol::AssetMap { sol: 0u32.into(), usdc: 0u32.into() },
				hub: hub::AssetMap { dot: 0u32.into(), usdc: 0u32.into(), usdt: 0u32.into() },
			},
			network_fees: NetworkFees {
				regular_network_fee: NetworkFeeDetails {
					standard_rate_and_minimum: FeeRateAndMinimum {
						rate: Permill::from_perthousand(1),
						minimum: 100_u32.into(),
					},
					rates: any::AssetMap {
						eth: eth::AssetMap {
							eth: Permill::from_perthousand(20),
							flip: Permill::from_perthousand(10),
							usdc: Permill::from_perthousand(1),
							usdt: Permill::from_perthousand(1),
						},
						btc: btc::AssetMap { btc: Permill::from_perthousand(40) },
						dot: dot::AssetMap { dot: Permill::from_perthousand(50) },
						arb: arb::AssetMap {
							eth: Permill::from_perthousand(1),
							usdc: Permill::from_perthousand(1),
						},
						sol: sol::AssetMap {
							sol: Permill::from_perthousand(1),
							usdc: Permill::from_perthousand(1),
						},
						hub: hub::AssetMap {
							dot: Permill::from_perthousand(1),
							usdc: Permill::from_perthousand(1),
							usdt: Permill::from_perthousand(1),
						},
					},
				},
				internal_swap_network_fee: NetworkFeeDetails {
					standard_rate_and_minimum: FeeRateAndMinimum {
						rate: Permill::from_perthousand(20),
						minimum: 200_u32.into(),
					},
					rates: any::AssetMap {
						eth: eth::AssetMap {
							eth: Permill::from_perthousand(20),
							flip: Permill::from_perthousand(20),
							usdc: Permill::from_perthousand(420),
							usdt: Permill::from_perthousand(249),
						},
						btc: btc::AssetMap { btc: Permill::from_perthousand(20) },
						dot: dot::AssetMap { dot: Permill::from_perthousand(20) },
						arb: arb::AssetMap {
							eth: Permill::from_perthousand(20),
							usdc: Permill::from_perthousand(123),
						},
						sol: sol::AssetMap {
							sol: Permill::from_perthousand(20),
							usdc: Permill::from_perthousand(456),
						},
						hub: hub::AssetMap {
							dot: Permill::from_perthousand(20),
							usdc: Permill::from_perthousand(789),
							usdt: Permill::from_perthousand(101),
						},
					},
				},
			},
		},
		ingress_egress: IngressEgressEnvironment {
			minimum_deposit_amounts: any::AssetMap {
				eth: eth::AssetMap {
					eth: 0u32.into(),
					flip: u64::MAX.into(),
					usdc: (u64::MAX / 2 - 1).into(),
					usdt: 0u32.into(),
				},
				btc: btc::AssetMap { btc: 0u32.into() },
				dot: dot::AssetMap { dot: 0u32.into() },
				arb: arb::AssetMap { eth: 0u32.into(), usdc: u64::MAX.into() },
				sol: sol::AssetMap { sol: 0u32.into(), usdc: 0u32.into() },
				hub: hub::AssetMap { dot: 0u32.into(), usdc: 0u32.into(), usdt: 0u32.into() },
			},
			ingress_fees: any::AssetMap {
				eth: eth::AssetMap {
					eth: Some(0u32.into()),
					flip: Some(AssetAmount::MAX.into()),
					usdc: None,
					usdt: None,
				},
				btc: btc::AssetMap { btc: Some(0u32.into()) },
				dot: dot::AssetMap { dot: Some((u64::MAX / 2 - 1).into()) },
				arb: arb::AssetMap { eth: Some(0u32.into()), usdc: None },
				sol: sol::AssetMap { sol: Some(0u32.into()), usdc: None },
				hub: hub::AssetMap { dot: Some((u64::MAX / 2 - 1).into()), usdc: None, usdt: None },
			},
			egress_fees: any::AssetMap {
				eth: eth::AssetMap {
					eth: Some(0u32.into()),
					usdc: None,
					flip: Some(AssetAmount::MAX.into()),
					usdt: None,
				},
				btc: btc::AssetMap { btc: Some(0u32.into()) },
				dot: dot::AssetMap { dot: Some((u64::MAX / 2 - 1).into()) },
				arb: arb::AssetMap { eth: Some(0u32.into()), usdc: None },
				sol: sol::AssetMap { sol: Some(1u32.into()), usdc: None },
				hub: hub::AssetMap { dot: Some((u64::MAX / 2 - 1).into()), usdc: None, usdt: None },
			},
			witness_safety_margins: HashMap::from([
				(ForeignChain::Bitcoin, Some(3u64)),
				(ForeignChain::Ethereum, Some(3u64)),
				(ForeignChain::Polkadot, None),
				(ForeignChain::Arbitrum, None),
				(ForeignChain::Solana, None),
				(ForeignChain::Assethub, None),
			]),
			egress_dust_limits: any::AssetMap {
				eth: eth::AssetMap {
					eth: 0u32.into(),
					usdc: (u64::MAX / 2 - 1).into(),
					flip: AssetAmount::MAX.into(),
					usdt: 0u32.into(),
				},
				btc: btc::AssetMap { btc: 0u32.into() },
				dot: dot::AssetMap { dot: 0u32.into() },
				arb: arb::AssetMap { eth: 0u32.into(), usdc: u64::MAX.into() },
				sol: sol::AssetMap { sol: 0u32.into(), usdc: 0u32.into() },
				hub: hub::AssetMap { dot: 0u32.into(), usdc: 0u32.into(), usdt: 0u32.into() },
			},
			channel_opening_fees: HashMap::from([
				(ForeignChain::Bitcoin, 0u32.into()),
				(ForeignChain::Ethereum, 1000u32.into()),
				(ForeignChain::Polkadot, 1000u32.into()),
				(ForeignChain::Arbitrum, 1000u32.into()),
				(ForeignChain::Solana, 1000u32.into()),
				(ForeignChain::Assethub, 1000u32.into()),
			]),
			ingress_delays: HashMap::from([
				(ForeignChain::Bitcoin, 0u32),
				(ForeignChain::Ethereum, 5u32),
				(ForeignChain::Polkadot, 2u32),
				(ForeignChain::Arbitrum, 5u32),
				(ForeignChain::Solana, 123u32),
				(ForeignChain::Assethub, 2u32),
			]),
			boost_delays: HashMap::from([
				(ForeignChain::Bitcoin, 0u32),
				(ForeignChain::Ethereum, 5u32),
				(ForeignChain::Polkadot, 0u32),
				(ForeignChain::Arbitrum, 0u32),
				(ForeignChain::Solana, 456u32),
				(ForeignChain::Assethub, 2u32),
			]),
		},
		funding: FundingEnvironment {
			redemption_tax: 0u32.into(),
			minimum_funding_amount: 0u32.into(),
		},
		pools: {
			let pool_info: RpcPoolInfo = PoolInfo {
				limit_order_fee_hundredth_pips: 0,
				range_order_fee_hundredth_pips: 100,
				range_order_total_fees_earned: Default::default(),
				limit_order_total_fees_earned: Default::default(),
				range_total_swap_inputs: Default::default(),
				limit_total_swap_inputs: Default::default(),
			}
			.into();
			PoolsEnvironment { fees: asset_map(Some(pool_info)) }
		},
	};

	insta::assert_snapshot!(serde_json::to_value(env).unwrap());
}

#[test]
fn test_boost_depth_serialization() {
	let val: BoostPoolDepthResponse = vec![
		BoostPoolDepth {
			asset: Asset::Flip,
			tier: 10,
			available_amount: 1_000_000_000 * FLIPPERINOS_PER_FLIP,
		},
		BoostPoolDepth { asset: Asset::Flip, tier: 30, available_amount: 0 },
	];
	insta::assert_json_snapshot!(val);
}

fn boost_details_1() -> BoostPoolDetails<AccountId32> {
	BoostPoolDetails {
		available_amounts: BTreeMap::from([(ID_1.clone(), 10_000)]),
		pending_boosts: BTreeMap::from([
			(
				PrewitnessedDepositId(0),
				BTreeMap::from([
					(ID_1.clone(), OwedAmount { total: 200, fee: 10 }),
					(ID_2.clone(), OwedAmount { total: 2_000, fee: 100 }),
				]),
			),
			(
				PrewitnessedDepositId(1),
				BTreeMap::from([(ID_1.clone(), OwedAmount { total: 1_000, fee: 50 })]),
			),
		]),
		pending_withdrawals: Default::default(),
		network_fee_deduction_percent: Percent::from_percent(40),
	}
}

fn boost_details_2() -> BoostPoolDetails<AccountId32> {
	BoostPoolDetails {
		available_amounts: BTreeMap::from([]),
		pending_boosts: BTreeMap::from([(
			PrewitnessedDepositId(0),
			BTreeMap::from([
				(ID_1.clone(), OwedAmount { total: 1_000, fee: 50 }),
				(ID_2.clone(), OwedAmount { total: 2_000, fee: 100 }),
			]),
		)]),
		pending_withdrawals: BTreeMap::from([
			(ID_1.clone(), BTreeSet::from([PrewitnessedDepositId(0)])),
			(ID_2.clone(), BTreeSet::from([PrewitnessedDepositId(0)])),
		]),
		network_fee_deduction_percent: Percent::from_percent(0),
	}
}

#[test]
fn test_boost_details_serialization() {
	let val: BoostPoolDetailsResponse = vec![
		BoostPoolDetailsRpc::new(Asset::ArbEth, 10, boost_details_1()),
		BoostPoolDetailsRpc::new(Asset::Btc, 30, boost_details_2()),
	];

	insta::assert_json_snapshot!(val);
}

#[test]
fn test_boost_fees_serialization() {
	let val: BoostPoolFeesResponse = vec![BoostPoolFeesRpc::new(Asset::Btc, 10, boost_details_1())];

	insta::assert_json_snapshot!(val);
}

#[test]
fn test_swap_output_serialization() {
	insta::assert_snapshot!(serde_json::to_value(RpcSwapOutputV2 {
		output: 1_000_000_000_000_000_000u128.into(),
		intermediary: Some(1_000_000u128.into()),
		network_fee: RpcFee { asset: Asset::Usdc, amount: 1_000u128.into() },
		ingress_fee: RpcFee { asset: Asset::Flip, amount: 500u128.into() },
		egress_fee: RpcFee { asset: Asset::Eth, amount: 1_000_000u128.into() },
		broker_commission: RpcFee { asset: Asset::Usdc, amount: 100u128.into() },
	})
	.unwrap());
}

#[test]
fn test_vault_addresses_custom_rpc() {
	let val: VaultAddresses = VaultAddresses {
		ethereum: EncodedAddress::Eth([0; 20]),
		arbitrum: EncodedAddress::Arb([1; 20]),
		bitcoin: vec![(ID_1.clone(), EncodedAddress::Btc(Vec::new()))],
		sol_swap_endpoint_program_data_account: EncodedAddress::Sol([2; 32]),
		usdc_token_mint_pubkey: EncodedAddress::Sol([3; 32]),
		sol_vault_program: EncodedAddress::Sol([4; 32]),
		bitcoin_vault: Some(EncodedAddress::Btc([5; 32].into())),
		solana_sol_vault: Some(EncodedAddress::Sol([6; 32])),
		solana_usdc_token_vault_ata: EncodedAddress::Sol([7; 32]),
		solana_vault_swap_account: Some(EncodedAddress::Sol([8; 32])),
		predicted_seconds_until_next_vault_rotation: 9,
	};
	insta::assert_json_snapshot!(val);
}

#[test]
fn swap_output_v2_serialization() {
	insta::assert_snapshot!(serde_json::to_value(RpcSwapOutputV2 {
		output: 1_000_000_000_000_000_000u128.into(),
		intermediary: Some(1_000_000u128.into()),
		network_fee: RpcFee { asset: Asset::Usdc, amount: 1_000u128.into() },
		ingress_fee: RpcFee { asset: Asset::Flip, amount: 500u128.into() },
		egress_fee: RpcFee { asset: Asset::Eth, amount: 1_000_000u128.into() },
		broker_commission: RpcFee { asset: Asset::Usdc, amount: 100u128.into() },
	})
	.unwrap());
}

#[test]
fn test_trading_strategies_custom_rpc() {
	use pallet_cf_trading_strategy::TradingStrategy;

	let val = TradingStrategyInfoHexAmounts {
		lp_id: ID_1,
		strategy_id: ID_2,
		strategy: TradingStrategy::TickZeroCentered { spread_tick: 1, base_asset: Asset::Usdt },
		balance: vec![(Asset::Usdc, 500u128.into()), (Asset::Usdt, 1_000u128.into())],
	};
	insta::assert_json_snapshot!(val);
}

#[test]
fn number_or_hex_number_serialization() {
	let num = NumberOrHex::Number(100);
	let hex = NumberOrHex::Hex(123456789u128.into());
	insta::assert_json_snapshot!(num);
	insta::assert_json_snapshot!(hex);
}

#[test]
fn asset_map_serialization() {
	let val: AssetMap<U256> = asset_map(500.into());

	insta::assert_json_snapshot!(val);
}

#[test]
fn suspensions_serialization() {
	let val = vec![
		(Offence::FailedToBroadcastTransaction, vec![(1u32, ID_1), (2u32, ID_2)]),
		(Offence::ParticipateKeyHandoverFailed, vec![(3u32, ID_1)]),
		(Offence::ParticipateSigningFailed, vec![(4u32, ID_2)]),
	];

	insta::assert_json_snapshot!(val);
}

#[test]
fn auction_state_serialization() {
	let val = RpcAuctionState {
		epoch_duration: 1u32,
		current_epoch_started_at: 2u32,
		redemption_period_as_percentage: 3u8,
		min_funding: NumberOrHex::Number(4u64),
		min_bid: NumberOrHex::Number(500u64),
		auction_size_range: (5u32, 6u32),
		min_active_bid: Some(NumberOrHex::Number(7u64)),
	};

	insta::assert_json_snapshot!(val);
}

#[test]
fn pool_price_v1_serialization() {
	let val = PoolPriceV1 {
		price: Price::from_raw_into(12345678u128),
		sqrt_price: SqrtPriceQ64F96::from_raw_into(87654321u128),
		tick: -100i32,
	};

	insta::assert_json_snapshot!(val);
}

#[test]
fn pool_price_v2_serialization() {
	let val = PoolPriceV2 {
		base_asset: any::Asset::Eth,
		quote_asset: any::Asset::Usdc,
		price: pallet_cf_pools::PoolPriceV2 {
			sell: Some(SqrtPriceQ64F96::from_raw_into(1234567u128)),
			buy: Some(SqrtPriceQ64F96::from_raw_into(1234567u128)),
			range_order: SqrtPriceQ64F96::from_raw_into(1234567u128),
		},
	};

	insta::assert_json_snapshot!(val);
}

#[test]
fn swap_output_v1_serialization() {
	let val = RpcSwapOutputV1 {
		intermediary: Some(NumberOrHex::Number(12345u64)),
		output: NumberOrHex::Number(54321u64),
	};

	insta::assert_json_snapshot!(val);
}

#[test]
fn pool_pairs_map_serialization() {
	let val = PoolPairsMap::<AmmAmount> { base: 12345678u128.into(), quote: 87654321u128.into() };

	insta::assert_json_snapshot!(val);
}

#[test]
fn pool_order_book_serialization() {
	let val = PoolOrderbook {
		bids: vec![PoolOrder {
			amount: 12345678u128.into(),
			sqrt_price: SqrtPriceQ64F96::from_raw_into(87654321u128),
		}],
		asks: vec![PoolOrder {
			amount: 23456789u128.into(),
			sqrt_price: SqrtPriceQ64F96::from_raw_into(98765432u128),
		}],
	};

	insta::assert_json_snapshot!(val);
}

#[test]
fn pool_info_serialization() {
	let val = PoolInfo {
		limit_order_fee_hundredth_pips: 100u32,
		range_order_fee_hundredth_pips: 200u32,
		range_order_total_fees_earned: PoolPairsMap { base: 1111.into(), quote: 2222.into() },
		limit_order_total_fees_earned: PoolPairsMap { base: 3333.into(), quote: 4444.into() },
		range_total_swap_inputs: PoolPairsMap { base: 5555.into(), quote: 6666.into() },
		limit_total_swap_inputs: PoolPairsMap { base: 7777.into(), quote: 8888.into() },
	};

	insta::assert_json_snapshot!(val);
}

#[test]
fn ask_bid_map_serialization() {
	let val = AskBidMap::<UnidirectionalPoolDepth> {
		asks: UnidirectionalPoolDepth {
			limit_orders: UnidirectionalSubPoolDepth {
				price: Some(SqrtPriceQ64F96::from_raw_into(123456)),
				depth: 654321.into(),
			},
			range_orders: UnidirectionalSubPoolDepth {
				price: Some(SqrtPriceQ64F96::from_raw_into(234567)),
				depth: 765432.into(),
			},
		},
		bids: UnidirectionalPoolDepth {
			limit_orders: UnidirectionalSubPoolDepth {
				price: Some(SqrtPriceQ64F96::from_raw_into(345678)),
				depth: 876543.into(),
			},
			range_orders: UnidirectionalSubPoolDepth {
				price: Some(SqrtPriceQ64F96::from_raw_into(456789)),
				depth: 987654.into(),
			},
		},
	};

	insta::assert_json_snapshot!(val);
}

#[test]
fn pool_liquidity_serialization() {
	let val = PoolLiquidity {
		limit_orders: AskBidMap {
			asks: vec![LimitOrderLiquidity { tick: -100i32, amount: 123456.into() }],
			bids: vec![LimitOrderLiquidity { tick: -200i32, amount: 234567.into() }],
		},
		range_orders: vec![RangeOrderLiquidity { tick: -300i32, liquidity: 345678.into() }],
	};

	insta::assert_json_snapshot!(val);
}

#[test]
fn pool_orders_serialization() {
	let val = PoolOrders::<Runtime> {
		limit_orders: AskBidMap {
			asks: vec![LimitOrder {
				lp: ID_1,
				id: 123456.into(),
				tick: -100,
				sell_amount: 234567.into(),
				fees_earned: 345678.into(),
				original_sell_amount: 456789.into(),
			}],
			bids: vec![LimitOrder {
				lp: ID_1,
				id: 654321.into(),
				tick: -200,
				sell_amount: 765432.into(),
				fees_earned: 876543.into(),
				original_sell_amount: 987654.into(),
			}],
		},
		range_orders: vec![RangeOrder {
			lp: ID_2,
			id: 13579.into(),
			range: Range { start: -400, end: 400 },
			liquidity: 24680u128,
			fees_earned: PoolPairsMap { base: 97531.into(), quote: 86420.into() },
		}],
	};

	insta::assert_json_snapshot!(val);
}

#[test]
fn semver_serialization() {
	let val = SemVer { major: 1u8, minor: 23u8, patch: 35u8 };

	insta::assert_json_snapshot!(val);
}

#[test]
fn block_update_serialization() {
	let val = BlockUpdate::<OrderFills> {
		block_hash: sp_core::H256([0xff; 32]),
		block_number: 100u32,
		timestamp: 1233456u64,
		data: OrderFills {
			fills: vec![
				OrderFilled::LimitOrder {
					lp: ID_1,
					base_asset: Asset::Sol,
					quote_asset: Asset::Usdc,
					side: Side::Buy,
					id: 123456.into(),
					tick: -100,
					sold: 23456.into(),
					bought: 23456.into(),
					fees: 123.into(),
					remaining: 456.into(),
				},
				OrderFilled::RangeOrder {
					lp: ID_2,
					base_asset: Asset::ArbEth,
					quote_asset: Asset::Usdc,
					id: 234567.into(),
					range: Range { start: -100, end: 100 },
					bought_amounts: PoolPairsMap { base: 54321.into(), quote: 65432.into() },
					fees: PoolPairsMap { base: 65432.into(), quote: 76543.into() },
					liquidity: 567890.into(),
				},
			],
		},
	};

	insta::assert_json_snapshot!(val);
}

#[test]
fn scheduled_swap_serialization() {
	let val = ScheduledSwap {
		swap_id: SwapId(1u64),
		swap_request_id: SwapRequestId(10u64),
		base_asset: Asset::Btc,
		quote_asset: Asset::Usdc,
		side: Side::Sell,
		amount: 12345.into(),
		source_asset: Some(Asset::Btc),
		source_amount: Some(12345.into()),
		execute_at: 31u32,
		remaining_chunks: 42u32,
		chunk_interval: 10u32,
	};

	insta::assert_json_snapshot!(val);
}

#[test]
fn eth_transaction_serialization() {
	let val = cf_chains::evm::Transaction {
		chain_id: 100u64,
		max_priority_fee_per_gas: Some(123456.into()),
		max_fee_per_gas: Some(234567.into()),
		gas_limit: Some(345678.into()),
		contract: H160([0xee; 20]),
		value: 456789.into(),
		data: vec![0x00, 0x01, 0x02, 0x03, 0x04],
	};

	insta::assert_json_snapshot!(val);
}

#[test]
fn arb_transaction_serialization() {
	let val = cf_chains::evm::Transaction {
		chain_id: 100u64,
		max_priority_fee_per_gas: Some(123456.into()),
		max_fee_per_gas: Some(234567.into()),
		gas_limit: Some(345678.into()),
		contract: H160([0xee; 20]),
		value: 456789.into(),
		data: vec![0x00, 0x01, 0x02, 0x03, 0x04],
	};

	insta::assert_json_snapshot!(val);
}

#[test]
fn failing_witness_validators_serialization() {
	let val = FailingWitnessValidators {
		failing_count: 100u32,
		validators: vec![
			(ID_1, "Romantic<>Robot".to_string(), true),
			(ID_2, "[Waldo.The.Unfounded]".to_string(), false),
		],
	};

	insta::assert_json_snapshot!(val);
}

#[test]
fn runtime_safe_mode_serialization() {
	let val = RuntimeSafeMode::default();

	insta::assert_json_snapshot!(val);
}

#[test]
fn vault_swap_details_serialization() {
	let btc = VaultSwapDetails::<AddressString>::Bitcoin {
		nulldata_payload: vec![0x00, 0x01, 0x02, 0x03],
		deposit_address: "1Pc9wMdCRguVVrZ9Paz8MSgUd47cYfyeQH".to_string().into(),
	};
	let eth = VaultSwapDetails::<AddressString>::Ethereum {
		details: EvmCallDetails {
			calldata: vec![0x11, 0x22, 0x33, 0x44],
			value: 12345678.into(),
			to: H160([0xdd; 20]),
			source_token_address: Some(H160([0xee; 20])),
		},
	};
	let arb = VaultSwapDetails::<AddressString>::Arbitrum {
		details: EvmCallDetails {
			calldata: vec![0x11, 0x22, 0x33, 0x44],
			value: 2345678.into(),
			to: H160([0xcc; 20]),
			source_token_address: Some(H160([0xbb; 20])),
		},
	};
	let sol = VaultSwapDetails::<AddressString>::Solana {
		instruction: cf_chains::sol::instruction_builder::SolanaInstructionBuilder::x_swap_native(
			SolApiEnvironment {
				vault_program: SolAddress([0x00; 32]),
				vault_program_data_account: SolAddress([0x00; 32]),
				token_vault_pda_account: SolAddress([0x00; 32]),
				usdc_token_mint_pubkey: SolAddress([0x00; 32]),
				usdc_token_vault_ata: SolAddress([0x00; 32]),
				swap_endpoint_program: SolAddress([0x00; 32]),
				swap_endpoint_program_data_account: SolAddress([0x00; 32]),
				alt_manager_program: SolAddress([0x00; 32]),
				address_lookup_table_account: SolAddressLookupTableAccount {
					key: SolPubkey([0x00; 32]),
					addresses: vec![SolPubkey([0x00; 32])],
				},
			},
			SolPubkey([0xf0; 32]),
			Asset::SolUsdc,
			EncodedAddress::Sol([0xf1; 32]),
			SolPubkey([0xf2; 32]),
			vec![0xf3; 32].try_into().unwrap(),
			SolPubkey([0xf3; 32]),
			1_000_000u64,
			vec![0x00, 0x01, 0x02, 0x03, 0x04, 0x05],
			Some(ccm_checked()),
		)
		.into(),
	};

	insta::assert_json_snapshot!(btc);
	insta::assert_json_snapshot!(eth);
	insta::assert_json_snapshot!(arb);
	insta::assert_json_snapshot!(sol);
}

#[test]
fn vault_swap_input_serialization() {
	let refund_parameter = RefundParametersRpc {
		retry_duration: 1000u32,
		refund_address: "E2aBDC008BaEa1d4Dd2eeE4f0BEa61f6f91897cC".to_string().into(),
		min_price: Price::from_raw_into(1234),
		refund_ccm_metadata: Some(ccm_unchecked()),
		max_oracle_price_slippage: Some(200u16),
	};

	let affiliate_fees: Affiliates<cf_primitives::AccountId> =
		vec![Beneficiary { account: ID_1, bps: 100u16 }].try_into().unwrap();
	let dca_parameter = Some(DcaParameters { number_of_chunks: 100u32, chunk_interval: 10u32 });
	let eth = VaultSwapInputRpc {
		source_asset: Asset::Eth,
		destination_asset: Asset::Sol,
		destination_address: "8RMUMRxniKbs9kMDVb81RtWoLNz2zesz3JQLRZXZc5kh".to_string().into(),
		broker_commission: 100u16,
		extra_parameters: VaultSwapExtraParametersRpc::Ethereum(EvmVaultSwapExtraParameters {
			input_amount: NumberOrHex::Number(1_000_000u64),
			refund_parameters: refund_parameter.clone(),
		}),
		channel_metadata: Some(ccm_unchecked()),
		boost_fee: 100u16,
		affiliate_fees: affiliate_fees.clone(),
		dca_parameters: dca_parameter.clone(),
	};

	let arb = VaultSwapInputRpc {
		source_asset: Asset::ArbEth,
		destination_asset: Asset::Sol,
		destination_address: "8RMUMRxniKbs9kMDVb81RtWoLNz2zesz3JQLRZXZc5kh".to_string().into(),
		broker_commission: 100u16,
		extra_parameters: VaultSwapExtraParametersRpc::Arbitrum(EvmVaultSwapExtraParameters {
			input_amount: NumberOrHex::Number(1_000_000u64),
			refund_parameters: refund_parameter.clone(),
		}),
		channel_metadata: Some(ccm_unchecked()),
		boost_fee: 100u16,
		affiliate_fees: affiliate_fees.clone(),
		dca_parameters: dca_parameter.clone(),
	};

	let btc = VaultSwapInputRpc {
		source_asset: Asset::Btc,
		destination_asset: Asset::Sol,
		destination_address: "8RMUMRxniKbs9kMDVb81RtWoLNz2zesz3JQLRZXZc5kh".to_string().into(),
		broker_commission: 100u16,
		extra_parameters: VaultSwapExtraParametersRpc::Bitcoin {
			min_output_amount: NumberOrHex::Number(100_000_000u64),
			retry_duration: 100u32,
			max_oracle_price_slippage: Some(200u8),
		},
		channel_metadata: Some(ccm_unchecked()),
		boost_fee: 100u16,
		affiliate_fees: affiliate_fees.clone(),
		dca_parameters: dca_parameter.clone(),
	};

	let sol = VaultSwapInputRpc {
		source_asset: Asset::Sol,
		destination_asset: Asset::SolUsdc,
		destination_address: "8RMUMRxniKbs9kMDVb81RtWoLNz2zesz3JQLRZXZc5kh".to_string().into(),
		broker_commission: 100u16,
		extra_parameters: VaultSwapExtraParametersRpc::Solana {
			from: "1Pc9wMdCRguVVrZ9Paz8MSgUd47cYfyeQH".to_string().into(),
			seed: vec![0xf1; 32].try_into().unwrap(),
			input_amount: NumberOrHex::Number(1_000_000u64),
			refund_parameters: refund_parameter.clone(),
			from_token_account: Some(
				"8RMUMRxniKbs9kMDVb81RtWoLNz2zesz3JQLRZXZc5kh".to_string().into(),
			),
		},
		channel_metadata: Some(ccm_unchecked()),
		boost_fee: 100u16,
		affiliate_fees: affiliate_fees.clone(),
		dca_parameters: dca_parameter.clone(),
	};

	insta::assert_json_snapshot!(eth);
	insta::assert_json_snapshot!(arb);
	insta::assert_json_snapshot!(btc);
	insta::assert_json_snapshot!(sol);
}

#[test]
fn chain_accounts_serialization() {
	let val = ChainAccounts {
		chain_accounts: vec![
			(
				ForeignChainAddress::Eth(cf_chains::evm::Address::from([1u8; 20]))
					.to_encoded_address(Default::default()),
				Asset::Eth,
			),
			(
				ForeignChainAddress::Dot(PolkadotAccountId([2u8; 32]))
					.to_encoded_address(Default::default()),
				Asset::Dot,
			),
			(
				ForeignChainAddress::Btc(ScriptPubkey::P2WPKH([3u8; 20]))
					.to_encoded_address(Default::default()),
				Asset::Btc,
			),
			(
				ForeignChainAddress::Btc(ScriptPubkey::Taproot([4u8; 32]))
					.to_encoded_address(Default::default()),
				Asset::Btc,
			),
			(
				ForeignChainAddress::Arb(cf_chains::evm::Address::from([5u8; 20]))
					.to_encoded_address(Default::default()),
				Asset::ArbEth,
			),
			(
				ForeignChainAddress::Sol(SolAddress([6u8; 32]))
					.to_encoded_address(Default::default()),
				Asset::Sol,
			),
			(
				ForeignChainAddress::Hub(PolkadotAccountId([7u8; 32]))
					.to_encoded_address(Default::default()),
				Asset::HubDot,
			),
		],
	};

	insta::assert_json_snapshot!(val);
}

#[test]
fn transaction_screening_events_serialization() {
	let val = TransactionScreeningEvents {
		btc_events: vec![
			BrokerRejectionEventFor::<Bitcoin>::TransactionRejectionRequestReceived {
				account_id: ID_1,
				tx_id: H256([0xe0; 32]),
			},
			BrokerRejectionEventFor::<Bitcoin>::TransactionRejectionRequestExpired {
				account_id: ID_2,
				tx_id: H256([0xe1; 32]),
			},
			BrokerRejectionEventFor::<Bitcoin>::TransactionRejectedByBroker {
				refund_broadcast_id: 3u32,
				deposit_details: Utxo {
					id: UtxoId { tx_id: H256([0xe3; 32]), vout: 7 },
					amount: 1_000_000,
					deposit_address: DepositAddress::new([0xe2; 32], 1),
				},
			},
		],
		eth_events: vec![
			BrokerRejectionEventFor::<Ethereum>::TransactionRejectionRequestReceived {
				account_id: ID_1,
				tx_id: H256([0xe0; 32]),
			},
			BrokerRejectionEventFor::<Ethereum>::TransactionRejectionRequestExpired {
				account_id: ID_2,
				tx_id: H256([0xe1; 32]),
			},
			BrokerRejectionEventFor::<Ethereum>::TransactionRejectedByBroker {
				refund_broadcast_id: 3u32,
				deposit_details: cf_chains::evm::DepositDetails {
					tx_hashes: Some(vec![H256([0xe2; 32])]),
				},
			},
		],
		arb_events: vec![
			BrokerRejectionEventFor::<Arbitrum>::TransactionRejectionRequestReceived {
				account_id: ID_1,
				tx_id: H256([0xe0; 32]),
			},
			BrokerRejectionEventFor::<Arbitrum>::TransactionRejectionRequestExpired {
				account_id: ID_2,
				tx_id: H256([0xe1; 32]),
			},
			BrokerRejectionEventFor::<Arbitrum>::TransactionRejectedByBroker {
				refund_broadcast_id: 3u32,
				deposit_details: cf_chains::evm::DepositDetails {
					tx_hashes: Some(vec![H256([0xe2; 32])]),
				},
			},
		],
		sol_events: vec![
			BrokerRejectionEventFor::<Solana>::TransactionRejectionRequestReceived {
				account_id: ID_1,
				tx_id: (SolAddress([0xe1; 32]), 7u64),
			},
			BrokerRejectionEventFor::<Solana>::TransactionRejectionRequestExpired {
				account_id: ID_2,
				tx_id: (SolAddress([0xe2; 32]), 9u64),
			},
			BrokerRejectionEventFor::<Solana>::TransactionRejectedByBroker {
				refund_broadcast_id: 3u32,
				deposit_details: VaultSwapOrDepositChannelId::Channel(SolAddress([0xe3; 32])),
			},
		],
	};

	insta::assert_json_snapshot!(val);
}

#[test]
fn opened_deposit_channels_serialization() {
	let val: Vec<OpenedDepositChannels> = vec![
		(
			ID_1,
			ChannelActionType::LiquidityProvision,
			ChainAccounts { chain_accounts: vec![(EncodedAddress::Eth([0x01; 20]), Asset::Eth)] },
		),
		(
			ID_1,
			ChannelActionType::Swap,
			ChainAccounts { chain_accounts: vec![(EncodedAddress::Sol([0x02; 32]), Asset::Sol)] },
		),
		(
			ID_1,
			ChannelActionType::Refund,
			ChainAccounts { chain_accounts: vec![(EncodedAddress::Eth([0x01; 20]), Asset::Eth)] },
		),
	];

	insta::assert_json_snapshot!(val);
}

#[test]
fn trading_strategy_limits_serialization() {
	let val = TradingStrategyLimits {
		minimum_deployment_amount: asset_map(Some(123456u128)),
		minimum_added_funds_amount: asset_map(Some(654321u128)),
	};

	insta::assert_json_snapshot!(val);
}

#[test]
fn swap_deposit_address_serialization() {
	let val = SwapDepositAddress {
		address: "8RMUMRxniKbs9kMDVb81RtWoLNz2zesz3JQLRZXZc5kh".to_string().into(),
		issued_block: 100u32,
		channel_id: 200u64,
		source_chain_expiry_block: NumberOrHex::Number(100u64),
		channel_opening_fee: 123456.into(),
		refund_parameters: RefundParametersRpc {
			retry_duration: 1000u32,
			refund_address: "E2aBDC008BaEa1d4Dd2eeE4f0BEa61f6f91897cC".to_string().into(),
			min_price: Price::from_raw_into(1234),
			refund_ccm_metadata: Some(ccm_unchecked()),
			max_oracle_price_slippage: Some(200u16),
		},
	};

	insta::assert_json_snapshot!(val);
}

#[test]
fn withdraw_fees_detail_serialization() {
	let val = WithdrawFeesDetail {
		tx_hash: H256([0xf0; 32]),
		egress_id: (ForeignChain::Ethereum, 1u64),
		egress_amount: 123456.into(),
		egress_fee: 234567.into(),
		destination_address: "E2aBDC008BaEa1d4Dd2eeE4f0BEa61f6f91897cC".to_string().into(),
	};

	insta::assert_json_snapshot!(val);
}

#[test]
fn bytes_serialization() {
	let val: RpcBytes = vec![0x00, 0x01, 0x02, 0x03].into();

	insta::assert_json_snapshot!(val);
}

#[test]
fn liquidity_deposit_channel_details_serialization() {
	let val = LiquidityDepositChannelDetails {
		deposit_address: "E2aBDC008BaEa1d4Dd2eeE4f0BEa61f6f91897cC".to_string().into(),
		deposit_chain_expiry_block: 500u64,
	};

	insta::assert_json_snapshot!(val);
}

#[test]
fn limit_or_range_order_serialization() {
	let limit = LimitOrRangeOrder::LimitOrder(cf_rpc_apis::lp::LimitOrder {
		base_asset: Asset::Btc,
		quote_asset: Asset::Usdc,
		side: Side::Buy,
		id: 123456.into(),
		tick: -100,
		sell_amount_total: 23456.into(),
		collected_fees: 34567.into(),
		bought_amount: 45678.into(),
		sell_amount_change: Some(IncreaseOrDecrease::Increase(12345.into())),
	});
	let range = LimitOrRangeOrder::RangeOrder(cf_rpc_apis::lp::RangeOrder {
		base_asset: Asset::Btc,
		quote_asset: Asset::Usdc,
		id: 123456.into(),
		tick_range: Range { start: -100, end: 100 },
		liquidity_total: 123456.into(),
		collected_fees: PoolPairsMap { base: 123456.into(), quote: 23456.into() },
		size_change: Some(IncreaseOrDecrease::Increase(RangeOrderChange {
			liquidity: 123456.into(),
			amounts: PoolPairsMap { base: 123456.into(), quote: 654321.into() },
		})),
	});

	insta::assert_json_snapshot!(limit);
	insta::assert_json_snapshot!(range);
}

#[test]
fn swap_request_response_serialization() {
	let val = SwapRequestResponse { swap_request_id: SwapRequestId(100) };

	insta::assert_json_snapshot!(val);
}

#[test]
fn swap_channel_info_serialization() {
	let val = SwapChannelInfo::<Ethereum> {
		deposit_address: H160([0xee; 20]),
		source_asset: Asset::Eth,
		destination_asset: Asset::Sol,
	};

	insta::assert_json_snapshot!(val);
}

#[test]
fn api_wait_result_serialization() {
	let hash: ApiWaitForResult<AddressString> = ApiWaitForResult::TxHash(H256([0xff; 32]));
	let response = ApiWaitForResult::TxDetails::<AddressString> {
		tx_hash: H256([0xf0; 32]),
		response: "E2aBDC008BaEa1d4Dd2eeE4f0BEa61f6f91897cC".to_string().into(),
	};
	insta::assert_json_snapshot!(hash);
	insta::assert_json_snapshot!(response);
}

use pallet_cf_lending_pools::{InterestRateConfiguration, LendingPoolConfiguration};

#[test]
fn lending_pools_serialization() {
	let pool = RpcLendingPool::<U256> {
		asset: Asset::Usdc,
		total_amount: 2_000u128.into(),
		available_amount: 1_500u128.into(),
		owed_to_network: 155u128.into(),
		utilisation_rate: Permill::from_percent(90),
		current_interest_rate: Permill::from_percent(8),
		config: LendingPoolConfiguration {
			origination_fee: Permill::from_parts(100),
			liquidation_fee: Permill::from_parts(500),
			interest_rate_curve: InterestRateConfiguration {
				interest_at_zero_utilisation: Permill::from_percent(2),
				junction_utilisation: Permill::from_percent(90),
				interest_at_junction_utilisation: Permill::from_percent(8),
				interest_at_max_utilisation: Permill::from_percent(50),
			},
		},
	};

	insta::assert_json_snapshot!(pool);
}

#[test]
fn loan_account_serialization() {
	use cf_traits::lending::LoanId;
	use pallet_cf_lending_pools::{
		LiquidationType, RpcLiquidationStatus, RpcLiquidationSwap, RpcLoan,
	};

	let loan_account = RpcLoanAccount::<_, U256> {
		account: ID_1,
		collateral_topup_asset: Some(Asset::Btc),
		ltv_ratio: Some(FixedU64::from_rational(4, 3)),
		collateral: vec![(AssetAndAmount { asset: Asset::Btc, amount: 3u128.into() })],
		loans: vec![RpcLoan {
			loan_id: LoanId(1),
			asset: Asset::Usdc,
			created_at: 400,
			principal_amount: 1000u128.into(),
		}],
		liquidation_status: Some(RpcLiquidationStatus {
			liquidation_swaps: vec![RpcLiquidationSwap {
				swap_request_id: SwapRequestId(1),
				loan_id: LoanId(1),
			}],
			liquidation_type: LiquidationType::SoftVoluntary,
		}),
	};

	insta::assert_json_snapshot!(loan_account);
}

#[test]
fn lending_supply_positions_serialization() {
	let value = LendingPoolAndSupplyPositions::<AccountId32, U256> {
		asset: Asset::Usdc,
		positions: vec![
			LendingSupplyPosition {
				lp_id: AccountId32::new([0x11; 32]),
				total_amount: 123456.into(),
			},
			LendingSupplyPosition {
				lp_id: AccountId32::new([0x12; 32]),
				total_amount: 234567.into(),
			},
		],
	};

	insta::assert_json_snapshot!(value);
}

#[test]
fn lending_config_serialization() {
	let config = RpcLendingConfig {
		ltv_thresholds: LtvThresholds {
			low_ltv: Permill::from_percent(50),
			target: Permill::from_percent(75),
			topup: Some(Permill::from_percent(80)),
			soft_liquidation: Permill::from_percent(90),
			soft_liquidation_abort: Permill::from_percent(88),
			hard_liquidation: Permill::from_percent(95),
			hard_liquidation_abort: Permill::from_percent(93),
		},
		network_fee_contributions: NetworkFeeContributions {
			extra_interest: Permill::from_percent(1),
			from_origination_fee: Permill::from_percent(20),
			from_liquidation_fee: Permill::from_percent(30),
			low_ltv_penalty_max: Permill::from_percent(50),
		},
		fee_swap_interval_blocks: 10,
		interest_payment_interval_blocks: 15,
		fee_swap_threshold_usd: U256::from(20_000_000),
		interest_collection_threshold_usd: U256::from(2_000_000),
		soft_liquidation_swap_chunk_size_usd: U256::from(5_000_000_000u64),
		hard_liquidation_swap_chunk_size_usd: U256::from(25_000_000_000u64),
		soft_liquidation_max_oracle_slippage: 50,
		hard_liquidation_max_oracle_slippage: 500,
		fee_swap_max_oracle_slippage: 50,
		minimum_loan_amount_usd: U256::from(100_000),
		minimum_supply_amount_usd: U256::from(100_000),
		minimum_update_loan_amount_usd: U256::from(50_000),
		minimum_update_collateral_amount_usd: U256::from(25_000),
	};

	insta::assert_json_snapshot!(config);
}

#[test]
fn operator_info() {
	let value = OperatorInfo::<NumberOrHex> {
		managed_validators: BTreeMap::from([
			(AccountId32::new([0x11; 32]), 1_000_000u128.into()),
			(AccountId32::new([0x22; 32]), 2_000_000u128.into()),
		]),
		settings: OperatorSettings {
			fee_bps: 2_000,
			delegation_acceptance: DelegationAcceptance::Deny,
		},
		allowed: vec![
			AccountId32::new([0x11; 32]),
			AccountId32::new([0x22; 32]),
			AccountId32::new([0x33; 32]),
			AccountId32::new([0x44; 32]),
		],
		blocked: vec![],
		delegators: BTreeMap::from([
			(AccountId32::new([0x33; 32]), 500_000u128.into()),
			(AccountId32::new([0x44; 32]), 1_500_000u128.into()),
		]),
		active_delegation: Some(DelegationSnapshot {
			operator: AccountId32::new([0x55; 32]),
			validators: BTreeMap::from([
				(AccountId32::new([0x11; 32]), 1_000_000u128.into()),
				(AccountId32::new([0x22; 32]), 2_000_000u128.into()),
			]),
			delegators: BTreeMap::from([(AccountId32::new([0x44; 32]), 1_500_000u128.into())]),
			delegation_fee_bps: 2_000,
		}),
	};

	insta::assert_snapshot!(serde_json::to_string_pretty(&value).unwrap());
}
